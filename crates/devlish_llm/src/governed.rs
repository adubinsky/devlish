//! Approved model routing, independent of mutable user provider configuration.
//! Callers must obtain this configuration from their authenticated release.
use crate::{CredentialResolver, LlmResponse};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::{self, Read, Write},
    time::Duration,
};

const ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";

/// Validated immutable route. No caller-selected URL, headers, proxy or defaults.
#[derive(Clone, Debug)]
pub struct ApprovedModel(Route);
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Route {
    format: String,
    format_version: u32,
    provider: String,
    model: String,
    credential: String,
    timeout_seconds: u64,
    max_request_bytes: usize,
    max_response_bytes: usize,
    #[serde(default)]
    max_tokens: Option<u32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    batch: Option<Value>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    provider: Option<String>,
    expect_json: bool,
    #[serde(default)]
    system: Option<String>,
}
impl ApprovedModel {
    pub fn parse(value: &Value) -> Result<Self, String> {
        let r: Route =
            serde_json::from_value(value.clone()).map_err(|_| "invalid approved model route")?;
        if r.format != "devlish-model-route"
            || r.format_version != 1
            || !["openrouter", "jev"].contains(&r.provider.as_str())
            || r.model.is_empty()
            || r.model.len() > 128
            || !r
                .model
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/_-.:".contains(&b))
            || r.credential.is_empty()
            || r.credential.len() > 128
            || !r
                .credential
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || !r.credential.as_bytes()[0].is_ascii_alphabetic()
            || !(1..=120).contains(&r.timeout_seconds)
            || !(1..=1_048_576).contains(&r.max_request_bytes)
            || !(1..=8_388_608).contains(&r.max_response_bytes)
            || (r.provider == "openrouter"
                && !r.max_tokens.is_some_and(|n| (1..=32_768).contains(&n)))
            || (r.provider == "jev"
                && (r.max_tokens.is_some()
                    || r.credential != "JEV_API_KEY"
                    || !r.model.starts_with("jev-")
                    || ["jev-latest", "jev-preview"].contains(&r.model.as_str())
                    || r.max_request_bytes > 1_048_576
                    || r.max_response_bytes > 1_048_576))
        {
            return Err("unsupported approved model route or limits".into());
        }
        Ok(Self(r))
    }

    fn body(&self, value: &Value) -> Result<(Vec<u8>, bool), String> {
        // Bound before cloning/deserializing the untrusted strings.
        let mut counter = SizeLimit(self.0.max_request_bytes);
        serde_json::to_writer(&mut counter, value)
            .map_err(|_| "model request exceeds approved limit")?;
        let request: Request =
            serde_json::from_value(value.clone()).map_err(|_| "invalid model request fields")?;
        if request.model.as_ref().is_some_and(|v| v != &self.0.model)
            || request
                .provider
                .as_ref()
                .is_some_and(|v| v != &self.0.provider)
        {
            return Err("model request conflicts with approved route".into());
        }
        if request.batch.is_some() {
            return Err("chat model cannot accept a JEV batch".into());
        }
        let prompt = request.prompt.ok_or("chat model requires a prompt")?;
        let mut messages = vec![];
        if let Some(system) = request.system {
            messages.push(json!({"role":"system","content":system}));
        }
        messages.push(json!({"role":"user","content":prompt}));
        let body = serde_json::to_vec(&json!({"model":self.0.model,"messages":messages,
            "stream":false,"max_tokens":self.0.max_tokens}))
        .expect("JSON serializes");
        if body.len() > self.0.max_request_bytes {
            return Err("model request exceeds approved limit".into());
        }
        Ok((body, request.expect_json))
    }

    /// Makes one request, with no automatic retry or redirect. A transport error
    /// can mean the provider received the request; callers must not blindly retry.
    pub fn complete(
        &self,
        value: &Value,
        credentials: &dyn CredentialResolver,
    ) -> Result<LlmResponse, String> {
        if self.0.provider == "jev" {
            return self.complete_jev(value, credentials, &mut |_| {});
        }
        self.send(value, credentials, ENDPOINT, true)
    }

    /// Same immutable route, with safe provider diagnostics supplied by the host.
    pub fn complete_with_trace(
        &self,
        value: &Value,
        credentials: &dyn CredentialResolver,
        trace: &mut dyn FnMut(Value),
    ) -> Result<LlmResponse, String> {
        if self.0.provider == "jev" {
            self.complete_jev(value, credentials, trace)
        } else {
            self.complete(value, credentials)
        }
    }

