//! Outbound LLM providers for the Devlish harness.
//!
//! Devlish programs call models through a journaled host effect. This crate
//! resolves the user's configured provider (Anthropic, OpenAI, or Ollama) and
//! performs the HTTP completion. Secrets never leave the credential chain.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    pub prompt: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub expect_json: bool,
    #[serde(default)]
    pub system: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub text: String,
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub parsed: Option<Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default = "default_provider")]
    pub default_provider: String,
    #[serde(default = "default_model")]
    pub default_model: String,
    #[serde(default)]
    pub anthropic: ProviderConfig,
    #[serde(default)]
    pub openai: ProviderConfig,
    #[serde(default)]
    pub openrouter: ProviderConfig,
    #[serde(default)]
    pub ollama: ProviderConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderConfig {
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub api_key_env: Option<String>,
    #[serde(default)]
    pub default_model: Option<String>,
}

fn default_provider() -> String {
    "openai".to_string()
}

fn default_model() -> String {
    "gpt-4o-mini".to_string()
}

impl LlmConfig {
    pub fn load() -> Self {
        let path = config_path();
        if path.is_file() {
            if let Ok(text) = fs::read_to_string(&path) {
                if let Ok(cfg) = toml::from_str::<LlmConfig>(&text) {
                    return cfg;
                }
            }
        }
        Self::default()
    }

    pub fn ensure_default_file() -> Result<PathBuf, String> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create {}: {e}", parent.display()))?;
        }
        if !path.is_file() {
            let sample = r#"# Devlish outbound LLM harness config
default_provider = "openai"
default_model = "gpt-4o-mini"

[openai]
api_key_env = "OPENAI_API_KEY"
# base_url = "https://api.openai.com/v1"
default_model = "gpt-4o-mini"

[openrouter]
api_key_env = "OPENROUTER_API_KEY"
base_url = "https://openrouter.ai/api/v1"
default_model = "openai/gpt-4o-mini"

[anthropic]
api_key_env = "ANTHROPIC_API_KEY"
# base_url = "https://api.anthropic.com"

[ollama]
base_url = "http://127.0.0.1:11434/v1"
default_model = "llama3.2"
"#;
            fs::write(&path, sample)
                .map_err(|e| format!("failed to write {}: {e}", path.display()))?;
        }
        Ok(path)
    }
}

pub fn config_path() -> PathBuf {
    if let Ok(override_path) = env::var("DEVLISH_CONFIG") {
        return PathBuf::from(override_path);
    }
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home).join(".devlish").join("config.toml");
    }
    PathBuf::from(".devlish").join("config.toml")
}

pub trait CredentialResolver {
    fn resolve(&self, key: &str) -> Option<String>;
}

/// Complete a prompt using the configured provider.
pub fn complete(
    request: &LlmRequest,
    config: &LlmConfig,
    credentials: &dyn CredentialResolver,
) -> Result<LlmResponse, String> {
    let provider_name = request
        .provider
        .as_deref()
        .unwrap_or(&config.default_provider)
        .to_ascii_lowercase();
    let model = request
        .model
        .clone()
        .or_else(|| match provider_name.as_str() {
            "anthropic" => config.anthropic.default_model.clone(),
            "openai" => config.openai.default_model.clone(),
            "openrouter" => config.openrouter.default_model.clone(),
            "ollama" => config.ollama.default_model.clone(),
            _ => None,
        })
        .unwrap_or_else(|| config.default_model.clone());

    let mut response = match provider_name.as_str() {
        "anthropic" => complete_anthropic(request, &model, &config.anthropic, credentials)?,
        "openai" => complete_openai_compatible(
            request,
            &model,
            &config.openai,
            credentials,
            "openai",
            "https://api.openai.com/v1",
            "OPENAI_API_KEY",
        )?,
        "openrouter" => complete_openai_compatible(
            request,
            &model,
            &config.openrouter,
            credentials,
            "openrouter",
            "https://openrouter.ai/api/v1",
            "OPENROUTER_API_KEY",
        )?,
        "ollama" => complete_openai_compatible(
            request,
            &model,
            &config.ollama,
            credentials,
            "ollama",
            "http://127.0.0.1:11434/v1",
            "",
        )?,
        other => return Err(format!("unknown LLM provider: {other}")),
    };

    if request.expect_json {
        let trimmed = response.text.trim();
        let json_text = extract_json_blob(trimmed).unwrap_or(trimmed);
        response.parsed = Some(
            serde_json::from_str(json_text)
                .map_err(|e| format!("model did not return valid JSON: {e}"))?,
        );
    }

    Ok(response)
}

fn extract_json_blob(text: &str) -> Option<&str> {
    if let Some(start) = text.find("```") {
        let after = &text[start + 3..];
        let after = after
            .strip_prefix("json")
            .or_else(|| after.strip_prefix("JSON"))
            .unwrap_or(after)
            .trim_start_matches(['\r', '\n']);
        if let Some(end) = after.find("```") {
            return Some(after[..end].trim());
        }
    }
    let start = text.find(['{', '['])?;
    let end = text.rfind(['}', ']'])?;
    if end >= start {
        Some(&text[start..=end])
    } else {
        None
    }
}

