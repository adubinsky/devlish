//! `devlish serve` — long-lived HTTP daemon wrapping the shared service layer.

use crate::devlish_search_paths_for;
use devlish_core::bridge::{Bridge, BridgeOverrides};
use devlish_core::logutil;
use devlish_core::service::{service_compile, service_lint, service_run, RunRequest, ServiceResult};
use serde_json::{json, Value};
use std::fs;
use std::io::{Cursor, Read};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

#[derive(Clone)]
struct ServeRoute {
    method: String,
    path: String,
    source: String,
    policy: Option<String>,
}

struct ServeState {
    token: Option<String>,
    #[allow(dead_code)]
    tools_dirs: Vec<PathBuf>,
    public_dir: Option<PathBuf>,
    routes: Vec<ServeRoute>,
    bridge: Bridge,
}

pub fn run_serve(args: Vec<String>) -> Result<(), String> {
    let mut bind = "127.0.0.1:7420".to_string();
    let mut tools_dirs: Vec<PathBuf> = Vec::new();
    let mut public_dir: Option<PathBuf> = None;
    let mut config_path: Option<PathBuf> = None;
    let mut bridge_overrides = BridgeOverrides {
        token_file: None,
        policy_file: None,
        log_file: None,
        rate_limit: None,
        rate_window_secs: None,
        in_progress_timeout_secs: None,
    };
    let mut index = 1usize;
    while index < args.len() {
        match args[index].as_str() {
            "--bind" => {
                index += 1;
                bind = args
                    .get(index)
                    .ok_or_else(|| "--bind requires host:port".to_string())?
                    .clone();
            }
            "--tools-dir" => {
                index += 1;
                tools_dirs.push(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--tools-dir requires a path".to_string())?,
                ));
            }
            "--public-dir" => {
                index += 1;
                public_dir = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--public-dir requires a path".to_string())?,
                ));
            }
            "--config" => {
                index += 1;
                config_path = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--config requires a path".to_string())?,
                ));
            }
            "--bridge-token-file" => {
                index += 1;
                bridge_overrides.token_file = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--bridge-token-file requires a path".to_string())?,
                ));
            }
            "--bridge-policy" => {
                index += 1;
                bridge_overrides.policy_file = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--bridge-policy requires a path".to_string())?,
                ));
            }
            "--bridge-log" => {
                index += 1;
                bridge_overrides.log_file = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--bridge-log requires a path".to_string())?,
                ));
            }
            "--bridge-rate-limit" => {
                index += 1;
                bridge_overrides.rate_limit = Some(parse_bridge_u64(
                    "--bridge-rate-limit",
                    args.get(index).ok_or_else(|| {
                        "--bridge-rate-limit requires an integer".to_string()
                    })?,
                )?);
            }
            "--bridge-rate-window-secs" => {
                index += 1;
                bridge_overrides.rate_window_secs = Some(parse_bridge_u64(
                    "--bridge-rate-window-secs",
                    args.get(index).ok_or_else(|| {
                        "--bridge-rate-window-secs requires an integer".to_string()
                    })?,
                )?);
            }
            "--bridge-in-progress-timeout-secs" => {
                index += 1;
                bridge_overrides.in_progress_timeout_secs = Some(parse_bridge_u64(
                    "--bridge-in-progress-timeout-secs",
                    args.get(index).ok_or_else(|| {
                        "--bridge-in-progress-timeout-secs requires an integer".to_string()
                    })?,
                )?);
            }
            "--log-level" => {
                // Consumed earlier by logutil::init_from_env_and_args; skip value.
                index += 1;
            }
            value if value.starts_with("--log-level=") => {}
            "--help" | "-h" => {
                println!(
                    "Usage: devlish --server [--bind 127.0.0.1:PORT] [--public-dir DIR] [--config serve.json] [--tools-dir DIR] [--log-level LEVEL]\n\
                     [--bridge-token-file PATH] [--bridge-policy PATH] [--bridge-log PATH]\n\
                     [--bridge-rate-limit N] [--bridge-rate-window-secs N] [--bridge-in-progress-timeout-secs N]\n\n\
                     Endpoints:\n\
                       GET  /v1/health\n\
                       POST /v1/compile\n\
                       POST /v1/run\n\
                       POST /v1/validate\n\
                       POST /v1/lint\n\
                       POST /v1/harness/sessions\n\
                       POST /v1/harness/sessions/:id/resume\n\
                       POST /bridge/jobs\n\
                       GET  /bridge/jobs/next?wait=SECONDS\n\
                       GET  /bridge/jobs/:id\n\
                       POST /bridge/jobs/:id/result\n\
                       GET  /*            static files from --public-dir or serve.json public\n\
                       POST <route>       named app routes from serve.json\n\n\
                     Auth: set DEVLISH_SERVE_TOKEN and send Authorization: Bearer <token>\n\
                     Bridge auth: X-Bridge-Token must match the bridge token file (default ~/.config/aside-bridge/token)\n\
                     Bind: 127.0.0.1 only\n\
                     Logging: --log-level error|info|debug (or DEVLISH_LOG)"
                );
                return Ok(());
            }
            other => return Err(format!("unknown serve option: {other}")),
        }
        index += 1;
    }

    let mut routes = Vec::new();
    let config_file = config_path.unwrap_or_else(|| PathBuf::from("serve.json"));
    if config_file.is_file() {
        let loaded = load_serve_config(&config_file)?;
        if public_dir.is_none() {
            public_dir = loaded.public_dir;
        }
        routes = loaded.routes;
        logutil::info(&format!(
            "serve config {} routes={} public={}",
            config_file.display(),
            routes.len(),
            public_dir
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "(none)".into())
        ));
    }

    require_loopback_bind(&bind)?;
    let bridge = Bridge::open(bridge_overrides)?;
    let token = std::env::var("DEVLISH_SERVE_TOKEN").ok();
    let state = Arc::new(ServeState {
        token,
        tools_dirs,
        public_dir,
        routes,
        bridge,
    });
    let server = Server::http(&bind).map_err(|e| format!("failed to bind {bind}: {e}"))?;
    logutil::info(&format!("devlish serve listening on http://{bind}"));
    logutil::debug(&format!(
        "serve auth={} tools_dirs={}",
        if state.token.is_some() {
            "token"
        } else {
            "none"
        },
        state.tools_dirs.len()
    ));

    for request in server.incoming_requests() {
        let state = Arc::clone(&state);
        std::thread::spawn(move || {
            if let Err(error) = handle_request(request, &state) {
                logutil::error(&format!("serve error: {error}"));
            }
        });
    }
    Ok(())
}

