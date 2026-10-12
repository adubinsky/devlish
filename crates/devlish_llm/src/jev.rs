//! Typed TypeSafe JEV batches. This is deliberately separate from chat completion.
//! Provider responses are data; they never authorize checklist state changes.

use crate::CredentialResolver;
use ring::digest::{digest, SHA256};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::Read,
    time::{Duration, Instant},
};

const ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
const MAX_BYTES: usize = 1_048_576;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Batch {
    pub correlation_id: String,
    pub idempotency_key: String,
    pub document_id: String,
    pub model: String,
    /// Readable text, not an attachment URL or a PDF byte stream.
    pub document_text: String,
    pub questions: BTreeMap<String, VersionedQuestion>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionedQuestion {
    pub version: String,
    pub question: Question,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Question {
    Choice {
        instructions: String,
        criteria: BTreeMap<String, String>,
    },
    Score {
        instructions: String,
        criteria: Vec<String>,
    },
    Noul {
        instructions: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Answer {
    Choice {
        choice: String,
        confidence: f64,
        probabilities: BTreeMap<String, f64>,
    },
    Score {
        score: f64,
        confidence: f64,
        probabilities: BTreeMap<String, f64>,
        legend: BTreeMap<String, String>,
    },
    /// This is P(yes), not a confidence in a separately selected boolean.
    Noul { noul: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Deserialize)]
struct WireResponse {
    model: String,
    answers: BTreeMap<String, Answer>,
    usage: Usage,
}

#[derive(Debug, Serialize)]
pub struct ResultBatch {
    pub provider: String,
    pub model: String,
    pub correlation_id: String,
    pub idempotency_key: String,
    pub document_id: String,
    pub document_sha256: String,
    pub question_versions: BTreeMap<String, String>,
    pub request_id: Option<String>,
    pub answers: BTreeMap<String, Answer>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    Authentication,
    InvalidRequest,
    RateLimit,
    Unavailable,
    Timeout,
    Transport,
    InvalidResponse,
}

#[derive(Debug, Serialize)]
pub struct Error {
    pub category: ErrorCategory,
    pub retryable: bool,
    /// A transport failure can happen after the provider accepted the request.
    pub outcome_uncertain: bool,
    pub http_status: Option<u16>,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "JEV {:?} (retryable={}, outcome_uncertain={})",
            self.category, self.retryable, self.outcome_uncertain
        )
    }
}
impl std::error::Error for Error {}

fn error(category: ErrorCategory) -> Error {
    Error {
        category,
        retryable: false,
        outcome_uncertain: false,
        http_status: None,
    }
}

fn identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-.:".contains(&b))
}

impl Batch {
    fn body(&self) -> Result<Vec<u8>, Error> {
        if !identifier(&self.correlation_id)
            || !identifier(&self.idempotency_key)
            || !identifier(&self.document_id)
            || !identifier(&self.model)
            || !self.model.starts_with("jev-")
            || self.document_text.trim().is_empty()
            || self.document_text.len() > MAX_BYTES
            || self.questions.is_empty()
            || self.questions.len() > 100
        {
            return Err(error(ErrorCategory::InvalidRequest));
        }
        for (key, q) in &self.questions {
            if !identifier(key) || !identifier(&q.version) {
                return Err(error(ErrorCategory::InvalidRequest));
            }
            let (instructions, valid) = match &q.question {
                Question::Choice {
                    instructions,
                    criteria,
                } => (
                    instructions,
                    (2..=255).contains(&criteria.len()) && criteria.keys().all(|s| identifier(s)),
                ),
                Question::Score {
                    instructions,
                    criteria,
                } => (
                    instructions,
                    (2..=10).contains(&criteria.len())
                        && criteria.iter().all(|s| !s.trim().is_empty()),
                ),
                Question::Noul { instructions } => (instructions, true),
            };
            if instructions.trim().is_empty() || instructions.len() > 16_384 || !valid {
                return Err(error(ErrorCategory::InvalidRequest));
            }
        }
        let questions: BTreeMap<_, _> = self
            .questions
            .iter()
            .map(|(k, v)| (k, &v.question))
            .collect();
        let body = serde_json::to_vec(
            &json!({"state":self.document_text,"model":self.model,"questions":questions}),
        )
        .map_err(|_| error(ErrorCategory::InvalidRequest))?;
        if body.len() > MAX_BYTES {
            return Err(error(ErrorCategory::InvalidRequest));
        }
        Ok(body)
    }

    pub fn document_sha256(&self) -> String {
        digest(&SHA256, self.document_text.as_bytes())
            .as_ref()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// Explicit allowlist: no state, instructions, answers, credentials or provider body.
    fn trace(&self, stage: &str, attempt: usize, start: Instant) -> Value {
        json!({"event":format!("jev.{stage}"),"correlation_id":self.correlation_id,
            "idempotency_key":self.idempotency_key,"document_id":self.document_id,
            "document_sha256":self.document_sha256(),"provider":"jev","requested_model":self.model,
            "question_versions":self.questions.iter().map(|(k,q)| (k.clone(),q.version.clone())).collect::<BTreeMap<_,_>>(),
            "attempt":attempt,"duration_ms":start.elapsed().as_millis()})
    }
}

/// Decode both live and recorded provider responses through the same validator.
pub fn decode(
    batch: &Batch,
    bytes: &[u8],
    request_id: Option<String>,
) -> Result<ResultBatch, Error> {
    batch.body()?;
    if bytes.len() > MAX_BYTES {
        return Err(error(ErrorCategory::InvalidResponse));
    }
    let r: WireResponse =
        serde_json::from_slice(bytes).map_err(|_| error(ErrorCategory::InvalidResponse))?;
    if !identifier(&r.model)
        || !r.model.starts_with("jev-")
        || (batch.model != "jev-latest" && batch.model != "jev-preview" && batch.model != r.model)
        || !r.answers.keys().eq(batch.questions.keys())
    {
        return Err(error(ErrorCategory::InvalidResponse));
    }
    for (key, answer) in &r.answers {
        let valid = match (&batch.questions[key].question, answer) {
            (Question::Noul { .. }, Answer::Noul { noul }) => probability(*noul),
            (
                Question::Choice { criteria, .. },
                Answer::Choice {
                    choice,
                    confidence,
                    probabilities,
                },
            ) => {
                criteria.contains_key(choice)
                    && probability(*confidence)
                    && probabilities.keys().eq(criteria.keys())
                    && distribution(probabilities)
                    && probabilities
                        .values()
                        .all(|p| *p <= probabilities[choice] + 0.001)
            }
            (
                Question::Score { criteria, .. },
                Answer::Score {
                    score,
                    confidence,
                    probabilities,
                    legend,
                },
            ) => {
                let keys: Vec<_> = (0..criteria.len()).map(|i| i.to_string()).collect();
                score.is_finite()
                    && *score >= 0.0
                    && *score <= (criteria.len() - 1) as f64
                    && probability(*confidence)
                    && distribution(probabilities)
                    && probabilities.len() == keys.len()
                    && legend.len() == keys.len()
                    && keys
                        .iter()
                        .all(|k| probabilities.contains_key(k) && legend.contains_key(k))
                    && keys
                        .iter()
                        .enumerate()
                        .all(|(i, k)| legend[k] == criteria[i])
                    && (keys
                        .iter()
                        .enumerate()
                        .map(|(i, k)| i as f64 * probabilities[k])
                        .sum::<f64>()
                        - score)
                        .abs()
                        <= 0.001
            }
            _ => false,
        };
        if !valid {
            return Err(error(ErrorCategory::InvalidResponse));
        }
    }
    Ok(ResultBatch {
        provider: "jev".into(),
        model: r.model,
        correlation_id: batch.correlation_id.clone(),
        idempotency_key: batch.idempotency_key.clone(),
        document_id: batch.document_id.clone(),
        document_sha256: batch.document_sha256(),
        question_versions: batch
            .questions
            .iter()
            .map(|(k, q)| (k.clone(), q.version.clone()))
            .collect(),
        request_id: request_id.filter(|s| identifier(s)),
        answers: r.answers,
        usage: r.usage,
    })
}

fn probability(p: f64) -> bool {
    p.is_finite() && (0.0..=1.0).contains(&p)
}
fn distribution(p: &BTreeMap<String, f64>) -> bool {
    !p.is_empty()
        && p.values().all(|v| probability(*v))
        && (p.values().sum::<f64>() - 1.0).abs() <= 0.001
}

/// Only an authenticated model route supplies these transport bounds.
pub(crate) struct Limits {
    pub timeout: Duration,
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
}

/// One authorized effect is exactly one provider attempt. Any retry needs a new
/// policy decision and consumes the existing model-effect budget.
pub(crate) fn evaluate_approved(
    batch: &Batch,
    credentials: &dyn CredentialResolver,
    trace: &mut dyn FnMut(Value),
    limits: Limits,
) -> Result<ResultBatch, Error> {
    send_config(batch, credentials, trace, ENDPOINT, true, limits)
}

#[cfg(test)]
fn send(
    batch: &Batch,
    credentials: &dyn CredentialResolver,
    trace: &mut dyn FnMut(Value),
    endpoint: &str,
    https_only: bool,
    backoff: Duration,
) -> Result<ResultBatch, Error> {
    send_with_timeout(
        batch,
        credentials,
        trace,
        endpoint,
        https_only,
        backoff,
        Duration::from_secs(30),
    )
}

#[cfg(test)]
fn send_with_timeout(
    batch: &Batch,
    credentials: &dyn CredentialResolver,
    trace: &mut dyn FnMut(Value),
    endpoint: &str,
    https_only: bool,
    _backoff: Duration,
    timeout: Duration,
) -> Result<ResultBatch, Error> {
    send_config(
        batch,
        credentials,
        trace,
        endpoint,
        https_only,
        Limits {
            timeout,
            max_request_bytes: MAX_BYTES,
            max_response_bytes: MAX_BYTES,
        },
    )
}

fn send_config(
    batch: &Batch,
    credentials: &dyn CredentialResolver,
    trace: &mut dyn FnMut(Value),
    endpoint: &str,
    https_only: bool,
    limits: Limits,
) -> Result<ResultBatch, Error> {
    let start = Instant::now();
    let mut last_attempt = 0;
    let result = (|| {
        let body = batch.body()?;
        if body.len() > limits.max_request_bytes {
            return Err(error(ErrorCategory::InvalidRequest));
        }
        trace(batch.trace("accepted", 0, start));
        let key = credentials
            .resolve("JEV_API_KEY")
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| error(ErrorCategory::Authentication))?;
        let agent = ureq::AgentBuilder::new()
            .redirects(0)
            .try_proxy_from_env(false)
            .https_only(https_only)
            .timeout(limits.timeout)
            .timeout_connect(limits.timeout)
            .build();
        {
            let attempt = 1;
            last_attempt = attempt;
            trace(batch.trace("started", attempt, start));
            let response = agent
                .post(endpoint)
                .set("Authorization", &format!("Bearer {key}"))
                .set("Content-Type", "application/json")
                .set("Idempotency-Key", &batch.idempotency_key)
                .set("X-Correlation-ID", &batch.correlation_id)
                .send_bytes(&body);
            match response {
                Ok(response) => {
                    if !(200..300).contains(&response.status()) {
                        return Err(http_error(response.status()));
                    }
                    let request_id = response.header("x-request-id").map(str::to_owned);
                    let mut bytes = Vec::new();
                    response
                        .into_reader()
                        .take(limits.max_response_bytes as u64 + 1)
                        .read_to_end(&mut bytes)
                        .map_err(|e| uncertain_transport(is_timeout(&e)))?;
                    if bytes.len() > limits.max_response_bytes {
                        return Err(error(ErrorCategory::InvalidResponse));
                    }
                    let result = decode(batch, &bytes, request_id).inspect_err(|_| {
                        trace(batch.trace("validation_failed", attempt, start));
                    })?;
                    trace(batch.trace("decoded", attempt, start));
                    let mut event = batch.trace("completed", attempt, start);
                    event["actual_model"] = json!(result.model);
                    event["request_id"] = json!(result.request_id);
                    event["usage"] = json!(result.usage);
                    trace(event);
                    Ok(result)
                }
                Err(ureq::Error::Status(status, _)) => Err(http_error(status)),
                Err(ureq::Error::Transport(t)) => {
                    use std::error::Error as _;
                    let timeout = t
                        .source()
                        .and_then(|e| e.downcast_ref::<std::io::Error>())
                        .is_some_and(is_timeout);
                    Err(uncertain_transport(timeout))
                }
            }
        }
    })();
    if let Err(e) = &result {
        // Invalid input is not echoed, even through its metadata fields.
        let mut event = json!({"event":"jev.failed","error_category":e.category,
            "retryable":e.retryable,"outcome_uncertain":e.outcome_uncertain,"http_status":e.http_status});
        if batch.body().is_ok() {
            event["context"] = batch.trace("failed", last_attempt, start);
        }
        trace(event);
    }
    result
}

fn is_timeout(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
    )
}

