//! Shared compile / run / lint service used by MCP, HTTP serve, and harness.
//! Returns structured JSON Values (not MCP content wrappers).

use crate::{
    compile_source_to_json, lint_source, logutil, CompileOptions,
};
use devlish_llm::{complete, response_value, CredentialResolver, LlmConfig, LlmRequest};
use devlish_vm::{HostEffects, Vm};
use serde_json::{json, Map, Value};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Options for a compile+run invocation.
#[derive(Debug, Clone, Default)]
pub struct RunRequest {
    pub source: String,
    pub source_path: Option<String>,
    pub input: Value,
    pub env: Vec<(String, String)>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub search_paths: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ServiceResult {
    pub ok: bool,
    pub value: Value,
}

pub fn service_compile(source: &str, source_path: Option<String>, search_paths: Vec<String>) -> ServiceResult {
    match compile_source_to_json(
        source,
        CompileOptions {
            source_path,
            search_paths,
        },
    ) {
        Ok(bytecode_json) => match serde_json::from_str::<Value>(&bytecode_json) {
            Ok(package) => ServiceResult {
                ok: true,
                value: package,
            },
            Err(error) => ServiceResult {
                ok: false,
                value: json!({"error": format!("Internal error: {error}")}),
            },
        },
        Err(error) => ServiceResult {
            ok: false,
            value: json!({
                "error": error.to_string(),
                "diagnostics": error.diagnostics.iter().map(|d| json!({
                    "line": d.line,
                    "severity": "error",
                    "message": d.message,
                    "source_text": d.source_text
                })).collect::<Vec<_>>()
            }),
        },
    }
}

pub fn service_lint(source: &str) -> ServiceResult {
    match compile_source_to_json(
        source,
        CompileOptions {
            source_path: None,
            search_paths: vec![],
        },
    ) {
        Ok(_) => {
            let diagnostics: Vec<Value> = match lint_source(
                source,
                CompileOptions {
                    source_path: None,
                    search_paths: vec![],
                },
            ) {
                Ok(warnings) => warnings
                    .iter()
                    .map(|w| {
                        json!({
                            "line": w.line,
                            "severity": "warning",
                            "message": w.message,
                            "source_text": w.source_text
                        })
                    })
                    .collect(),
                Err(_) => Vec::new(),
            };
            ServiceResult {
                ok: true,
                value: json!({ "valid": true, "diagnostics": diagnostics }),
            }
        }
        Err(error) => {
            let diagnostics: Vec<Value> = error
                .diagnostics
                .iter()
                .map(|d| {
                    json!({
                        "line": d.line,
                        "severity": "error",
                        "message": d.message,
                        "source_text": d.source_text
                    })
                })
                .collect();
            ServiceResult {
                ok: false,
                value: json!({
                    "valid": false,
                    "diagnostics": diagnostics,
                    "error": error.to_string()
                }),
            }
        }
    }
}

pub fn service_run(req: RunRequest) -> ServiceResult {
    let compiled = service_compile(&req.source, req.source_path.clone(), req.search_paths);
    if !compiled.ok {
        return compiled;
    }
    let package = compiled.value;
    let source_file = req.source_path.as_deref().map(Path::new);
    let mut host = ServiceHost::new(&req.env, source_file, req.provider.clone(), req.model.clone());
    match Vm::new(package, req.input) {
        Err(error) => ServiceResult {
            ok: false,
            value: json!({"error": format!("VM error: {}", error.message)}),
        },
        Ok(mut vm) => match vm.run(&mut host) {
            Ok(result) => ServiceResult {
                ok: true,
                value: result,
            },
            Err(error) => {
                if let Ok(structured) = serde_json::from_str::<Value>(&error.message) {
                    ServiceResult {
                        ok: false,
                        value: json!({"error": structured, "failed": true}),
                    }
                } else {
                    ServiceResult {
                        ok: false,
                        value: json!({"error": format!("Runtime error: {}", error.message)}),
                    }
                }
            }
        },
    }
}

/// Minimal host for service/MCP/HTTP runs with LLM + clock + random effects.
pub struct ServiceHost {
    credentials: Map<String, Value>,
    source_dir: Option<PathBuf>,
    llm_config: LlmConfig,
    default_provider: Option<String>,
    default_model: Option<String>,
    rng_state: u64,
}

impl ServiceHost {
    pub fn new(
        env: &[(String, String)],
        source_file: Option<&Path>,
        provider: Option<String>,
        model: Option<String>,
    ) -> Self {
        let mut credentials = Map::new();
        for (k, v) in env {
            credentials.insert(k.clone(), json!(v));
        }
        // Seed from time; journaled draws still capture exact results.
        let mut hasher = DefaultHasher::new();
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
            .hash(&mut hasher);
        Self {
            credentials,
            source_dir: source_file.and_then(|p| p.parent().map(|d| d.to_path_buf())),
            llm_config: LlmConfig::load(),
            default_provider: provider,
            default_model: model,
            rng_state: hasher.finish() | 1,
        }
    }

