//! Browser bridge job queue on `devlish --server`.
use serde_json::{json, Value};
use std::fs::{self, File};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Server {
    child: Child,
    dir: PathBuf,
    base: String,
    token: String,
    log_path: PathBuf,
    err_path: PathBuf,
}

impl Server {
    fn start(extra: &[&str]) -> Self {
        Self::start_with(extra, None, None)
    }

    fn start_with(extra: &[&str], serve_token: Option<&str>, policy: Option<&str>) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "devlish-bridge-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let token_path = dir.join("token");
        let log_path = dir.join("jobs.jsonl");
        let err_path = dir.join("server.err");
        fs::write(&token_path, "test-token\n").unwrap();
        let mut extra_owned: Vec<String> = extra.iter().map(|arg| (*arg).to_string()).collect();
        if let Some(policy) = policy {
            let policy_path = dir.join("bridge-policy.dvl");
            fs::write(&policy_path, policy).unwrap();
            extra_owned.push("--bridge-policy".into());
            extra_owned.push(policy_path.to_string_lossy().into_owned());
        }
        let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = socket.local_addr().unwrap();
        drop(socket);
        let err_file = File::create(&err_path).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_devlish-core"));
        command
            .current_dir(&dir)
            .env_remove("DEVLISH_VERIFIED_PROFILE")
            .env_remove("DEVLISH_AUDIT_LOG")
            .env_remove("DEVLISH_DEFAULT_AUTHORIZATION")
            .env_remove("ASIDE_BRIDGE_POLICY")
            .env_remove("ASIDE_BRIDGE_TOKEN_FILE")
            .env_remove("ASIDE_BRIDGE_LOG")
            .env_remove("ASIDE_BRIDGE_RATE_LIMIT")
            .env_remove("ASIDE_BRIDGE_RATE_WINDOW_SECS")
            .env_remove("ASIDE_BRIDGE_IN_PROGRESS_TIMEOUT_SECS")
            .env("HOME", &dir)
            .args([
                "--server",
                "--bind",
                &address.to_string(),
                "--bridge-token-file",
                &token_path.to_string_lossy(),
                "--bridge-log",
                &log_path.to_string_lossy(),
            ])
            .args(&extra_owned)
            .stdout(Stdio::null())
            .stderr(Stdio::from(err_file));
        if let Some(token) = serve_token {
            command.env("DEVLISH_SERVE_TOKEN", token);
        } else {
            command.env_remove("DEVLISH_SERVE_TOKEN");
        }
        let child = command.spawn().unwrap();
        let mut server = Self {
            child,
            dir,
            base: format!("http://{address}"),
            token: "test-token".into(),
            log_path,
            err_path,
        };
        let health = format!("{}/v1/health", server.base);
        for _ in 0..100 {
            if ureq::get(&health)
                .timeout(Duration::from_millis(100))
                .call()
                .is_ok()
            {
                return server;
            }
            if server.child.try_wait().unwrap().is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        panic!(
            "server did not start\n{}",
            fs::read_to_string(&server.err_path).unwrap_or_default()
        );
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn call(
    server: &Server,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> (u16, Value) {
    let url = format!("{}{path}", server.base);
    let mut request = ureq::request(method, &url).timeout(Duration::from_secs(40));
    if let Some(token) = token {
        request = request.set("X-Bridge-Token", token);
    }
    let result = if let Some(body) = body {
        request.send_json(body)
    } else {
        request.call()
    };
    match result {
        Ok(response) => {
            let status = response.status();
            let text = response.into_string().unwrap_or_default();
            let parsed = if text.is_empty() {
                Value::Null
            } else {
                serde_json::from_str(&text).unwrap_or(Value::String(text))
            };
            (status, parsed)
        }
        Err(ureq::Error::Status(status, response)) => {
            let text = response.into_string().unwrap_or_default();
            let parsed = serde_json::from_str(&text).unwrap_or(Value::String(text));
            (status, parsed)
        }
        Err(error) => panic!("request failed: {error}"),
    }
}

fn post(server: &Server, path: &str, body: Value) -> (u16, Value) {
    call(server, "POST", path, Some(&server.token), Some(body))
}

fn get(server: &Server, path: &str) -> (u16, Value) {
    call(server, "GET", path, Some(&server.token), None)
}

fn error_text(body: &Value) -> String {
    body.get("error")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

#[test]
fn queue_happy_path_claims_a_job_and_stores_the_result() {
    let server = Server::start(&[]);
    let args = json!({
        "name": "Lender follow-ups",
        "urls": [
            "https://www.linkedin.com/in/one",
            "https://www.linkedin.com/in/two"
        ]
    });
    let (status, created) = post(
        &server,
        "/bridge/jobs",
        json!({"verb": "tabs.open_group", "args": args}),
    );
    assert_eq!(status, 201, "{created}");
    assert_eq!(created["status"], json!("queued"));
    let id = created["id"].as_str().unwrap();

    let (status, queued) = get(&server, &format!("/bridge/jobs/{id}"));
    assert_eq!(status, 200, "{queued}");
    assert_eq!(queued["verb"], json!("tabs.open_group"));
    assert_eq!(queued["args"], args);
    assert_eq!(queued["status"], json!("queued"));
    assert!(queued["created_at"].as_str().unwrap().ends_with('Z'));
    assert!(queued["updated_at"].as_str().unwrap().ends_with('Z'));

    let (status, claimed) = get(&server, "/bridge/jobs/next?wait=1");
    assert_eq!(status, 200, "{claimed}");
    let mut keys: Vec<_> = claimed.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(
        keys,
        vec!["args".to_string(), "id".to_string(), "verb".to_string()]
    );
    assert_eq!(claimed["id"], json!(id));
    assert_eq!(claimed["args"], args);

    let (status, running) = get(&server, &format!("/bridge/jobs/{id}"));
    assert_eq!(
        running["status"],
        json!("in_progress"),
        "{status} {running}"
    );

    let (status, done) = post(
        &server,
        &format!("/bridge/jobs/{id}/result"),
        json!({"ok": true, "result": {"opened": 2}}),
    );
    assert_eq!(status, 200, "{done}");
    assert_eq!(done["status"], json!("done"));
    assert_eq!(done["result"], json!({"opened": 2}));

    let (status, again) = post(
        &server,
        &format!("/bridge/jobs/{id}/result"),
        json!({"ok": true, "result": {"opened": 2}}),
    );
    assert_eq!(status, 409, "{again}");
    let (status, missing) = post(
        &server,
        "/bridge/jobs/job-missing/result",
        json!({"ok": false, "error": "nope"}),
    );
    assert_eq!(status, 404, "{missing}");

    let log = fs::read_to_string(&server.log_path).unwrap();
    assert!(
        !log.contains("https://www.linkedin.com/in/one"),
        "job log should store an args summary, not the full url list: {log}"
    );
    let lines: Vec<Value> = log
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let outcomes: Vec<_> = lines
        .iter()
        .map(|line| line["outcome"].as_str().unwrap())
        .collect();
    assert_eq!(outcomes, vec!["queued", "in_progress", "done"]);
    assert_eq!(lines[0]["verb"], json!("tabs.open_group"));
    assert_eq!(lines[0]["args_summary"]["name"], json!("Lender follow-ups"));
    assert_eq!(lines[0]["args_summary"]["urls"], json!(2));
    assert!(lines[0]["created_at"].as_str().unwrap().ends_with('Z'));
    assert!(lines[2]["updated_at"].as_str().unwrap().ends_with('Z'));
}

#[test]
fn long_poll_returns_204_when_no_job_arrives() {
    let server = Server::start(&[]);
    let started = Instant::now();
    let (status, body) = get(&server, "/bridge/jobs/next?wait=1");
    assert_eq!(status, 204, "{body}");
    assert!(
        started.elapsed() >= Duration::from_millis(800),
        "wait=1 returned too quickly: {:?}",
        started.elapsed()
    );
    assert!(body.is_null(), "{body}");
}

#[test]
fn only_one_long_poll_receives_a_queued_job() {
    let server = Server::start(&[]);
    let (status, created) = post(
        &server,
        "/bridge/jobs",
        json!({"verb": "tabs.list", "args": {}}),
    );
    assert_eq!(status, 201, "{created}");
    let base = server.base.clone();
    let token = server.token.clone();
    let first = thread::spawn(move || poll_next(&base, &token));
    let base = server.base.clone();
    let token = server.token.clone();
    let second = thread::spawn(move || poll_next(&base, &token));
    let left = first.join().unwrap();
    let right = second.join().unwrap();
    let codes = [left.0, right.0];
    assert!(
        codes.contains(&200) && codes.contains(&204),
        "expected one 200 and one 204, got {left:?} {right:?}"
    );
}

fn poll_next(base: &str, token: &str) -> (u16, Value) {
    let result = ureq::get(&format!("{base}/bridge/jobs/next?wait=1"))
        .timeout(Duration::from_secs(40))
        .set("X-Bridge-Token", token)
        .call();
    match result {
        Ok(response) => {
            let status = response.status();
            let text = response.into_string().unwrap_or_default();
            let body = if text.is_empty() {
                Value::Null
            } else {
                serde_json::from_str(&text).unwrap_or(Value::String(text))
            };
            (status, body)
        }
        Err(ureq::Error::Status(status, response)) => {
            let text = response.into_string().unwrap_or_default();
            (
                status,
                serde_json::from_str(&text).unwrap_or(Value::String(text)),
            )
        }
        Err(error) => panic!("{error}"),
    }
}

#[test]
fn failed_result_is_stored_on_the_job() {
    let server = Server::start(&[]);
    let (status, created) = post(
        &server,
        "/bridge/jobs",
        json!({"verb": "tabs.focus", "args": {"tab_id": "t1"}}),
    );
    assert_eq!(status, 201, "{created}");
    let id = created["id"].as_str().unwrap();
    let (status, claimed) = get(&server, "/bridge/jobs/next?wait=0");
    assert_eq!(status, 200, "{claimed}");
    let (status, failed) = post(
        &server,
        &format!("/bridge/jobs/{id}/result"),
        json!({"ok": false, "error": "tab is gone"}),
    );
    assert_eq!(status, 200, "{failed}");
    assert_eq!(failed["status"], json!("failed"));
    assert_eq!(failed["error"], json!("tab is gone"));
    assert!(failed.get("result").is_none(), "{failed}");
}

#[test]
fn unknown_verb_is_rejected_with_403() {
    let server = Server::start(&[]);
    for verb in [
        "tabs.open_group",
        "tabs.list",
        "tabs.focus",
        "tabs.close",
        "tabs.move",
    ] {
        let (status, body) = post(&server, "/bridge/jobs", json!({"verb": verb, "args": {}}));
        assert_eq!(status, 201, "{verb} {body}");
    }
    let (status, body) = post(
        &server,
        "/bridge/jobs",
        json!({"verb": "tabs.explode", "args": {"name": "nope"}}),
    );
    assert_eq!(status, 403, "{body}");
    let message = error_text(&body);
    assert!(
        message.contains("tabs.explode") && message.contains("not on the bridge allowlist"),
        "{message}"
    );
    let (status, bad) = post(&server, "/bridge/jobs", json!({"args": {}}));
    assert_eq!(status, 400, "{bad}");
    let (status, bad) = post(
        &server,
        "/bridge/jobs",
        json!({"verb": "tabs.list", "args": ["not", "an", "object"]}),
    );
    assert_eq!(status, 400, "{bad}");
}

#[test]
fn send_submit_and_click_verbs_are_rejected_even_when_policy_allows_them() {
    let policy = "\
Rule:
  id: aside.bridge.allow_all
  version: 1.0.0

Ask \"Which effect?\" as effect
Ask \"What arguments?\" as request
Respond with record with true as allow and \"Test policy allows every verb.\" as reason
";
    let server = Server::start_with(&[], None, Some(policy));
    for verb in [
        "send",
        "submit",
        "click",
        "SEND",
        "tabs.send",
        "linkedin.send_message",
        "page.click",
        "form.submit",
        "composer.click_send",
    ] {
        let (status, body) = post(&server, "/bridge/jobs", json!({"verb": verb, "args": {}}));
        assert_eq!(status, 403, "{verb} {body}");
        let message = error_text(&body);
        assert!(
            message.contains(verb) && message.contains("send, submit, and click"),
            "{verb}: {message}"
        );
        assert!(
            !message.contains("Test policy allows"),
            "policy allow must not override the denylist: {message}"
        );
    }
    let (status, pinned) = post(
        &server,
        "/bridge/jobs",
        json!({"verb": "tabs.pin", "args": {"tab_id": "t"}}),
    );
    assert_eq!(
        status, 201,
        "a non-denied verb allowed by policy should queue: {pinned}"
    );
    let (status, opened) = post(
        &server,
        "/bridge/jobs",
        json!({"verb": "tabs.open_group", "args": {"name": "g", "urls": []}}),
    );
    assert_eq!(status, 201, "{opened}");
}

#[test]
fn missing_or_wrong_bridge_token_is_401() {
    let server = Server::start_with(&[], Some("serve-secret"), None);
    let body = json!({"verb": "tabs.list", "args": {}});
    let (status, denied) = call(&server, "POST", "/bridge/jobs", None, Some(body.clone()));
    assert_eq!(status, 401, "{denied}");
    assert!(error_text(&denied).contains("X-Bridge-Token"), "{denied}");

    let (status, denied) = call(
        &server,
        "POST",
        "/bridge/jobs",
        Some("wrong-token"),
        Some(body.clone()),
    );
    assert_eq!(status, 401, "{denied}");

    let (status, denied) = call(
        &server,
        "GET",
        "/bridge/jobs/next?wait=0",
        Some("serve-secret"),
        None,
    );
    assert_eq!(status, 401, "{denied}");

    let bearer = ureq::post(&format!("{}/bridge/jobs", server.base))
        .set("Authorization", "Bearer serve-secret")
        .send_json(body.clone());
    match bearer {
        Err(ureq::Error::Status(401, _)) => {}
        other => panic!("serve bearer token must not authorize bridge routes: {other:?}"),
    }

    let (status, created) = post(&server, "/bridge/jobs", body);
    assert_eq!(status, 201, "{created}");

    let health = ureq::get(&format!("{}/v1/health", server.base))
        .call()
        .expect("health stays open");
    assert_eq!(health.status(), 200);
    let lint = ureq::post(&format!("{}/v1/lint", server.base))
        .send_json(json!({"source": "Respond with \"ok\"\n"}));
    match lint {
        Err(ureq::Error::Status(401, _)) => {}
        other => panic!("serve token must still protect /v1 routes: {other:?}"),
    }
}

#[test]
fn rate_cap_rejects_intake_after_the_configured_limit() {
    let server = Server::start(&["--bridge-rate-limit", "2"]);
    for index in 0..2 {
        let (status, body) = post(
            &server,
            "/bridge/jobs",
            json!({"verb": "tabs.move", "args": {"index": index}}),
        );
        assert_eq!(status, 201, "{body}");
    }
    let (status, body) = post(
        &server,
        "/bridge/jobs",
        json!({"verb": "tabs.move", "args": {"index": 2}}),
    );
    assert_eq!(status, 429, "{body}");
    assert!(error_text(&body).contains("rate limit"), "{body}");
    let log = fs::read_to_string(&server.log_path).unwrap();
    assert!(log.contains("rate_limited"), "{log}");
}

#[test]
fn in_progress_job_times_out_to_failed() {
    let server = Server::start(&["--bridge-in-progress-timeout-secs", "1"]);
    let (status, first) = post(
        &server,
        "/bridge/jobs",
        json!({"verb": "tabs.close", "args": {"tab_id": "a"}}),
    );
    assert_eq!(status, 201, "{first}");
    let first_id = first["id"].as_str().unwrap().to_string();
    let (status, second) = post(
        &server,
        "/bridge/jobs",
        json!({"verb": "tabs.close", "args": {"tab_id": "b"}}),
    );
    assert_eq!(status, 201, "{second}");
    let second_id = second["id"].as_str().unwrap().to_string();

    let (status, claimed) = get(&server, "/bridge/jobs/next?wait=0");
    assert_eq!(status, 200, "{claimed}");
    assert_eq!(claimed["id"], json!(first_id));

    thread::sleep(Duration::from_secs(2));
    let (status, timed_out) = get(&server, &format!("/bridge/jobs/{first_id}"));
    assert_eq!(status, 200, "{timed_out}");
    assert_eq!(timed_out["status"], json!("failed"));
    assert_eq!(timed_out["error"], json!("in_progress timed out"));

    let (status, waiting) = get(&server, &format!("/bridge/jobs/{second_id}"));
    assert_eq!(waiting["status"], json!("queued"), "{status} {waiting}");
    let (status, next) = get(&server, "/bridge/jobs/next?wait=0");
    assert_eq!(status, 200, "{next}");
    assert_eq!(next["id"], json!(second_id));
}

#[test]
fn server_only_binds_loopback() {
    for bind in ["0.0.0.0:9", "localhost:7420"] {
        let out = Command::new(env!("CARGO_BIN_EXE_devlish-core"))
            .args(["--server", "--bind", bind])
            .env_remove("DEVLISH_VERIFIED_PROFILE")
            .output()
            .unwrap();
        assert!(!out.status.success(), "{bind} should be rejected");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            err.contains("127.0.0.1"),
            "{bind} stderr did not name the required host: {err}"
        );
    }
}

#[test]
fn browser_bridge_workflow_opens_a_group_and_waits_for_the_result() {
    let server = Server::start(&[]);
    let base = server.base.clone();
    let token = server.token.clone();
    let host = thread::spawn(move || fake_host(&base, &token));
    thread::sleep(Duration::from_millis(100));
    let workflow = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/browser_bridge/workflow.dvl");
    let input = json!({
        "name": "Lender follow-ups",
        "urls": [
            "https://www.linkedin.com/in/one",
            "https://www.linkedin.com/in/two"
        ],
        "bridge_url": server.base,
    });
    let token_file = server.dir.join("token");
    let output = Command::new(env!("CARGO_BIN_EXE_devlish-core"))
        .current_dir(&server.dir)
        .env_remove("DEVLISH_VERIFIED_PROFILE")
        .env("HOME", &server.dir)
        .env("ASIDE_BRIDGE_TOKEN_FILE", &token_file)
        .args([
            "--run",
            &workflow.to_string_lossy(),
            "--input",
            &input.to_string(),
        ])
        .output()
        .unwrap();
    let host_result = host.join().unwrap();
    assert!(
        host_result.is_ok(),
        "host: {host_result:?}\nstdout {}\nstderr {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success(),
        "stdout {}\nstderr {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let response: Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|error| panic!("workflow stdout was not JSON ({error}): {stdout}"));
    assert_eq!(response["ok"], json!(true), "{response}");
    assert_eq!(response["result"], json!({"opened": 2}), "{response}");
}

fn fake_host(base: &str, token: &str) -> Result<(), String> {
    let response = ureq::get(&format!("{base}/bridge/jobs/next?wait=25"))
        .timeout(Duration::from_secs(40))
        .set("X-Bridge-Token", token)
        .call()
        .map_err(|error| error.to_string())?;
    if response.status() != 200 {
        return Err(format!("next status {}", response.status()));
    }
    let job: Value = response.into_json().map_err(|error| error.to_string())?;
    let id = job["id"].as_str().unwrap_or("").to_string();
    let opened = job["args"]["urls"]
        .as_array()
        .map(|items| items.len())
        .unwrap_or(0);
    let posted = ureq::post(&format!("{base}/bridge/jobs/{id}/result"))
        .set("X-Bridge-Token", token)
        .send_json(json!({"ok": true, "result": {"opened": opened}}))
        .map_err(|error| error.to_string())?;
    if posted.status() != 200 {
        return Err(format!("result status {}", posted.status()));
    }
    if job["verb"] != json!("tabs.open_group") {
        return Err(format!("unexpected verb {job}"));
    }
    if job["args"]["name"] != json!("Lender follow-ups") {
        return Err(format!("unexpected args {job}"));
    }
    Ok(())
}