fn uncertain_transport(timeout: bool) -> Error {
    let mut e = error(if timeout {
        ErrorCategory::Timeout
    } else {
        ErrorCategory::Transport
    });
    e.outcome_uncertain = true;
    e
}

fn http_error(status: u16) -> Error {
    let category = match status {
        401 | 403 => ErrorCategory::Authentication,
        400 | 413 | 415 | 422 => ErrorCategory::InvalidRequest,
        429 => ErrorCategory::RateLimit,
        500..=599 => ErrorCategory::Unavailable,
        _ => ErrorCategory::InvalidResponse,
    };
    Error {
        category,
        retryable: status == 429 || status == 529,
        outcome_uncertain: false,
        http_status: Some(status),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    fn batch() -> Batch {
        serde_json::from_str(include_str!("../tests/fixtures/jev_batch.json")).unwrap()
    }
    fn response() -> Value {
        serde_json::from_str(include_str!("../tests/fixtures/jev_response.json")).unwrap()
    }
    struct Credentials;
    impl CredentialResolver for Credentials {
        fn resolve(&self, key: &str) -> Option<String> {
            assert_eq!(key, "JEV_API_KEY");
            Some("secret-test-key".into())
        }
    }

    // Value: protects=typed batch question, criteria and instruction resource bounds;
    // fails_when=empty or over-limit collections/instructions are accepted;
    // why_new=existing invalid-input cases do not exercise these limits;
    // seam=none; no_claim=live provider schema acceptance.
    #[test]
    fn batch_question_criteria_and_instruction_bounds_are_enforced() {
        let template = batch().questions["signed"].clone();
        for (count, valid) in [(0, false), (100, true), (101, false)] {
            let mut b = batch();
            b.questions = (0..count)
                .map(|i| (format!("q{i}"), template.clone()))
                .collect();
            assert_eq!(b.body().is_ok(), valid, "question count {count}");
        }
        for (count, valid) in [(1, false), (2, true), (255, true), (256, false)] {
            let mut b = batch();
            b.questions.get_mut("document_type").unwrap().question = Question::Choice {
                instructions: "Choose a supplied criterion".into(),
                criteria: (0..count)
                    .map(|i| (format!("c{i}"), "Criterion".into()))
                    .collect(),
            };
            assert_eq!(b.body().is_ok(), valid, "choice count {count}");
        }
        for (count, valid) in [(1, false), (2, true), (10, true), (11, false)] {
            let mut b = batch();
            b.questions.get_mut("readability").unwrap().question = Question::Score {
                instructions: "Score using supplied criteria".into(),
                criteria: vec!["Criterion".into(); count],
            };
            assert_eq!(b.body().is_ok(), valid, "score count {count}");
        }
        for (instructions, valid) in [
            (" ".into(), false),
            ("x".repeat(16_384), true),
            ("x".repeat(16_385), false),
        ] {
            let mut b = batch();
            b.questions.get_mut("signed").unwrap().question = Question::Noul { instructions };
            assert_eq!(b.body().is_ok(), valid, "instruction bound");
        }
        let mut b = batch();
        b.questions.get_mut("readability").unwrap().question = Question::Score {
            instructions: "Score".into(),
            criteria: vec!["Valid".into(), " ".into()],
        };
        assert_eq!(
            b.body().unwrap_err().category,
            ErrorCategory::InvalidRequest
        );
    }

    // Value: protects=choice selection and score/legend consistency with approved questions;
    // fails_when=nonmaximal choices, inconsistent expected scores or changed legends pass;
    // why_new=existing invalid-answer cases exercise other rejection paths;
    // seam=none; no_claim=model answer quality.
    #[test]
    fn answer_semantics_match_the_approved_question() {
        let mut variants = Vec::new();
        let mut r = response();
        r["answers"]["document_type"]["choice"] = json!("other");
        variants.push(r);
        let mut r = response();
        r["answers"]["readability"]["score"] = json!(1.0);
        variants.push(r);
        let mut r = response();
        r["answers"]["readability"]["legend"]["2"] = json!("Unapproved criterion");
        variants.push(r);
        for r in variants {
            assert_eq!(
                decode(&batch(), &serde_json::to_vec(&r).unwrap(), None)
                    .unwrap_err()
                    .category,
                ErrorCategory::InvalidResponse
            );
        }
        let mut r = response();
        r["answers"]["document_type"]["probabilities"] =
            json!({"insurance":0.5,"other":0.5,"uncertain":0.0});
        r["answers"]["readability"]["score"] = json!(1.0);
        r["answers"]["readability"]["probabilities"] = json!({"0":0.25,"1":0.5,"2":0.25});
        assert!(decode(&batch(), &serde_json::to_vec(&r).unwrap(), None).is_ok());
    }

    // Value: protects=operator-selected response byte limit below the global decoder cap;
    // fails_when=transport ignores its configured cap or rejects exact-limit responses;
    // why_new=global oversize decode test does not cover the signed transport bound;
    // seam=none; no_claim=production HTTPS provider acceptance.
    #[test]
    fn transport_enforces_the_configured_response_byte_limit() {
        let body = response().to_string();
        assert!(body.len() < MAX_BYTES);
        for (limit, valid) in [(body.len(), true), (body.len() - 1, false)] {
            let (endpoint, server) = server(vec![200], body.clone());
            let mut events = Vec::new();
            let result = send_config(
                &batch(),
                &Credentials,
                &mut |v| events.push(v),
                &endpoint,
                false,
                Limits {
                    timeout: Duration::from_secs(2),
                    max_request_bytes: MAX_BYTES,
                    max_response_bytes: limit,
                },
            );
            assert_eq!(server.join().unwrap().len(), 1);
            if valid {
                assert_eq!(result.unwrap().model, "jev-1.13.0");
                assert_eq!(events.last().unwrap()["event"], "jev.completed");
            } else {
                assert_eq!(result.unwrap_err().category, ErrorCategory::InvalidResponse);
                assert_eq!(events.last().unwrap()["event"], "jev.failed");
                assert!(!events.iter().any(|v| v["event"] == "jev.completed"));
            }
        }
    }

    #[test]
    fn typed_batch_preserves_provenance_and_noul_probability() {
        let b = batch();
        let r = decode(
            &b,
            &serde_json::to_vec(&response()).unwrap(),
            Some("req-123".into()),
        )
        .unwrap();
        assert_eq!(r.model, "jev-1.13.0");
        assert_eq!(r.question_versions["signed"], "v1");
        assert_eq!(r.correlation_id, b.correlation_id);
        assert_eq!(r.document_sha256.len(), 64);
        assert!(matches!(r.answers["signed"], Answer::Noul { noul } if noul == 0.01));
        let mut changed = batch();
        changed.document_text.push(' ');
        assert_ne!(r.document_sha256, changed.document_sha256());
        let wire: Value = serde_json::from_slice(&b.body().unwrap()).unwrap();
        assert!(wire.get("correlation_id").is_none());
        assert!(wire["questions"]["signed"].get("version").is_none());
    }

    #[test]
    fn rejects_missing_extra_mismatched_and_invalid_answers() {
        let original = response();
        let mut variants = Vec::new();
        let mut r = original.clone();
        r["answers"].as_object_mut().unwrap().remove("signed");
        variants.push(r);
        let mut r = original.clone();
        r["answers"]["extra"] = json!({"type":"noul","noul":0.5});
        variants.push(r);
        let mut r = original.clone();
        r["answers"]["signed"] = r["answers"]["document_type"].clone();
        variants.push(r);
        let mut r = original.clone();
        r["answers"]["signed"]["noul"] = json!(1.1);
        variants.push(r);
        let mut r = original.clone();
        r["answers"]["document_type"]["choice"] = json!("invented");
        variants.push(r);
        let mut r = original.clone();
        r["answers"]["document_type"]["confidence"] = json!(-0.1);
        variants.push(r);
        let mut r = original.clone();
        r["answers"]["document_type"]["probabilities"]["other"] = json!(0.5);
        variants.push(r);
        let mut r = original.clone();
        r["answers"]["readability"]["score"] = json!(3);
        variants.push(r);
        let mut r = original.clone();
        r["answers"]["readability"]["legend"]
            .as_object_mut()
            .unwrap()
            .remove("2");
        variants.push(r);
        let mut r = original;
        r["model"] = json!("openai-model");
        variants.push(r);
        for r in variants {
            assert_eq!(
                decode(&batch(), &serde_json::to_vec(&r).unwrap(), None)
                    .unwrap_err()
                    .category,
                ErrorCategory::InvalidResponse
            );
        }
        assert_eq!(
            decode(&batch(), b"not JSON", None).unwrap_err().category,
            ErrorCategory::InvalidResponse
        );
        assert_eq!(
            decode(&batch(), &vec![b' '; MAX_BYTES + 1], None)
                .unwrap_err()
                .category,
            ErrorCategory::InvalidResponse
        );
    }

    #[test]
    fn pinned_model_and_input_validation() {
        let mut b = batch();
        b.model = "jev-1.12.0".into();
        assert_eq!(
            decode(&b, &serde_json::to_vec(&response()).unwrap(), None)
                .unwrap_err()
                .category,
            ErrorCategory::InvalidResponse
        );
        b.document_text.clear();
        assert_eq!(
            b.body().unwrap_err().category,
            ErrorCategory::InvalidRequest
        );
        b = batch();
        b.correlation_id = "private text\nunsafe".into();
        let mut events = Vec::new();
        let e = send(
            &b,
            &Credentials,
            &mut |v| events.push(v),
            "http://127.0.0.1:1",
            false,
            Duration::ZERO,
        )
        .unwrap_err();
        assert_eq!(e.category, ErrorCategory::InvalidRequest);
        assert!(!serde_json::to_string(&events)
            .unwrap()
            .contains("private text"));
    }

    // Loopback only. Read the exact body to test retries reuse the same request.
    fn server(statuses: Vec<u16>, body: String) -> (String, thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/v1/systemone", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
            let mut requests = Vec::new();
            for status in statuses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut one = [0];
                while !bytes.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut one).unwrap();
                    bytes.push(one[0]);
                }
                let headers = String::from_utf8(bytes.clone()).unwrap();
                let length: usize = headers
                    .lines()
                    .find_map(|l| {
                        l.to_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse().unwrap())
                    })
                    .unwrap();
                let mut payload = vec![0; length];
                stream.read_exact(&mut payload).unwrap();
                bytes.extend(payload);
                requests.push(String::from_utf8(bytes).unwrap());
                let location = if status == 302 {
                    "Location: http://127.0.0.1:1/private-redirect\r\n"
                } else {
                    ""
                };
                write!(stream, "HTTP/1.1 {status} Test\r\n{location}Content-Length: {}\r\nContent-Type: application/json\r\nX-Request-ID: req-test\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            requests
        });
        (endpoint, handle)
    }

    #[test]
    fn one_attempt_preserves_correlation_and_safe_logs() {
        let (endpoint, server) = server(vec![200], response().to_string());
        let mut events = Vec::new();
        let r = send(
            &batch(),
            &Credentials,
            &mut |v| events.push(v),
            &endpoint,
            false,
            Duration::ZERO,
        )
        .unwrap();
        let requests = server.join().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(requests[0]
            .to_lowercase()
            .contains("x-correlation-id: synthetic-check-001"));
        assert!(requests[0]
            .to_lowercase()
            .contains("idempotency-key: synthetic-check-001:v1"));
        assert_eq!(r.request_id.as_deref(), Some("req-test"));
        assert_eq!(
            events.iter().filter(|v| v["event"] == "jev.retry").count(),
            0
        );
        assert_eq!(events.last().unwrap()["event"], "jev.completed");
        let logs = serde_json::to_string(&events).unwrap();
        for private in [
            "secret-test-key",
            "Example Solar",
            "Classify the document",
            "probabilities",
        ] {
            assert!(!logs.contains(private));
        }
    }

    #[test]
    fn authentication_and_bad_response_fail_safely_without_retry() {
        for (status, expected) in [
            (401, ErrorCategory::Authentication),
            (200, ErrorCategory::InvalidResponse),
            (302, ErrorCategory::InvalidResponse),
        ] {
            let (endpoint, server) =
                server(vec![status], "private provider body secret-test-key".into());
            let mut events = Vec::new();
            let e = send(
                &batch(),
                &Credentials,
                &mut |v| events.push(v),
                &endpoint,
                false,
                Duration::ZERO,
            )
            .unwrap_err();
            assert_eq!(e.category, expected);
            assert_eq!(server.join().unwrap().len(), 1);
            assert!(!serde_json::to_string(&events)
                .unwrap()
                .contains("private provider body"));
            assert!(!e.to_string().contains("secret-test-key"));
            assert_eq!(events.last().unwrap()["event"], "jev.failed");
        }
    }

    #[test]
    fn slow_provider_times_out_without_retry() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_millis(250));
            drop(stream);
        });
        let mut events = Vec::new();
        let e = send_with_timeout(
            &batch(),
            &Credentials,
            &mut |v| events.push(v),
            &endpoint,
            false,
            Duration::ZERO,
            Duration::from_millis(50),
        )
        .unwrap_err();
        assert_eq!(e.category, ErrorCategory::Timeout);
        assert!(e.outcome_uncertain);
        assert!(!e.retryable);
        assert_eq!(
            events
                .iter()
                .filter(|v| v["event"] == "jev.started")
                .count(),
            1
        );
        server.join().unwrap();
    }

    #[test]
    fn absent_credentials_fail_before_dispatch() {
        struct Missing;
        impl CredentialResolver for Missing {
            fn resolve(&self, _: &str) -> Option<String> {
                None
            }
        }
        let mut events = Vec::new();
        let e = send(
            &batch(),
            &Missing,
            &mut |v| events.push(v),
            "http://127.0.0.1:1",
            false,
            Duration::ZERO,
        )
        .unwrap_err();
        assert_eq!(e.category, ErrorCategory::Authentication);
        assert_eq!(
            events
                .iter()
                .filter(|v| v["event"] == "jev.started")
                .count(),
            0
        );
    }

    #[test]
    fn overload_does_not_retry_outside_the_effect_budget() {
        for status in [429, 529] {
            let (endpoint, server) = server(vec![status], "private error body".into());
            let mut events = Vec::new();
            let e = send(
                &batch(),
                &Credentials,
                &mut |v| events.push(v),
                &endpoint,
                false,
                Duration::ZERO,
            )
            .unwrap_err();
            assert_eq!(
                e.category,
                if status == 429 {
                    ErrorCategory::RateLimit
                } else {
                    ErrorCategory::Unavailable
                }
            );
            assert!(e.retryable);
            assert!(!e.outcome_uncertain);
            assert_eq!(server.join().unwrap().len(), 1);
        }
    }

    #[test]
    fn transport_failure_is_uncertain_and_never_retried() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let mut events = Vec::new();
        let e = send(
            &batch(),
            &Credentials,
            &mut |v| events.push(v),
            &endpoint,
            false,
            Duration::ZERO,
        )
        .unwrap_err();
        assert_eq!(e.category, ErrorCategory::Transport);
        assert!(e.outcome_uncertain);
        assert!(!e.retryable);
        assert_eq!(
            events
                .iter()
                .filter(|v| v["event"] == "jev.started")
                .count(),
            1
        );
    }
}