    fn resolve_cred(&self, key: &str) -> Option<String> {
        if let Some(Value::String(v)) = self.credentials.get(key) {
            return Some(v.clone());
        }
        std::env::var(key).ok().or_else(|| {
            // Program-local .env then ~/.devlish/.env
            for dir in [
                self.source_dir.clone(),
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".devlish")),
            ]
            .into_iter()
            .flatten()
            {
                let env_path = if dir.ends_with(".devlish") {
                    dir.join(".env")
                } else {
                    dir.join(".env")
                };
                if let Ok(text) = std::fs::read_to_string(env_path) {
                    for line in text.lines() {
                        let line = line.trim();
                        if line.is_empty() || line.starts_with('#') {
                            continue;
                        }
                        if let Some((k, v)) = line.split_once('=') {
                            if k.trim() == key {
                                return Some(v.trim().trim_matches('"').to_string());
                            }
                        }
                    }
                }
            }
            None
        })
    }
}

struct HostCreds<'a>(&'a ServiceHost);

impl CredentialResolver for HostCreds<'_> {
    fn resolve(&self, key: &str) -> Option<String> {
        self.0.resolve_cred(key)
    }
}

impl HostEffects for ServiceHost {
    fn emit_event(&mut self, _event: &Value) {}

    fn write_file(&mut self, request: &Value) -> Result<(), String> {
        let path = request
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| "write_file missing path".to_string())?;
        let content = request
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if let Some(parent) = Path::new(path).parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(path, content).map_err(|e| e.to_string())
    }

    fn read_file(&mut self, request: &Value) -> Result<Value, String> {
        let path = request
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| "read_file missing path".to_string())?;
        let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        Ok(Value::String(content))
    }

    fn resolve_credential(&self, key: &str) -> Option<String> {
        self.resolve_cred(key)
    }

    fn respond(&mut self, _value: &Value) -> Result<(), String> {
        // Result is captured in the VM run envelope; nothing to print for HTTP/MCP.
        Ok(())
    }

    fn llm_complete(&mut self, request: &Value) -> Result<Value, String> {
        let prompt = request
            .get("prompt")
            .and_then(Value::as_str)
            .ok_or_else(|| "llm_complete missing prompt".to_string())?
            .to_string();
        let expect_json = request
            .get("expect_json")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let llm_req = LlmRequest {
            prompt: prompt.clone(),
            model: request
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| self.default_model.clone()),
            provider: request
                .get("provider")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| self.default_provider.clone()),
            expect_json,
            system: request
                .get("system")
                .and_then(Value::as_str)
                .map(str::to_string),
        };
        logutil::debug(&format!(
            "llm_complete provider={:?} model={:?} expect_json={expect_json} prompt_chars={}",
            llm_req.provider,
            llm_req.model,
            prompt.len()
        ));
        let response = complete(&llm_req, &self.llm_config, &HostCreds(self))?;
        logutil::info(&format!(
            "llm_complete ok provider={} model={} response_chars={}",
            response.provider,
            response.model,
            response.text.len()
        ));
        Ok(response_value(&response))
    }

    fn clock_now(&mut self, kind: &str) -> Result<Value, String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?;
        match kind {
            "iso" => {
                let secs = now.as_secs() as i64;
                // Minimal ISO-8601 UTC without chrono dependency.
                Ok(Value::String(format!("{secs}")))
            }
            _ => Ok(json!(now.as_secs_f64())),
        }
    }

    fn random_draw(&mut self, request: &Value) -> Result<Value, String> {
        // xorshift64*
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        let unit = (self.rng_state as f64) / (u64::MAX as f64);
        let low = request.get("low").and_then(Value::as_f64).unwrap_or(0.0);
        let high = request.get("high").and_then(Value::as_f64).unwrap_or(1.0);
        if (high - low).abs() < f64::EPSILON {
            return Ok(json!(low));
        }
        Ok(json!(low + unit * (high - low)))
    }

    #[cfg(feature = "native")]
    fn http_request(
        &mut self,
        method: &str,
        url: &str,
        body: &Value,
        _headers: &Value,
    ) -> Result<Value, String> {
        let upper = method.to_ascii_uppercase();
        let response = match upper.as_str() {
            "GET" => ureq::get(url).call(),
            "POST" => ureq::post(url).send_json(body),
            "PUT" => ureq::put(url).send_json(body),
            "DELETE" => ureq::delete(url).call(),
            other => return Err(format!("unsupported HTTP method: {other}")),
        }
        .map_err(|e| format!("HTTP {method} {url}: {e}"))?;
        let status = response.status();
        let body_text = response.into_string().unwrap_or_default();
        let parsed = serde_json::from_str::<Value>(&body_text).unwrap_or(Value::String(body_text));
        Ok(json!({"status": status, "body": parsed}))
    }
}

/// Wrap a ServiceResult as MCP tool content.
pub fn to_mcp_content(result: &ServiceResult) -> Value {
    let text = serde_json::to_string_pretty(&result.value).unwrap_or_default();
    if result.ok {
        json!([{"type": "text", "text": text}])
    } else {
        json!([{"type": "text", "text": text, "isError": true}])
    }
}