    fn jev_batch(&self, value: &Value) -> Result<crate::jev::Batch, String> {
        let mut counter = SizeLimit(self.0.max_request_bytes);
        serde_json::to_writer(&mut counter, value)
            .map_err(|_| "model request exceeds approved limit")?;
        let r: Request =
            serde_json::from_value(value.clone()).map_err(|_| "invalid model request fields")?;
        if r.provider.as_deref() != Some("jev")
            || r.prompt.is_some()
            || r.system.is_some()
            || !r.expect_json
            || r.model.as_ref().is_some_and(|m| m != &self.0.model)
        {
            return Err("model request conflicts with approved route".into());
        }
        let batch: crate::jev::Batch =
            serde_json::from_value(r.batch.ok_or("JEV requires a typed batch")?)
                .map_err(|_| "invalid JEV batch fields")?;
        if batch.model != self.0.model {
            return Err("model request conflicts with approved route".into());
        }
        Ok(batch)
    }

    fn complete_jev(
        &self,
        value: &Value,
        credentials: &dyn CredentialResolver,
        trace: &mut dyn FnMut(Value),
    ) -> Result<LlmResponse, String> {
        let batch = self.jev_batch(value)?;
        let result = crate::jev::evaluate_approved(
            &batch,
            credentials,
            trace,
            crate::jev::Limits {
                timeout: Duration::from_secs(self.0.timeout_seconds),
                max_request_bytes: self.0.max_request_bytes,
                max_response_bytes: self.0.max_response_bytes,
            },
        )
        .map_err(|e| e.to_string())?;
        let model = result.model.clone();
        let parsed = serde_json::to_value(result).map_err(|_| "JEV result serialization failed")?;
        Ok(LlmResponse {
            text: String::new(),
            provider: "jev".into(),
            model,
            parsed: Some(parsed),
        })
    }

    // Destination is only supplied by complete (fixed HTTPS endpoint) or unit tests.
    fn send(
        &self,
        value: &Value,
        credentials: &dyn CredentialResolver,
        endpoint: &str,
        https_only: bool,
    ) -> Result<LlmResponse, String> {
        let (body, expect_json) = self.body(value)?;
        let key = credentials
            .resolve(&self.0.credential)
            .filter(|s| !s.is_empty())
            .ok_or("approved model credential unavailable")?;
        let timeout = Duration::from_secs(self.0.timeout_seconds);
        let agent = ureq::AgentBuilder::new()
            .redirects(0)
            .try_proxy_from_env(false)
            .https_only(https_only)
            .timeout(timeout)
            .timeout_connect(timeout)
            .build();
        let response = agent
            .post(endpoint)
            .set("content-type", "application/json")
            .set("authorization", &format!("Bearer {key}"))
            .send_bytes(&body)
            .map_err(|_| "approved model transport failed; outcome uncertain")?;
        if !(200..300).contains(&response.status()) {
            return Err("approved model returned non-success status".into());
        }
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(self.0.max_response_bytes as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "approved model response could not be read")?;
        if bytes.len() > self.0.max_response_bytes {
            return Err("model response exceeds approved limit".into());
        }
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| "approved model response is not JSON")?;
        let text = value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .ok_or("approved model response missing text")?
            .to_owned();
        let parsed = if expect_json {
            Some(
                serde_json::from_str(text.trim())
                    .map_err(|_| "model content is not strict JSON")?,
            )
        } else {
            None
        };
        Ok(LlmResponse {
            text,
            provider: self.0.provider.clone(),
            model: self.0.model.clone(),
            parsed,
        })
    }
}

