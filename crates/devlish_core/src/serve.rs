//! `devlish serve` — long-lived HTTP daemon wrapping the shared service layer.

use crate::devlish_search_paths_for;
use devlish_core::logutil;
use devlish_core::service::{service_compile, service_lint, service_run, RunRequest};
use serde_json::{json, Value};
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

#[derive(Clone)]
struct ServeState {
    token: Option<String>,
    #[allow(dead_code)]
    tools_dirs: Vec<PathBuf>,
}

pub fn run_serve(args: Vec<String>) -> Result<(), String> {
    let mut bind = "127.0.0.1:7420".to_string();
    let mut tools_dirs: Vec<PathBuf> = Vec::new();
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
            "--log-level" => {
                // Consumed earlier by logutil::init_from_env_and_args; skip value.
                index += 1;
            }
            value if value.starts_with("--log-level=") => {}
            "--help" | "-h" => {
                println!(
                    "Usage: devlish-core serve [--bind HOST:PORT] [--tools-dir DIR] [--log-level LEVEL]\n\n\
                     Endpoints:\n\
                       GET  /v1/health\n\
                       POST /v1/compile\n\
                       POST /v1/run\n\
                       POST /v1/validate\n\
                       POST /v1/lint\n\
                       POST /v1/harness/sessions\n\
                       POST /v1/harness/sessions/:id/resume\n\n\
                     Auth: set DEVLISH_SERVE_TOKEN and send Authorization: Bearer <token>\n\
                     Logging: --log-level error|info|debug (or DEVLISH_LOG)"
                );
                return Ok(());
            }
            other => return Err(format!("unknown serve option: {other}")),
        }
        index += 1;
    }

    let token = std::env::var("DEVLISH_SERVE_TOKEN").ok();
    let state = Arc::new(ServeState { token, tools_dirs });
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
        if let Err(error) = handle_request(request, &state) {
            logutil::error(&format!("serve error: {error}"));
        }
    }
    Ok(())
}

fn handle_request(mut request: Request, state: &ServeState) -> Result<(), String> {
    if let Some(expected) = &state.token {
        let authorized = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("Authorization"))
            .map(|h| h.value.as_str())
            .is_some_and(|v| v == format!("Bearer {expected}") || v == expected.as_str());
        if !authorized && request.url() != "/v1/health" {
            let response = Response::from_string(json!({"error":"unauthorized"}).to_string())
                .with_status_code(StatusCode(401))
                .with_header(
                    Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap(),
                );
            request.respond(response).map_err(|e| e.to_string())?;
            return Ok(());
        }
    }

    let method = request.method().clone();
    let method_for_error = method.clone();
    let url = request.url().to_string();
    let path = url.split('?').next().unwrap_or(&url).to_string();
    logutil::info(&format!("serve {method_for_error} {path}"));

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
        vec![Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()],
        Cursor::new(body.clone()),
        Some(body.len()),
        None,
    );
    request.respond(response).map_err(|e| e.to_string())?;
    Ok(())
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
    })
}
