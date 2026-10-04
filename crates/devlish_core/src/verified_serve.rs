//! Authenticated loopback adapter for operator-selected signed sessions.
//! Local tamper-evidence service, not a hostile-user availability/isolation boundary.
use devlish_core::verified_session::VerifiedSession;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{self, Read, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
};
use subtle::ConstantTimeEq;
use tiny_http::{Header, Method, Request, Response, Server};

const MAX_BODY: usize = 65_536;
const RESPONSE_BUDGET: usize = 64_000;

struct State {
    profile: PathBuf,
    logs: PathBuf,
    token: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RunRequest {
    session_id: String,
    input: Value,
}

pub(super) fn run(args: Vec<String>) -> Result<(), String> {
    devlish_core::logutil::set_level(devlish_core::logutil::LogLevel::Error);
    const USAGE: &str = "Usage: DEVLISH_VERIFIED_PROFILE=<operator-profile> DEVLISH_SERVE_TOKEN=<64-hex-secret> devlish-core serve-verified --log-dir <private-directory> [--bind 127.0.0.1:7421]";
    if args.len() == 2 && ["--help", "-h"].contains(&args[1].as_str()) {
        println!("{USAGE}");
        return Ok(());
    }
    let mut options = BTreeMap::new();
    let mut index = 1;
    while index < args.len() {
        if !["--log-dir", "--bind"].contains(&args[index].as_str())
            || index + 1 >= args.len()
            || options
                .insert(args[index].as_str(), args[index + 1].as_str())
                .is_some()
        {
            return Err(USAGE.into());
        }
        index += 2;
    }
    let bind: SocketAddr = options
        .get("--bind")
        .copied()
        .unwrap_or("127.0.0.1:7421")
        .parse()
        .map_err(|_| "verified service bind must be a literal loopback address")?;
    if !bind.ip().is_loopback() {
        return Err("verified service only supports loopback binding".into());
    }
    let token = std::env::var("DEVLISH_SERVE_TOKEN")
        .map_err(|_| "verified service requires an operator token")?;
    if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("verified service requires a 64-hex operator token".into());
    }
    let profile = std::env::var_os("DEVLISH_VERIFIED_PROFILE")
        .map(PathBuf::from)
        .ok_or("verified service requires an operator profile")?;
    let profile = if profile.is_absolute() {
        profile
    } else {
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(profile)
    };
    let logs = private_log_directory(Path::new(options.get("--log-dir").ok_or(USAGE)?))?;
    // Refuse to listen for an invalid deployment; every request admits afresh too.
    drop(VerifiedSession::admit(&profile, "server-startup", false)?);
    drop(super::CredentialStore::verified()?);
    let state = State {
        profile,
        logs,
        token,
    };
    let server = Server::http(bind).map_err(|_| "verified service could not bind")?;
    println!("verified service listening on {}", server.server_addr());
    for request in server.incoming_requests() {
        // No body, token, policy reason, provider response or filesystem diagnostic
        // enters operator stdout/stderr. Delivery failure never triggers re-execution.
        let _ = handle(request, &state);
    }
    Ok(())
}

fn private_log_directory(path: &Path) -> Result<PathBuf, String> {
    #[cfg(not(unix))]
    {
        let _ = path;
        Err("verified service storage requires Unix permissions".into())
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata =
            std::fs::symlink_metadata(path).map_err(|_| "verified log directory unavailable")?;
        if !metadata.is_dir()
            || metadata.mode() & 0o077 != 0
            || metadata.uid() != unsafe { libc::geteuid() }
        {
            return Err("verified log directory must be private, owned and not a symlink".into());
        }
        std::fs::canonicalize(path).map_err(|_| "cannot anchor verified log directory".into())
    }
}

fn authorized(request: &Request, token: &str) -> bool {
    let mut headers = request
        .headers()
        .iter()
        .filter(|h| h.field.equiv("Authorization"));
    let Some(header) = headers.next() else {
        return false;
    };
    if headers.next().is_some() {
        return false;
    }
    let Some(candidate) = header.value.as_str().strip_prefix("Bearer ") else {
        return false;
    };
    bool::from(candidate.as_bytes().ct_eq(token.as_bytes()))
}
fn reply(request: Request, status: u16, value: Value) -> Result<(), String> {
    let response = Response::from_string(value.to_string())
        .with_status_code(status)
        .with_header(
            Header::from_bytes("Content-Type", "application/json").expect("constant header"),
        )
        .with_header(Header::from_bytes("Cache-Control", "no-store").expect("constant header"));
    request
        .respond(response)
        .map_err(|_| "verified response delivery failed".into())
}
fn handle(mut request: Request, state: &State) -> Result<(), String> {
    if !authorized(&request, &state.token) {
        return reply(request, 401, json!({"error":"unauthorized"}));
    }
    if request.method() == &Method::Get && request.url() == "/v1/health" {
        return reply(
            request,
            200,
            json!({"ok":true,"mode":"verified","execution_origin_verified":false}),
        );
    }
    if request.method() != &Method::Post || request.url() != "/v1/run" {
        return reply(
            request,
            404,
            json!({"error":"unsupported verified endpoint"}),
        );
    }
    // Deliberately no chunked upload, caller-selected profile/source/path/provider,
    // raw-evidence switch, legacy compile/harness/resume or arbitrary tool endpoint.
    if request
        .headers()
        .iter()
        .any(|h| h.field.equiv("Transfer-Encoding"))
    {
        return reply(
            request,
            400,
            json!({"error":"chunked requests are unsupported"}),
        );
    }
    let Some(size) = request.body_length() else {
        return reply(request, 411, json!({"error":"content length required"}));
    };
    if size > MAX_BODY {
        return reply(
            request,
            413,
            json!({"error":"request exceeds service limit"}),
        );
    }
    let mut body = Vec::new();
    if request
        .as_reader()
        .take(MAX_BODY as u64 + 1)
        .read_to_end(&mut body)
        .is_err()
        || body.len() != size
    {
        return reply(request, 400, json!({"error":"invalid request body"}));
    }
    let command: RunRequest = match serde_json::from_slice(&body) {
        Ok(command) => command,
        Err(_) => return reply(request, 400, json!({"error":"invalid verified request"})),
    };
    let admitted = match VerifiedSession::admit(&state.profile, &command.session_id, false) {
        Ok(admitted) => admitted,
        Err(_) => return reply(request, 503, json!({"error":"verified admission rejected"})),
    };
    // Admission validates the correlation ID before it is used in a path.
    let log = state.logs.join(format!("{}.jsonl", command.session_id));
    if std::fs::symlink_metadata(&log).is_ok() {
        return reply(
            request,
            409,
            json!({"error":"session already exists; no retry performed"}),
        );
    }
    let credentials = match super::CredentialStore::verified() {
        Ok(credentials) => credentials,
        Err(_) => {
            return reply(
                request,
                503,
                json!({"error":"operator credential source unavailable"}),
            )
        }
    };
    let mut host = super::NativeHost::new(credentials, None);
    host.verified_model_route = Some(admitted.model_route());
    host.response_buffer = Some(ResponseBuffer::new());
    let result = admitted.execute(command.input, &log, &mut host);
    let buffer = host
        .response_buffer
        .take()
        .expect("installed response sink");
    let (status, value) = execution_response(result, buffer, &command.session_id);
    reply(request, status, value)
}