fn require_loopback_bind(bind: &str) -> Result<(), String> {
    let addr: SocketAddr = bind
        .parse()
        .map_err(|_| format!("--bind must be 127.0.0.1:PORT, got {bind}"))?;
    if addr.ip() != IpAddr::V4(Ipv4Addr::LOCALHOST) {
        return Err(format!(
            "devlish --server only listens on 127.0.0.1, got {bind}"
        ));
    }
    Ok(())
}

fn parse_bridge_u64(flag: &str, value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|_| format!("{flag} requires an integer"))
}

fn handle_request(mut request: Request, state: &ServeState) -> Result<(), String> {
    let method = request.method().clone();
    let method_for_error = method.clone();
    let url = request.url().to_string();
    let (path, query) = match url.split_once('?') {
        Some((path, query)) => (path.to_string(), query.to_string()),
        None => (url.clone(), String::new()),
    };
    logutil::info(&format!("serve {method_for_error} {path}"));

    if method == Method::Options {
        let mut response = Response::empty(StatusCode(204));
        for header in cors_headers() {
            response = response.with_header(header);
        }
        request.respond(response).map_err(|e| e.to_string())?;
        return Ok(());
    }

    if path == "/bridge/jobs" || path.starts_with("/bridge/jobs/") {
        return handle_bridge(request, state, &method_for_error, &path, &query);
    }

    let is_health = method == Method::Get && path == "/v1/health";
    let is_static = method == Method::Get && !path.starts_with("/v1/");
    if let Some(expected) = &state.token {
        let authorized = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("Authorization"))
            .map(|h| h.value.as_str())
            .is_some_and(|v| v == format!("Bearer {expected}") || v == expected.as_str());
        if !authorized && !is_health && !is_static {
            let mut response = Response::from_string(json!({"error":"unauthorized"}).to_string())
                .with_status_code(StatusCode(401))
                .with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                );
            for header in cors_headers() {
                response = response.with_header(header);
            }
            request.respond(response).map_err(|e| e.to_string())?;
            return Ok(());
        }
    }

    if is_static {
        if let Some(public_dir) = &state.public_dir {
            if let Some(file_path) = public_file(public_dir, &path) {
                return respond_static(request, &file_path, &method_for_error, &path);
            }
        }
    }

    let mut body = String::new();
    request
        .as_reader()
        .read_to_string(&mut body)
        .map_err(|e| e.to_string())?;
    logutil::debug(&format!(
        "serve request body_chars={} body_preview={}",
        body.len(),
        truncate_for_log(&body, 240)
    ));

    let (status, payload) = match (method, path.as_str()) {
        (Method::Get, "/v1/health") => (200, json!({"ok": true, "service": "devlish", "version": "0.1.0"})),
        (Method::Post, "/v1/compile") => {
            let args: Value = serde_json::from_str(&body).unwrap_or(json!({}));
            let source = args.get("source").and_then(Value::as_str).unwrap_or("");
            let source_path = args
                .get("source_path")
                .and_then(Value::as_str)
                .map(String::from);
            let result = service_compile(source, source_path, devlish_search_paths_for(None));
            (if result.ok { 200 } else { 400 }, result.value)
        }
        (Method::Post, "/v1/run") | (Method::Post, "/v1/harness/sessions") => {
            let args: Value = serde_json::from_str(&body).unwrap_or(json!({}));
            let result = run_from_json(&args);
            (if result.ok { 200 } else { 400 }, result.value)
        }
        (Method::Post, "/v1/validate") | (Method::Post, "/v1/lint") => {
            let args: Value = serde_json::from_str(&body).unwrap_or(json!({}));
            let source = args.get("source").and_then(Value::as_str).unwrap_or("");
            let result = service_lint(source);
            (if result.ok { 200 } else { 400 }, result.value)
        }
        (Method::Post, p) if p.starts_with("/v1/harness/sessions/") && p.ends_with("/resume") => {
            let args: Value = serde_json::from_str(&body).unwrap_or(json!({}));
            let result = run_from_json(&args);
            (if result.ok { 200 } else { 400 }, result.value)
        }
        (ref method, path) if named_route(state, method, path).is_some() => {
            let route = named_route(state, method, path).expect("route");
            let input: Value = serde_json::from_str(&body).unwrap_or(json!({}));
            let result = run_named_route(&route, input);
            (if result.ok { 200 } else { 400 }, result.value)
        }
        _ => (
            404,
            json!({"error": format!("not found: {method_for_error} {path}")}),
        ),
    };

    let body = serde_json::to_string_pretty(&payload).unwrap_or_default();
    logutil::info(&format!("serve {method_for_error} {path} -> {status}"));
    logutil::debug(&format!(
        "serve response body_chars={} body_preview={}",
        body.len(),
        truncate_for_log(&body, 240)
    ));
    let response = Response::new(
        StatusCode(status),
        json_headers(),
        Cursor::new(body.clone()),
        Some(body.len()),
        None,
    );
    request.respond(response).map_err(|e| e.to_string())?;
    Ok(())
}