// Check serialized input size without allocating a second unbounded copy.
struct SizeLimit(usize);
impl Write for SizeLimit {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_sub(bytes.len())
            .ok_or_else(|| io::Error::other("size limit"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{net::TcpListener, sync::mpsc, thread};
    fn route() -> Value {
        json!({"format":"devlish-model-route","format_version":1,"provider":"openrouter",
            "model":"synthetic/model","credential":"SYNTHETIC_MODEL_KEY","timeout_seconds":1,
            "max_request_bytes":4096,"max_response_bytes":4096,"max_tokens":100})
    }
    struct Key;
    fn jev_route() -> Value {
        json!({"format":"devlish-model-route","format_version":1,"provider":"jev",
            "model":"jev-1.13.0","credential":"JEV_API_KEY","timeout_seconds":1,
            "max_request_bytes":4096,"max_response_bytes":4096})
    }
    fn jev_request() -> Value {
        let mut batch: Value =
            serde_json::from_str(include_str!("../tests/fixtures/jev_batch.json")).unwrap();
        batch["model"] = json!("jev-1.13.0");
        json!({"provider":"jev","expect_json":true,"batch":batch})
    }

    #[test]
    fn approved_jev_route_binds_model_credential_and_limits() {
        let route = ApprovedModel::parse(&jev_route()).unwrap();
        assert_eq!(route.jev_batch(&jev_request()).unwrap().model, "jev-1.13.0");
        for (field, value) in [
            ("credential", json!("OTHER_KEY")),
            ("model", json!("jev-latest")),
            ("endpoint", json!("https://unapproved.invalid")),
            ("max_tokens", json!(100)),
            ("max_response_bytes", json!(1_048_577)),
        ] {
            let mut config = jev_route();
            config[field] = value;
            assert!(ApprovedModel::parse(&config).is_err());
        }
        let mut config = jev_route();
        config["max_request_bytes"] = json!(100);
        assert!(ApprovedModel::parse(&config)
            .unwrap()
            .jev_batch(&jev_request())
            .is_err());
    }

    #[test]
    fn jev_caller_cannot_override_the_approved_transport_or_switch_provider() {
        let route = ApprovedModel::parse(&jev_route()).unwrap();
        for (field, value) in [
            ("provider", json!("openrouter")),
            ("prompt", json!("private")),
            ("system", json!("private")),
            ("expect_json", json!(false)),
            ("model", json!("jev-1.12.0")),
            ("endpoint", json!("http://unapproved.invalid")),
        ] {
            let mut request = jev_request();
            request[field] = value;
            assert!(route.jev_batch(&request).is_err());
        }
        let mut request = jev_request();
        request["batch"]["model"] = json!("jev-preview");
        assert!(route.jev_batch(&request).is_err());
        let chat_route = ApprovedModel::parse(&self::route()).unwrap();
        assert!(chat_route.body(&jev_request()).is_err());
    }
    impl CredentialResolver for Key {
        fn resolve(&self, key: &str) -> Option<String> {
            assert_eq!(key, "SYNTHETIC_MODEL_KEY");
            Some("synthetic-not-a-secret".into())
        }
    }
    // All sockets bind an ephemeral loopback port and have bounded accept/read.
    fn server(
        response: String,
        delay: Duration,
    ) -> (
        String,
        mpsc::Receiver<(String, Value)>,
        thread::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!("http://{}/chat/completions", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            let mut stream = loop {
                if let Ok((stream, _)) = listener.accept() {
                    break stream;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "test request did not arrive"
                );
                thread::sleep(Duration::from_millis(5));
            };
            // Accepted sockets inherit nonblocking mode on macOS. Use bounded
            // blocking reads after the bounded listener accept loop.
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut header = vec![];
            while !header.ends_with(b"\r\n\r\n") {
                let mut b = [0];
                stream.read_exact(&mut b).unwrap();
                header.push(b[0]);
                assert!(header.len() < 8192);
            }
            let header = String::from_utf8(header).unwrap();
            let size: usize = header
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .map(str::to_string)
                })
                .unwrap()
                .parse()
                .unwrap();
            assert!(size < 8192);
            let mut body = vec![0; size];
            stream.read_exact(&mut body).unwrap();
            tx.send((header, serde_json::from_slice(&body).unwrap()))
                .unwrap();
            thread::sleep(delay);
            let _ = stream.write_all(response.as_bytes());
        });
        (endpoint, rx, handle)
    }
    fn http(body: &str) -> String {
        format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len())
    }
    #[test]
    fn approved_route_rejects_unknown_destinations_and_unbounded_controls() {
        for (field, value) in [
            ("provider", json!("ollama")),
            ("base_url", json!("http://127.0.0.1")),
            ("credential", json!("KEY\r\nInjected")),
            ("timeout_seconds", json!(0)),
            ("timeout_seconds", json!(121)),
            ("max_request_bytes", json!(1_048_577)),
            ("max_response_bytes", json!(8_388_609)),
            ("max_tokens", json!(0)),
            ("model", json!("")),
        ] {
            let mut r = route();
            r[field] = value;
            assert!(ApprovedModel::parse(&r).is_err(), "{field}");
        }
    }
    #[test]
    fn caller_cannot_override_route_or_add_transport_fields() {
        let approved = ApprovedModel::parse(&route()).unwrap();
        for (field, value) in [
            ("model", json!("other/model")),
            ("provider", json!("openai")),
            ("url", json!("https://unapproved.invalid")),
            ("headers", json!({})),
            ("tools", json!([])),
            ("max_tokens", json!(999)),
        ] {
            let mut request = json!({"prompt":"public","expect_json":true});
            request[field] = value;
            assert!(approved.body(&request).is_err(), "{field}");
        }
        assert!(approved.body(&json!({"prompt":"public","expect_json":true,"provider":"openrouter","model":"synthetic/model"})).is_ok());
    }
    #[test]
    fn one_bounded_request_uses_only_approved_route_and_returns_strict_json() {
        let approved = ApprovedModel::parse(&route()).unwrap();
        let (url, rx, handle) = server(
            http(r#"{"choices":[{"message":{"content":"{\"steps\":[]}"}}]}"#),
            Duration::ZERO,
        );
        let response = approved
            .send(
                &json!({"prompt":"synthetic public tokens","expect_json":true}),
                &Key,
                &url,
                false,
            )
            .unwrap();
        let (header, body) = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        handle.join().unwrap();
        assert!(header
            .to_ascii_lowercase()
            .contains("authorization: bearer synthetic-not-a-secret"));
        assert_eq!(
            body,
            json!({"model":"synthetic/model","messages":[{"role":"user","content":"synthetic public tokens"}],"max_tokens":100,"stream":false})
        );
        assert_eq!(response.parsed, Some(json!({"steps":[]})));
    }
    #[test]
    fn requests_and_responses_obey_byte_limits_including_encoded_body_overhead() {
        let mut config = route();
        config["max_request_bytes"] = json!(80);
        let approved = ApprovedModel::parse(&config).unwrap();
        assert!(approved
            .body(&json!({"prompt":"x".repeat(1000),"expect_json":false}))
            .is_err());
        // Small input still exceeds the approved budget after adding model/messages.
        assert!(approved
            .body(&json!({"prompt":"x","expect_json":false}))
            .is_err());
        config = route();
        config["max_response_bytes"] = json!(32);
        let approved = ApprovedModel::parse(&config).unwrap();
        let (url, rx, handle) = server(http(&"x".repeat(1000)), Duration::ZERO);
        let error = approved
            .send(
                &json!({"prompt":"public","expect_json":false}),
                &Key,
                &url,
                false,
            )
            .unwrap_err();
        rx.recv_timeout(Duration::from_secs(2)).unwrap();
        handle.join().unwrap();
        assert_eq!(error, "model response exceeds approved limit");
    }
    #[test]
    fn redirect_does_not_contact_unapproved_destination() {
        let target = TcpListener::bind("127.0.0.1:0").unwrap();
        target.set_nonblocking(true).unwrap();
        let response=format!("HTTP/1.1 307 Temporary Redirect\r\nLocation: http://{}/leak\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",target.local_addr().unwrap());
        let (url, rx, handle) = server(response, Duration::ZERO);
        let error = ApprovedModel::parse(&route())
            .unwrap()
            .send(
                &json!({"prompt":"public","expect_json":false}),
                &Key,
                &url,
                false,
            )
            .unwrap_err();
        rx.recv_timeout(Duration::from_secs(2)).unwrap();
        handle.join().unwrap();
        assert_eq!(error, "approved model returned non-success status");
        assert_eq!(
            target.accept().unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }
    #[test]
    fn malformed_private_provider_errors_are_not_returned_to_program() {
        for body in [
            "SYNTHETIC_PRIVATE_PROVIDER_ERROR",
            r#"{"error":"SYNTHETIC_PRIVATE_PROVIDER_ERROR"}"#,
            r#"{"choices":[{"message":{"content":"SYNTHETIC_PRIVATE_PROVIDER_ERROR"}}]}"#,
        ] {
            let (url, rx, handle) = server(http(body), Duration::ZERO);
            let error = ApprovedModel::parse(&route())
                .unwrap()
                .send(
                    &json!({"prompt":"public","expect_json":true}),
                    &Key,
                    &url,
                    false,
                )
                .unwrap_err();
            rx.recv_timeout(Duration::from_secs(2)).unwrap();
            handle.join().unwrap();
            assert!(!error.contains("SYNTHETIC_PRIVATE"));
        }
    }
    #[test]
    fn absent_resolver_key_fails_before_any_network_request() {
        struct NoneKey;
        impl CredentialResolver for NoneKey {
            fn resolve(&self, _: &str) -> Option<String> {
                None
            }
        }
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let error = ApprovedModel::parse(&route())
            .unwrap()
            .send(
                &json!({"prompt":"public","expect_json":false}),
                &NoneKey,
                &url,
                false,
            )
            .unwrap_err();
        assert_eq!(error, "approved model credential unavailable");
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }
    #[test]
    fn production_transport_rejects_plain_http_before_dispatch() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let result = ApprovedModel::parse(&route()).unwrap().send(
            &json!({"prompt":"public","expect_json":false}),
            &Key,
            &url,
            true,
        );
        assert!(result.is_err());
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn slow_provider_fails_with_uncertain_outcome_without_retry() {
        let (url, rx, handle) = server(http("{}"), Duration::from_millis(1400));
        let error = ApprovedModel::parse(&route())
            .unwrap()
            .send(
                &json!({"prompt":"public","expect_json":false}),
                &Key,
                &url,
                false,
            )
            .unwrap_err();
        rx.recv_timeout(Duration::from_secs(2)).unwrap();
        handle.join().unwrap();
        assert_eq!(error, "approved model transport failed; outcome uncertain");
    }
}