fn complete_anthropic(
    request: &LlmRequest,
    model: &str,
    provider: &ProviderConfig,
    credentials: &dyn CredentialResolver,
) -> Result<LlmResponse, String> {
    let key_env = provider
        .api_key_env
        .as_deref()
        .unwrap_or("ANTHROPIC_API_KEY");
    let api_key = credentials
        .resolve(key_env)
        .or_else(|| env::var(key_env).ok())
        .ok_or_else(|| format!("missing API key ({key_env}) for Anthropic"))?;
    let base = provider
        .base_url
        .as_deref()
        .unwrap_or("https://api.anthropic.com");
    let url = format!("{}/v1/messages", base.trim_end_matches('/'));

    let mut body = json!({
        "model": model,
        "max_tokens": 4096,
        "messages": [{"role": "user", "content": request.prompt}]
    });
    if let Some(system) = &request.system {
        body.as_object_mut()
            .unwrap()
            .insert("system".to_string(), json!(system));
    }

    let response = ureq::post(&url)
        .set("x-api-key", &api_key)
        .set("anthropic-version", "2023-06-01")
        .set("content-type", "application/json")
        .send_json(&body)
        .map_err(|e| format!("Anthropic request failed: {e}"))?;

    let value: Value = response
        .into_json()
        .map_err(|e| format!("Anthropic response parse failed: {e}"))?;
    let text = value
        .get("content")
        .and_then(Value::as_array)
        .and_then(|blocks| {
            blocks.iter().find_map(|block| {
                if block.get("type").and_then(Value::as_str) == Some("text") {
                    block.get("text").and_then(Value::as_str).map(str::to_string)
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| format!("Anthropic response missing text content: {value}"))?;

    Ok(LlmResponse {
        text,
        provider: "anthropic".to_string(),
        model: model.to_string(),
        parsed: None,
    })
}

fn complete_openai_compatible(
    request: &LlmRequest,
    model: &str,
    provider: &ProviderConfig,
    credentials: &dyn CredentialResolver,
    provider_name: &str,
    default_base: &str,
    default_key_env: &str,
) -> Result<LlmResponse, String> {
    let base = provider
        .base_url
        .as_deref()
        .unwrap_or(default_base)
        .trim_end_matches('/');
    let url = format!("{base}/chat/completions");

    let mut messages = Vec::new();
    if let Some(system) = &request.system {
        messages.push(json!({"role": "system", "content": system}));
    }
    messages.push(json!({"role": "user", "content": request.prompt}));

    let body = json!({
        "model": model,
        "messages": messages
    });

    let mut req = ureq::post(&url).set("content-type", "application/json");
    let key_env = provider
        .api_key_env
        .as_deref()
        .unwrap_or(default_key_env);
    if !key_env.is_empty() {
        if let Some(api_key) = credentials
            .resolve(key_env)
            .or_else(|| env::var(key_env).ok())
        {
            req = req.set("authorization", &format!("Bearer {api_key}"));
        } else if provider_name != "ollama" {
            return Err(format!("missing API key ({key_env}) for {provider_name}"));
        }
    }

    let response = req
        .send_json(&body)
        .map_err(|e| format!("{provider_name} request failed: {e}"))?;
    let value: Value = response
        .into_json()
        .map_err(|e| format!("{provider_name} response parse failed: {e}"))?;
    let text = value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("{provider_name} response missing content: {value}"))?;

    Ok(LlmResponse {
        text,
        provider: provider_name.to_string(),
        model: model.to_string(),
        parsed: None,
    })
}

/// Build the structured value programs see after `Ask the model`.
pub fn response_value(response: &LlmResponse) -> Value {
    let mut map = serde_json::Map::new();
    map.insert("text".to_string(), json!(response.text));
    map.insert("provider".to_string(), json!(response.provider));
    map.insert("model".to_string(), json!(response.model));
    if let Some(parsed) = &response.parsed {
        map.insert("json".to_string(), parsed.clone());
    }
    Value::Object(map)
}

/// Resolve a credential from an optional path-aware store callback.
pub struct EnvCredentials;

impl CredentialResolver for EnvCredentials {
    fn resolve(&self, key: &str) -> Option<String> {
        env::var(key).ok()
    }
}

pub fn config_dir() -> PathBuf {
    config_path()
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_fenced_json() {
        let text = "Here you go:\n```json\n{\"a\": 1}\n```\n";
        assert_eq!(extract_json_blob(text), Some("{\"a\": 1}"));
    }

    #[test]
    fn default_config_loads() {
        let cfg = LlmConfig::default();
        assert_eq!(cfg.default_provider, "openai");
    }
}