fn handle_bridge(
    mut request: Request,
    state: &ServeState,
    method: &Method,
    path: &str,
    query: &str,
) -> Result<(), String> {
    let method_name = method_name(method);
    let token = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("X-Bridge-Token"))
        .map(|header| header.value.as_str().trim().to_string());
    let mut body = String::new();
    request
        .as_reader()
        .take(1_048_577)
        .read_to_string(&mut body)
        .map_err(|error| error.to_string())?;
    if body.len() > 1_048_576 {
        return respond_recorded(
            request,
            &method_name,
            path,
            400,
            Some(json!({"error": "request body exceeds 1 MiB"}).to_string()),
        );
    }
    let response = state.bridge.handle(
        &method_name,
        path,
        query,
        token.as_deref(),
        &body,
    );
    let payload = response
        .body
        .as_ref()
        .map(|value| serde_json::to_string_pretty(value).unwrap_or_default());
    respond_recorded(request, &method_name, path, response.status, payload)
}

fn respond_recorded(
    request: Request,
    method: &str,
    path: &str,
    status: u16,
    body: Option<String>,
) -> Result<(), String> {
    logutil::info(&format!("serve {method} {path} -> {status}"));
    if let Some(body) = body {
        let response = Response::new(
            StatusCode(status),
            json_headers(),
            Cursor::new(body.clone()),
            Some(body.len()),
            None,
        );
        request.respond(response).map_err(|error| error.to_string())
    } else {
        let mut response = Response::empty(StatusCode(status));
        for header in cors_headers() {
            response = response.with_header(header);
        }
        request.respond(response).map_err(|error| error.to_string())
    }
}

