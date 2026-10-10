//! Shared compile / run / lint service used by MCP, HTTP serve, and harness.
//! Returns structured JSON Values (not MCP content wrappers).

use crate::{compile_source_to_json, lint_source, logutil, policy_log::PolicyLog, CompileOptions};
use devlish_llm::{complete, response_value, CredentialResolver, LlmConfig, LlmRequest};
use devlish_vm::{
    policy::{EffectPolicy, PolicyHost, PolicyRecorder},
    sha256_hex, HostEffects, Vm,
};
use serde_json::{json, Map, Value};
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
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
    /// Challenge / harness policy source path. Requires `policy_log`.
    pub policy_path: Option<PathBuf>,
    /// Hash-chained policy log path. Requires `policy_path`.
    pub policy_log: Option<PathBuf>,
    /// Operator default when the policy abstains. Requires `policy_path`.
    pub default_authorization: Option<String>,
    /// Trusted bounded execution controls; require policy recording.
    pub limits: Option<Value>,
    /// Trusted authoring contract; validates the complete model pair before writes.
    pub artifact_requirements: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct ServiceResult {
    pub ok: bool,
    pub value: Value,
}

pub fn service_compile(
    source: &str,
    source_path: Option<String>,
    search_paths: Vec<String>,
) -> ServiceResult {
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

pub fn execution_limits(
    value: &Value,
) -> Result<(u64, devlish_vm::effect_budget::EffectBudget), String> {
    let object = value.as_object().ok_or("limits must be an object")?;
    if object.len() != 2
        || !object.contains_key("instruction_limit")
        || !object.contains_key("effect_budget")
    {
        return Err("limits require only instruction_limit and effect_budget".into());
    }
    let instructions = value["instruction_limit"]
        .as_u64()
        .filter(|n| *n > 0 && *n <= 10_000_000)
        .ok_or("instruction_limit must be 1 through 10000000")?;
    Ok((
        instructions,
        devlish_vm::effect_budget::EffectBudget::parse(&value["effect_budget"])?,
    ))
}

pub fn service_run(req: RunRequest) -> ServiceResult {
    if req.policy_path.is_some() != req.policy_log.is_some() {
        return ServiceResult {
            ok: false,
            value: json!({"error": "--policy and --policy-log must be supplied together"}),
        };
    }
    if (req.default_authorization.is_some() || req.limits.is_some()) && req.policy_path.is_none() {
        return ServiceResult {
            ok: false,
            value: json!({
                "error": "--default-authorization requires --policy and --policy-log"
            }),
        };
    }
    if let Some(requirements) = &req.artifact_requirements {
        if req.policy_path.is_none() {
            return ServiceResult {
                ok: false,
                value: json!({"error": "artifact validation requires governed execution"}),
            };
        }
        if let Err(error) = crate::artifact_validation::validate_requirements(requirements) {
            return ServiceResult {
                ok: false,
                value: json!({"error": error}),
            };
        }
    }
    let compiled = service_compile(
        &req.source,
        req.source_path.clone(),
        req.search_paths.clone(),
    );
    if !compiled.ok {
        return compiled;
    }
    let mut package = compiled.value;
    // Governed runs treat absent declarations as no external authority.
    if req.policy_path.is_some() {
        let declared = package
            .pointer("/manifest/permissions")
            .and_then(Value::as_array)
            .is_some_and(|permissions| !permissions.is_empty());
        if !declared {
            let object = package.as_object_mut().unwrap();
            let manifest = object.entry("manifest").or_insert_with(|| json!({}));
            if !manifest.is_object() {
                *manifest = json!({});
            }
            manifest
                .as_object_mut()
                .unwrap()
                .insert("permissions".into(), json!([{"kind": "respond"}]));
        }
    }
    let source_file = req.source_path.as_deref().map(Path::new);
    let mut host = ServiceHost::new(
        &req.env,
        source_file,
        req.provider.clone(),
        req.model.clone(),
    );
    host.artifact_requirements = req.artifact_requirements.clone();
    let vm = match Vm::new(package.clone(), req.input.clone()) {
        Err(error) => {
            return ServiceResult {
                ok: false,
                value: json!({"error": format!("VM error: {}", error.message)}),
            }
        }
        Ok(vm) => vm,
    };

    if let (Some(policy_path), Some(log_path)) = (&req.policy_path, &req.policy_log) {
        return run_with_policy(vm, &mut host, &package, &req, policy_path, log_path);
    }

    let mut vm = vm;
    match vm.run(&mut host) {
        Ok(result) => ServiceResult {
            ok: true,
            value: result,
        },
        Err(error) => runtime_error_result(&error.message),
    }
}

fn run_with_policy(
    mut vm: Vm,
    host: &mut ServiceHost,
    package: &Value,
    req: &RunRequest,
    policy_path: &Path,
    log_path: &Path,
) -> ServiceResult {
    let policy_source = match fs::read_to_string(policy_path) {
        Ok(text) => text,
        Err(error) => {
            return ServiceResult {
                ok: false,
                value: json!({
                    "error": format!("failed to read policy {}: {error}", policy_path.display())
                }),
            }
        }
    };
    let policy_compiled = service_compile(
        &policy_source,
        Some(policy_path.display().to_string()),
        req.search_paths.clone(),
    );
    if !policy_compiled.ok {
        return policy_compiled;
    }
    let mut policy = match EffectPolicy::new(policy_compiled.value) {
        Ok(policy) => policy,
        Err(error) => {
            return ServiceResult {
                ok: false,
                value: json!({"error": format!("invalid policy: {error}")}),
            }
        }
    };
    if let Some(posture) = &req.default_authorization {
        if let Err(error) = policy.set_default_authorization(posture) {
            return ServiceResult {
                ok: false,
                value: json!({"error": error}),
            };
        }
    }
    let mut log = match PolicyLog::create_for_run(
        log_path,
        policy.identity(),
        package,
        &req.input,
        false,
        false,
    ) {
        Ok(log) => log,
        Err(error) => {
            return ServiceResult {
                ok: false,
                value: json!({"error": error}),
            }
        }
    };
    let controls = req.limits.clone().unwrap_or_else(|| {
        json!({
            "instruction_limit": 50000, "effect_budget": {"total": 100, "per_effect": {}}
        })
    });
    let (instructions, budget) = match execution_limits(&controls) {
        Ok(limits) => limits,
        Err(error) => {
            return ServiceResult {
                ok: false,
                value: json!({"error": error}),
            }
        }
    };
    if let Some(requirements) = &req.artifact_requirements {
        if let Err(error) = log.record(&json!({"type": "artifact_requirements_captured",
            "profile": "restricted-flat-devlish-v1",
            "requirements_sha256": sha256_hex(&serde_json::to_vec(requirements).unwrap()),
            "max_artifact_bytes": 131072}))
        {
            return ServiceResult {
                ok: false,
                value: json!({"error": error}),
            };
        }
    }
    vm.set_instruction_limit(instructions);
    if let Err(error) = log.record(&json!({"type": "execution_limits_captured",
        "instruction_limit": instructions, "effect_budget": budget.to_value()}))
    {
        return ServiceResult {
            ok: false,
            value: json!({"error": error}),
        };
    }
    vm.set_emit_events(false);
    let mut guarded = PolicyHost::new(host, &policy, &mut log).with_effect_budget(budget);
    let result = vm.run(&mut guarded);
    if guarded.recording_failed() {
        return ServiceResult {
            ok: false,
            value: json!({"error": "policy recording failed; run cannot report success"}),
        };
    }
    let result_value = match &result {
        Ok(value) => json!({"ok": value}),
        Err(error) => json!({"err": error.message}),
    };
    if let Err(error) = log.record(&json!({
        "type": "policy_run_finished",
        "success": result.is_ok(),
        "paused": result_value
            .pointer("/ok/is_checkpoint")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "result_sha256": sha256_hex(
            &serde_json::to_vec(&result_value).unwrap_or_else(|_| b"null".to_vec())
        )
    })) {
        return ServiceResult {
            ok: false,
            value: json!({"error": error}),
        };
    }
    match result {
        Ok(mut value) => {
            if let Some(object) = value.as_object_mut() {
                object.insert(
                    "policy_log".to_string(),
                    json!(log_path.display().to_string()),
                );
            }
            ServiceResult { ok: true, value }
        }
        Err(error) => runtime_error_result(&error.message),
    }
}

fn runtime_error_result(message: &str) -> ServiceResult {
    if let Ok(structured) = serde_json::from_str::<Value>(message) {
        ServiceResult {
            ok: false,
            value: json!({"error": structured, "failed": true}),
        }
    } else {
        ServiceResult {
            ok: false,
            value: json!({"error": format!("Runtime error: {message}")}),
        }
    }
}

/// Minimal host for service/MCP/HTTP runs with LLM + clock + random effects.
pub struct ServiceHost {
    credentials: Map<String, Value>,
    artifact_requirements: Option<Value>,
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
            artifact_requirements: None,
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

#[cfg(feature = "native")]
impl ServiceHost {
    #[cfg(feature = "native")]
    fn google_validation_request(&self, method: &str, url: &str) -> ureq::Request {
        // Disable redirects before attaching the host-owned, endpoint-scoped key.
        let request = ureq::AgentBuilder::new()
            .redirects(0)
            .build()
            .request(method, url);
        if method == "POST" && url == "https://addressvalidation.googleapis.com/v1:validateAddress"
        {
            if let Some(key) = self
                .resolve_cred("GOOGLE_ADDRESS_VALIDATION_API_KEY")
                .filter(|key| !key.is_empty())
            {
                return request.set("X-Goog-Api-Key", &key);
            }
        }
        request
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
        let mode = request
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("write");
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        if self.artifact_requirements.is_some() {
            // Authoring never replaces existing artifacts, including symlinks.
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options
                .open(path)
                .map_err(|_| "cannot exclusively create generated artifact".to_string())?;
            file.write_all(content.as_bytes())
                .map_err(|_| "cannot write generated artifact".to_string())?;
            return file
                .sync_all()
                .map_err(|_| "cannot sync generated artifact".to_string());
        }
        match mode {
            "append" => {
                let mut file = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .map_err(|e| format!("failed to open {path}: {e}"))?;
                file.write_all(content.as_bytes())
                    .map_err(|e| format!("failed to append {path}: {e}"))
            }
            "assertions" | "csv" | "export" | "overwrite" | "write" => {
                fs::write(path, content).map_err(|e| format!("failed to write {path}: {e}"))
            }
            other => Err(format!("unsupported write_file mode: {other}")),
        }
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
        if let Some(requirements) = &self.artifact_requirements {
            let payload = response
                .parsed
                .as_ref()
                .ok_or("authoring requires a parsed JSON artifact pair")?;
            crate::artifact_validation::validate_draft(payload, requirements)?;
        }
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
            "POST" => self.google_validation_request(&upper, url).send_json(body),
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

#[cfg(all(test, feature = "native"))]
mod credential_tests {
    use super::*;

    #[test]
    fn google_key_is_scoped_to_exact_post_endpoint() {
        let host = ServiceHost::new(
            &[(
                "GOOGLE_ADDRESS_VALIDATION_API_KEY".into(),
                "synthetic-key".into(),
            )],
            None,
            None,
            None,
        );
        let url = "https://addressvalidation.googleapis.com/v1:validateAddress";
        assert_eq!(
            host.google_validation_request("POST", url)
                .header("X-Goog-Api-Key"),
            Some("synthetic-key")
        );
        for (method, endpoint) in [
            ("GET", url),
            (
                "POST",
                "https://addressvalidation.googleapis.com/v1:validateAddress?extra=true",
            ),
            ("POST", "https://example.com/v1:validateAddress"),
        ] {
            assert_eq!(
                host.google_validation_request(method, endpoint)
                    .header("X-Goog-Api-Key"),
                None
            );
        }
    }
}