fn execution_response(
    result: Result<devlish_core::governed_run::Completion, String>,
    buffer: ResponseBuffer,
    session_id: &str,
) -> (u16, Value) {
    match result {
        Ok(completion) => (
            200,
            json!({"success":true,"session_id":session_id,"paused":completion.paused,"responses":buffer.values}),
        ),
        Err(_) => (
            500,
            json!({"error":"verified execution failed; effects may have occurred; no retry performed"}),
        ),
    }
}

pub(super) struct ResponseBuffer {
    values: Vec<Value>,
    remaining: usize,
}
impl ResponseBuffer {
    fn new() -> Self {
        Self {
            values: vec![],
            remaining: RESPONSE_BUDGET,
        }
    }
    pub(super) fn push(&mut self, value: &Value) -> Result<(), String> {
        if self.values.len() >= 16 {
            return Err("approved response count exceeds service limit".into());
        }
        let mut counter = Budget(self.remaining);
        serde_json::to_writer(&mut counter, value)
            .map_err(|_| "approved response exceeds service limit")?;
        self.remaining = counter.0;
        self.values.push(value.clone());
        Ok(())
    }
}
struct Budget(usize);
impl Write for Budget {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_sub(bytes.len())
            .ok_or_else(|| io::Error::other("response size limit"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recording_failure_after_response_never_releases_staged_values() {
        use devlish_core::{compile_source_to_json, governed_run::GovernedRun, CompileOptions};
        use devlish_vm::policy::{EffectPolicy, PolicyRecorder};
        struct Recorder {
            remaining: usize,
        }
        impl PolicyRecorder for Recorder {
            fn record(&mut self, _: &Value) -> Result<(), String> {
                if self.remaining == 0 {
                    return Err("synthetic-private-recording-error".into());
                }
                self.remaining -= 1;
                Ok(())
            }
        }
        let compile = |source| {
            serde_json::from_str(
                &compile_source_to_json(
                    source,
                    CompileOptions {
                        source_path: None,
                        search_paths: vec![],
                    },
                )
                .unwrap(),
            )
            .unwrap()
        };
        for accepted in [1, 2] {
            // Decision is recorded; fail either the response outcome or final record.
            let policy = EffectPolicy::new(compile("Rule:\n  id: test.http\n  version: 1.0.0\nRespond with record with true as allow and \"ok\" as reason")).unwrap();
            let runner = GovernedRun::new(
                compile("Respond with \"staged-approved-output\""),
                Value::Null,
                policy,
                1000,
                ["respond".into()].into(),
            )
            .unwrap();
            let mut host =
                super::super::NativeHost::new(super::super::CredentialStore::new(&[], None), None);
            host.response_buffer = Some(ResponseBuffer::new());
            let result = runner.run(
                &mut host,
                &mut Recorder {
                    remaining: accepted,
                },
            );
            assert!(result.is_err());
            let buffer = host.response_buffer.take().unwrap();
            assert_eq!(buffer.values, vec![json!("staged-approved-output")]);
            let (status, response) =
                execution_response(result.map_err(|e| e.to_string()), buffer, "test");
            assert_eq!(status, 500);
            assert!(response.get("responses").is_none());
            assert!(!response.to_string().contains("staged-approved-output"));
            assert!(!response
                .to_string()
                .contains("synthetic-private-recording-error"));
        }
    }
    #[test]
    fn approved_response_buffer_enforces_aggregate_size_and_count() {
        let mut buffer = ResponseBuffer::new();
        assert!(buffer.push(&json!("x".repeat(RESPONSE_BUDGET))).is_err());
        assert!(buffer.values.is_empty());
        buffer
            .push(&json!("x".repeat(RESPONSE_BUDGET / 2)))
            .unwrap();
        assert!(buffer
            .push(&json!("x".repeat(RESPONSE_BUDGET / 2)))
            .is_err());
        assert_eq!(buffer.values.len(), 1);
        let mut buffer = ResponseBuffer::new();
        for _ in 0..16 {
            buffer.push(&Value::Null).unwrap();
        }
        assert!(buffer.push(&Value::Null).is_err());
    }
}