fn cors_headers() -> Vec<Header> {
    vec![
        Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap(),
        Header::from_bytes(
            &b"Access-Control-Allow-Headers"[..],
            &b"Content-Type, Authorization, X-Bridge-Token"[..],
        )
        .unwrap(),
        Header::from_bytes(
            &b"Access-Control-Allow-Methods"[..],
            &b"GET, POST, OPTIONS"[..],
        )
        .unwrap(),
    ]
}

fn json_headers() -> Vec<Header> {
    let mut headers = vec![
        Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
    ];
    headers.extend(cors_headers());
    headers
}

fn truncate_for_log(text: &str, max: usize) -> String {
    let cleaned = text.replace('\n', " ");
    if cleaned.chars().count() <= max {
        return cleaned;
    }
    let truncated: String = cleaned.chars().take(max).collect();
    format!("{truncated}…")
}

fn run_from_json(args: &Value) -> devlish_core::service::ServiceResult {
    let source = args
        .get("source")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            args.get("source_path")
                .and_then(Value::as_str)
                .and_then(|path| std::fs::read_to_string(path).ok())
        })
        .unwrap_or_default();
    let source_path = args
        .get("source_path")
        .and_then(Value::as_str)
        .map(String::from);
    service_run(RunRequest {
        source,
        source_path: source_path.clone(),
        input: args.get("input").cloned().unwrap_or(json!({})),
        env: vec![],
        provider: args
            .get("provider")
            .and_then(Value::as_str)
            .map(String::from),
        model: args.get("model").and_then(Value::as_str).map(String::from),
        search_paths: devlish_search_paths_for(source_path.as_deref().map(std::path::Path::new)),
        policy_path: None,
        policy_log: None,
        default_authorization: None,
        limits: None,
        artifact_requirements: None,
    })
}

struct LoadedServeConfig {
    public_dir: Option<PathBuf>,
    routes: Vec<ServeRoute>,
}

fn load_serve_config(path: &Path) -> Result<LoadedServeConfig, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("serve config {}: {e}", path.display()))?;
    let value: Value =
        serde_json::from_str(&text).map_err(|e| format!("serve config {}: {e}", path.display()))?;
    let public_dir = value
        .get("public")
        .and_then(Value::as_str)
        .map(PathBuf::from);
    let mut routes = Vec::new();
    if let Some(items) = value.get("routes").and_then(Value::as_array) {
        for item in items {
            let method = item
                .get("method")
                .and_then(Value::as_str)
                .ok_or("serve route missing method")?
                .to_string();
            let route_path = item
                .get("path")
                .and_then(Value::as_str)
                .ok_or("serve route missing path")?
                .to_string();
            let source = item
                .get("source")
                .and_then(Value::as_str)
                .ok_or("serve route missing source")?
                .to_string();
            if source.contains("..") || Path::new(&source).is_absolute() {
                return Err(format!("serve route source must be a relative project path: {source}"));
            }
            if !route_path.starts_with('/') || route_path.starts_with("/v1/") {
                return Err(format!("serve route path must be an app path, not /v1: {route_path}"));
            }
            routes.push(ServeRoute {
                method,
                path: route_path,
                source,
                policy: item
                    .get("policy")
                    .and_then(Value::as_str)
                    .map(String::from),
            });
        }
    }
    Ok(LoadedServeConfig { public_dir, routes })
}

fn named_route(state: &ServeState, method: &Method, path: &str) -> Option<ServeRoute> {
    let method_name = method_name(method);
    state
        .routes
        .iter()
        .find(|route| route.method.eq_ignore_ascii_case(&method_name) && route.path == path)
        .cloned()
}

fn method_name(method: &Method) -> String {
    match method {
        Method::Get => "GET".into(),
        Method::Post => "POST".into(),
        Method::Put => "PUT".into(),
        Method::Delete => "DELETE".into(),
        Method::Patch => "PATCH".into(),
        other => format!("{other}"),
    }
}

fn run_named_route(route: &ServeRoute, input: Value) -> ServiceResult {
    let source = match fs::read_to_string(&route.source) {
        Ok(text) => text,
        Err(error) => {
            return ServiceResult {
                ok: false,
                value: json!({"error": format!("read {}: {error}", route.source)}),
            }
        }
    };
    let policy_path = route.policy.as_ref().map(PathBuf::from);
    let policy_log = if policy_path.is_some() {
        match fresh_policy_log(&route.path) {
            Ok(path) => Some(path),
            Err(error) => {
                return ServiceResult {
                    ok: false,
                    value: json!({"error": error}),
                }
            }
        }
    } else {
        None
    };
    service_run(RunRequest {
        source,
        source_path: Some(route.source.clone()),
        input,
        env: vec![],
        provider: None,
        model: None,
        search_paths: devlish_search_paths_for(Some(Path::new(&route.source))),
        policy_path,
        policy_log,
        default_authorization: if route.policy.is_some() {
            Some("deny-unless-allowed".into())
        } else {
            None
        },
        limits: None,
        artifact_requirements: None,
    })
}

fn fresh_policy_log(route_path: &str) -> Result<PathBuf, String> {
    fs::create_dir_all("journals").map_err(|e| format!("journals/: {e}"))?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let slug = route_path.trim_start_matches('/').replace('/', "-");
    let path = PathBuf::from("journals").join(format!("{slug}-{nanos}.jsonl"));
    if path.exists() {
        return Err(format!("policy log already exists: {}", path.display()));
    }
    Ok(path)
}

fn public_file(root: &Path, url_path: &str) -> Option<PathBuf> {
    let relative = url_path.trim_start_matches('/');
    let relative = if relative.is_empty() {
        "index.html"
    } else {
        relative
    };
    if relative.contains("..") || Path::new(relative).is_absolute() {
        return None;
    }
    let joined = root.join(relative);
    let root = root.canonicalize().ok()?;
    let canonical = joined.canonicalize().ok()?;
    if !canonical.starts_with(&root) || !canonical.is_file() {
        return None;
    }
    Some(canonical)
}

fn respond_static(
    request: Request,
    file_path: &Path,
    method: &Method,
    path: &str,
) -> Result<(), String> {
    let bytes = fs::read(file_path).map_err(|e| format!("read {}: {e}", file_path.display()))?;
    let mime = mime_for(file_path);
    logutil::info(&format!("serve {method} {path} -> 200"));
    let response = Response::new(
        StatusCode(200),
        vec![Header::from_bytes(&b"Content-Type"[..], mime.as_bytes()).unwrap()],
        Cursor::new(bytes.clone()),
        Some(bytes.len()),
        None,
    );
    request.respond(response).map_err(|e| e.to_string())
}

fn mime_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("wasm") => "application/wasm",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
