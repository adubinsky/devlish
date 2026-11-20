//! Local job queue for the Aside browser bridge.
//! The native host long-polls this queue. Verbs are admitted by a Devlish
//! effect policy under deny-unless-allowed. Send, submit, and click verbs are
//! rejected before that policy runs.

use crate::{compile_source_to_json, logutil, CompileOptions};
use devlish_vm::policy::EffectPolicy;
use serde_json::{json, Map, Value};
use std::collections::{HashMap, VecDeque};
use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DEFAULT_POLICY: &str = include_str!("../../../examples/browser_bridge/policy.dvl");
const DEFAULT_RATE_LIMIT: u64 = 30;
const DEFAULT_RATE_WINDOW_SECS: u64 = 3600;
const DEFAULT_IN_PROGRESS_TIMEOUT_SECS: u64 = 120;
const MAX_WAIT_SECS: u64 = 60;
const SUMMARY_CHARS: usize = 80;

/// Send, submit, and click commands are rejected before policy evaluation.
/// A policy file cannot allow them.
pub fn verb_is_forbidden(verb: &str) -> bool {
    const STEMS: [&str; 3] = ["send", "submit", "click"];
    verb.split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .any(|token| {
            let lower = token.to_ascii_lowercase();
            STEMS.iter().any(|stem| lower.starts_with(stem))
        })
}

pub fn is_local_bridge_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://127.0.0.1:") else {
        return false;
    };
    match rest.find('/') {
        Some(index) => rest[index..].starts_with("/bridge/"),
        None => false,
    }
}

/// Token the workflow host attaches to local bridge calls.
pub fn shared_token() -> Result<String, String> {
    let path = token_path_from_env();
    read_token(&path).map_err(|error| format!("bridge token file {}: {error}", path.display()))
}

pub fn token_path_from_env() -> PathBuf {
    if let Some(path) = env_path("ASIDE_BRIDGE_TOKEN_FILE") {
        return path;
    }
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".config/aside-bridge/token")
}

pub struct BridgeOverrides {
    pub token_file: Option<PathBuf>,
    pub policy_file: Option<PathBuf>,
    pub log_file: Option<PathBuf>,
    pub rate_limit: Option<u64>,
    pub rate_window_secs: Option<u64>,
    pub in_progress_timeout_secs: Option<u64>,
}

pub struct BridgeResponse {
    pub status: u16,
    pub body: Option<Value>,
}

pub struct Bridge {
    token_path: PathBuf,
    cv: Condvar,
    inner: Mutex<BridgeInner>,
}

struct BridgeInner {
    policy: EffectPolicy,
    log_path: PathBuf,
    rate_limit: u64,
    rate_window: Duration,
    timeout: Duration,
    next_id: u64,
    jobs: HashMap<String, Job>,
    queued: VecDeque<String>,
    accepted_at: VecDeque<Instant>,
}

#[derive(Clone)]
struct Job {
    id: String,
    verb: String,
    args: Value,
    status: Status,
    result: Option<Value>,
    error: Option<Value>,
    created_at: String,
    updated_at: String,
    started: Option<Instant>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Queued,
    InProgress,
    Done,
    Failed,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::InProgress => "in_progress",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }

    fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Failed)
    }
}

impl Bridge {
    pub fn open(overrides: BridgeOverrides) -> Result<Self, String> {
        let token_path = overrides
            .token_file
            .or_else(|| env_path("ASIDE_BRIDGE_TOKEN_FILE"))
            .unwrap_or_else(token_path_from_env);
        let log_path = overrides
            .log_file
            .or_else(|| env_path("ASIDE_BRIDGE_LOG"))
            .unwrap_or_else(|| {
                token_path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .join("jobs.jsonl")
            });
        let rate_limit = pick_u64(
            "bridge rate limit",
            overrides.rate_limit,
            "ASIDE_BRIDGE_RATE_LIMIT",
            DEFAULT_RATE_LIMIT,
        )?;
        let rate_window_secs = pick_u64(
            "bridge rate window",
            overrides.rate_window_secs,
            "ASIDE_BRIDGE_RATE_WINDOW_SECS",
            DEFAULT_RATE_WINDOW_SECS,
        )?;
        let timeout_secs = pick_u64(
            "bridge in-progress timeout",
            overrides.in_progress_timeout_secs,
            "ASIDE_BRIDGE_IN_PROGRESS_TIMEOUT_SECS",
            DEFAULT_IN_PROGRESS_TIMEOUT_SECS,
        )?;
        if rate_limit == 0 {
            return Err("bridge rate limit must be at least 1".into());
        }
        if rate_window_secs == 0 {
            return Err("bridge rate window must be at least 1 second".into());
        }
        if timeout_secs == 0 {
            return Err("bridge in-progress timeout must be at least 1 second".into());
        }
        let policy_source = match overrides
            .policy_file
            .or_else(|| env_path("ASIDE_BRIDGE_POLICY"))
        {
            Some(path) => fs::read_to_string(&path)
                .map_err(|error| format!("bridge policy {}: {error}", path.display()))?,
            None => DEFAULT_POLICY.to_string(),
        };
        let policy = compile_policy(&policy_source)?;
        let policy_id = policy
            .identity()
            .pointer("/rule/id")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        logutil::info(&format!(
            "bridge queue token_file={} log={} rate={rate_limit}/{rate_window_secs}s timeout={timeout_secs}s policy={policy_id}",
            token_path.display(),
            log_path.display()
        ));
        Ok(Self {
            token_path,
            cv: Condvar::new(),
            inner: Mutex::new(BridgeInner {
                policy,
                log_path,
                rate_limit,
                rate_window: Duration::from_secs(rate_window_secs),
                timeout: Duration::from_secs(timeout_secs),
                next_id: 0,
                jobs: HashMap::new(),
                queued: VecDeque::new(),
                accepted_at: VecDeque::new(),
            }),
        })
    }

    pub fn handle(
        &self,
        method: &str,
        path: &str,
        query: &str,
        token: Option<&str>,
        body: &str,
    ) -> BridgeResponse {
        if !self.authorized(token) {
            return error_response(401, "X-Bridge-Token is missing or does not match");
        }
        if path == "/bridge/jobs" && method == "POST" {
            return self.submit(body);
        }
        if path == "/bridge/jobs/next" && method == "GET" {
            return self.next_job(query);
        }
        if let Some(rest) = path.strip_prefix("/bridge/jobs/") {
            if let Some(id) = rest.strip_suffix("/result") {
                if method == "POST" {
                    return self.finish(id, body);
                }
            } else if !rest.is_empty() && method == "GET" {
                return self.get(rest, query);
            }
        }
        if path == "/bridge/jobs"
            || path == "/bridge/jobs/next"
            || path.starts_with("/bridge/jobs/")
        {
            return error_response(405, format!("method not allowed: {method} {path}"));
        }
        error_response(404, format!("not found: {method} {path}"))
    }

    fn authorized(&self, presented: Option<&str>) -> bool {
        let Ok(expected) = read_token(&self.token_path) else {
            return false;
        };
        match presented {
            Some(token) => tokens_match(&expected, token.trim()),
            None => false,
        }
    }

    fn submit(&self, body: &str) -> BridgeResponse {
        let payload = match parse_object(body) {
            Ok(value) => value,
            Err(response) => return response,
        };
        let verb = match payload.get("verb").and_then(Value::as_str) {
            Some(verb) => verb.trim().to_string(),
            None => return error_response(400, "verb is required"),
        };
        if verb.is_empty() {
            return error_response(400, "verb is required");
        }
        let args = match payload.get("args") {
            None => json!({}),
            Some(Value::Object(map)) => Value::Object(map.clone()),
            Some(_) => return error_response(400, "args must be an object"),
        };
        if verb_is_forbidden(&verb) {
            let message = format!(
                "verb is not allowed: {verb}. send, submit, and click commands are rejected"
            );
            return self.reject(&verb, &args, 403, &message);
        }
        let decision = {
            let inner = self.lock();
            inner.policy.evaluate(&verb, &args)
        };
        match decision {
            Ok((true, _)) => {}
            Ok((false, reason)) => {
                let message = format!("verb is not allowed: {verb}. {reason}");
                return self.reject(&verb, &args, 403, &message);
            }
            Err(error) => {
                let message =
                    format!("verb is not allowed: {verb}. policy decision failed: {error}");
                return self.reject(&verb, &args, 403, &message);
            }
        }
        let mut inner = self.lock();
        if let Err(error) = expire(&mut inner) {
            return error_response(500, format!("failed to record bridge job: {error}"));
        }
        if !consume_rate(&mut inner) {
            let message = format!(
                "bridge rate limit exceeded: {} jobs per {} seconds",
                inner.rate_limit,
                inner.rate_window.as_secs()
            );
            if let Err(error) = log_rejection(&inner, &verb, &args, "rate_limited", &message) {
                return error_response(500, format!("failed to record bridge job: {error}"));
            }
            logutil::info(&format!("bridge rate limited {verb}"));
            return error_response(429, message);
        }
        inner.next_id += 1;
        let id = format!("job-{}", inner.next_id);
        let now = rfc3339_now();
        let job = Job {
            id: id.clone(),
            verb,
            args,
            status: Status::Queued,
            result: None,
            error: None,
            created_at: now.clone(),
            updated_at: now,
            started: None,
        };
        let line = log_line(&job);
        inner.jobs.insert(id.clone(), job);
        inner.queued.push_back(id.clone());
        if let Err(error) = append_log(&inner.log_path, &line) {
            inner.jobs.remove(&id);
            inner.queued.pop_back();
            inner.accepted_at.pop_back();
            return error_response(500, format!("failed to record bridge job: {error}"));
        }
        if let Some(job) = inner.jobs.get(&id) {
            logutil::info(&format!(
                "bridge job {} {} {}",
                job.id,
                job.verb,
                job.status.as_str()
            ));
        }
        self.cv.notify_all();
        json_response(201, json!({"id": id, "status": "queued"}))
    }

    fn next_job(&self, query: &str) -> BridgeResponse {
        let wait_secs = match required_wait(query) {
            Ok(wait) => wait,
            Err(response) => return response,
        };
        let deadline = Instant::now() + Duration::from_secs(wait_secs);
        let mut inner = self.lock();
        loop {
            if let Err(error) = expire(&mut inner) {
                return error_response(500, format!("failed to record bridge job: {error}"));
            }
            match claim_next(&mut inner) {
                Ok(Some(job)) => {
                    self.cv.notify_all();
                    return json_response(200, job);
                }
                Ok(None) => {}
                Err(error) => {
                    return error_response(500, format!("failed to record bridge job: {error}"))
                }
            }
            let now = Instant::now();
            if now >= deadline {
                return BridgeResponse {
                    status: 204,
                    body: None,
                };
            }
            let slice = wait_slice(&inner, deadline);
            if slice.is_zero() {
                continue;
            }
            let (guard, _) = self
                .cv
                .wait_timeout(inner, slice)
                .unwrap_or_else(|error| error.into_inner());
            inner = guard;
        }
    }

    fn get(&self, id: &str, query: &str) -> BridgeResponse {
        if !valid_id(id) {
            return error_response(404, "job not found");
        }
        let wait_secs = match optional_wait(query) {
            Ok(wait) => wait,
            Err(response) => return response,
        };
        let deadline = Instant::now() + Duration::from_secs(wait_secs.unwrap_or(0));
        let mut inner = self.lock();
        loop {
            if let Err(error) = expire(&mut inner) {
                return error_response(500, format!("failed to record bridge job: {error}"));
            }
            let Some(job) = inner.jobs.get(id) else {
                return error_response(404, "job not found");
            };
            if job.status.is_terminal() || wait_secs.is_none() || Instant::now() >= deadline {
                return json_response(200, full_view(job));
            }
            let slice = wait_slice(&inner, deadline);
            if slice.is_zero() {
                continue;
            }
            let (guard, _) = self
                .cv
                .wait_timeout(inner, slice)
                .unwrap_or_else(|error| error.into_inner());
            inner = guard;
        }
    }

    fn finish(&self, id: &str, body: &str) -> BridgeResponse {
        if !valid_id(id) {
            return error_response(404, "job not found");
        }
        let payload = match parse_object(body) {
            Ok(value) => value,
            Err(response) => return response,
        };
        let Some(ok) = payload.get("ok").and_then(Value::as_bool) else {
            return error_response(400, "result requires boolean ok");
        };
        let mut inner = self.lock();
        if let Err(error) = expire(&mut inner) {
            return error_response(500, format!("failed to record bridge job: {error}"));
        }
        let Some(job) = inner.jobs.get(id).cloned() else {
            return error_response(404, "job not found");
        };
        if job.status != Status::InProgress {
            return error_response(409, format!("job is {}", job.status.as_str()));
        }
        let mut updated = job.clone();
        if ok {
            updated.status = Status::Done;
            updated.result = Some(payload.get("result").cloned().unwrap_or(Value::Null));
            updated.error = None;
        } else {
            updated.status = Status::Failed;
            updated.result = None;
            updated.error = Some(
                payload
                    .get("error")
                    .cloned()
                    .unwrap_or_else(|| json!("bridge host reported failure")),
            );
        }
        updated.updated_at = rfc3339_now();
        updated.started = None;
        let line = log_line(&updated);
        if let Err(error) = append_log(&inner.log_path, &line) {
            return error_response(500, format!("failed to record bridge job: {error}"));
        }
        let view = full_view(&updated);
        logutil::info(&format!(
            "bridge job {} {} {}",
            updated.id,
            updated.verb,
            updated.status.as_str()
        ));
        inner.jobs.insert(id.to_string(), updated);
        self.cv.notify_all();
        json_response(200, view)
    }

    fn reject(&self, verb: &str, args: &Value, status: u16, message: &str) -> BridgeResponse {
        let inner = self.lock();
        if let Err(error) = log_rejection(&inner, verb, args, "rejected", message) {
            return error_response(500, format!("failed to record bridge job: {error}"));
        }
        logutil::info(&format!("bridge rejected {verb}"));
        error_response(status, message)
    }

    fn lock(&self) -> MutexGuard<'_, BridgeInner> {
        self.inner.lock().unwrap_or_else(|error| error.into_inner())
    }
}

fn compile_policy(source: &str) -> Result<EffectPolicy, String> {
    let json = compile_source_to_json(
        source,
        CompileOptions {
            source_path: Some("browser-bridge-policy.dvl".into()),
            search_paths: Vec::new(),
        },
    )
    .map_err(|error| format!("invalid bridge policy: {error}"))?;
    let package: Value =
        serde_json::from_str(&json).map_err(|error| format!("invalid bridge policy: {error}"))?;
    let mut policy =
        EffectPolicy::new(package).map_err(|error| format!("invalid bridge policy: {error}"))?;
    policy
        .set_default_authorization("deny-unless-allowed")
        .map_err(|error| format!("invalid bridge policy: {error}"))?;
    Ok(policy)
}

fn claim_next(inner: &mut BridgeInner) -> Result<Option<Value>, String> {
    let Some(id) = inner.queued.pop_front() else {
        return Ok(None);
    };
    let Some(existing) = inner.jobs.get(&id).cloned() else {
        return claim_next(inner);
    };
    let mut job = existing;
    job.status = Status::InProgress;
    job.updated_at = rfc3339_now();
    job.started = Some(Instant::now());
    let line = log_line(&job);
    if let Err(error) = append_log(&inner.log_path, &line) {
        inner.queued.push_front(id);
        return Err(error);
    }
    let view = next_view(&job);
    logutil::info(&format!(
        "bridge job {} {} {}",
        job.id,
        job.verb,
        job.status.as_str()
    ));
    inner.jobs.insert(id, job);
    Ok(Some(view))
}

fn expire(inner: &mut BridgeInner) -> Result<(), String> {
    let now = Instant::now();
    let due: Vec<String> = inner
        .jobs
        .iter()
        .filter(|(_, job)| {
            job.status == Status::InProgress
                && job
                    .started
                    .is_some_and(|started| now.duration_since(started) >= inner.timeout)
        })
        .map(|(id, _)| id.clone())
        .collect();
    for id in due {
        let Some(existing) = inner.jobs.get(&id).cloned() else {
            continue;
        };
        let mut job = existing;
        job.status = Status::Failed;
        job.error = Some(json!("in_progress timed out"));
        job.result = None;
        job.updated_at = rfc3339_now();
        job.started = None;
        let line = log_line(&job);
        append_log(&inner.log_path, &line)?;
        logutil::info(&format!(
            "bridge job {} {} {}",
            job.id,
            job.verb,
            job.status.as_str()
        ));
        inner.jobs.insert(id, job);
    }
    Ok(())
}

fn consume_rate(inner: &mut BridgeInner) -> bool {
    let now = Instant::now();
    while inner
        .accepted_at
        .front()
        .is_some_and(|stamp| now.duration_since(*stamp) >= inner.rate_window)
    {
        inner.accepted_at.pop_front();
    }
    if inner.accepted_at.len() as u64 >= inner.rate_limit {
        return false;
    }
    inner.accepted_at.push_back(now);
    true
}

fn wait_slice(inner: &BridgeInner, deadline: Instant) -> Duration {
    let now = Instant::now();
    let Some(mut slice) = deadline.checked_duration_since(now) else {
        return Duration::ZERO;
    };
    for job in inner.jobs.values() {
        if job.status != Status::InProgress {
            continue;
        }
        let Some(started) = job.started else {
            continue;
        };
        let due = started + inner.timeout;
        if due <= now {
            return Duration::ZERO;
        }
        if let Some(remaining) = due.checked_duration_since(now) {
            slice = slice.min(remaining);
        }
    }
    slice
}

fn log_rejection(
    inner: &BridgeInner,
    verb: &str,
    args: &Value,
    outcome: &str,
    message: &str,
) -> Result<(), String> {
    append_log(
        &inner.log_path,
        &json!({
            "verb": verb,
            "args_summary": summarize(args),
            "outcome": outcome,
            "error": message,
            "at": rfc3339_now(),
        }),
    )
}

fn log_line(job: &Job) -> Value {
    let mut line = json!({
        "id": job.id,
        "verb": job.verb,
        "args_summary": summarize(&job.args),
        "outcome": job.status.as_str(),
        "created_at": job.created_at,
        "updated_at": job.updated_at,
    });
    if let Some(error) = &job.error {
        line["error"] = error.clone();
    }
    line
}

fn next_view(job: &Job) -> Value {
    json!({
        "id": job.id,
        "verb": job.verb,
        "args": job.args,
    })
}

fn full_view(job: &Job) -> Value {
    let mut view = json!({
        "id": job.id,
        "verb": job.verb,
        "args": job.args,
        "status": job.status.as_str(),
        "created_at": job.created_at,
        "updated_at": job.updated_at,
    });
    if let Some(result) = &job.result {
        view["result"] = result.clone();
    }
    if let Some(error) = &job.error {
        view["error"] = error.clone();
    }
    view
}

fn summarize(args: &Value) -> Value {
    let Some(object) = args.as_object() else {
        return json!({"value": abbreviate(args)});
    };
    let mut keys: Vec<&String> = object.keys().collect();
    keys.sort();
    let mut summary = Map::new();
    for key in keys {
        summary.insert(key.clone(), summarize_value(&object[key]));
    }
    Value::Object(summary)
}

fn summarize_value(value: &Value) -> Value {
    match value {
        Value::String(text) => Value::String(truncate_chars(text, SUMMARY_CHARS)),
        Value::Array(items) => json!(items.len()),
        Value::Object(map) => json!({"keys": map.len()}),
        other => other.clone(),
    }
}

fn abbreviate(value: &Value) -> String {
    truncate_chars(&value.to_string(), SUMMARY_CHARS)
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    text.chars().take(max).collect()
}

fn append_log(path: &Path, line: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("create {}: {error}", parent.display()))?;
        }
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("open {}: {error}", path.display()))?;
    let text = serde_json::to_string(line).map_err(|error| error.to_string())?;
    writeln!(file, "{text}").map_err(|error| format!("write {}: {error}", path.display()))?;
    file.flush()
        .map_err(|error| format!("write {}: {error}", path.display()))
}

fn parse_object(body: &str) -> Result<Map<String, Value>, BridgeResponse> {
    if body.trim().is_empty() {
        return Err(error_response(400, "request body is required"));
    }
    match serde_json::from_str::<Value>(body) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(error_response(400, "request body must be a JSON object")),
        Err(_) => Err(error_response(400, "invalid JSON")),
    }
}

fn required_wait(query: &str) -> Result<u64, BridgeResponse> {
    match query_param(query, "wait") {
        Some(value) => parse_wait(value),
        None => Err(error_response(400, "wait is required")),
    }
}

fn optional_wait(query: &str) -> Result<Option<u64>, BridgeResponse> {
    match query_param(query, "wait") {
        Some(value) => parse_wait(value).map(Some),
        None => Ok(None),
    }
}

fn parse_wait(value: &str) -> Result<u64, BridgeResponse> {
    match value.parse::<u64>() {
        Ok(wait) if wait <= MAX_WAIT_SECS => Ok(wait),
        _ => Err(error_response(
            400,
            format!("wait must be an integer from 0 through {MAX_WAIT_SECS}"),
        )),
    }
}

fn query_param<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let (name, value) = pair.split_once('=')?;
        (name == key).then_some(value)
    })
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn json_response(status: u16, body: Value) -> BridgeResponse {
    BridgeResponse {
        status,
        body: Some(body),
    }
}

fn error_response(status: u16, message: impl Into<String>) -> BridgeResponse {
    BridgeResponse {
        status,
        body: Some(json!({"error": message.into()})),
    }
}

fn read_token(path: &Path) -> Result<String, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let token = text.trim_end_matches(['\r', '\n']);
    if token.is_empty() {
        return Err("token is empty".into());
    }
    Ok(token.to_string())
}

fn tokens_match(expected: &str, presented: &str) -> bool {
    let left = expected.as_bytes();
    let right = presented.as_bytes();
    let mut diff = left.len() ^ right.len();
    let len = left.len().max(right.len());
    for index in 0..len {
        let a = left.get(index).copied().unwrap_or(0);
        let b = right.get(index).copied().unwrap_or(0);
        diff |= usize::from(a ^ b);
    }
    diff == 0
}

fn env_path(key: &str) -> Option<PathBuf> {
    env::var(key)
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn pick_u64(label: &str, flag: Option<u64>, env_key: &str, default: u64) -> Result<u64, String> {
    if let Some(value) = flag {
        return Ok(value);
    }
    match env::var(env_key) {
        Ok(value) if !value.trim().is_empty() => value
            .trim()
            .parse::<u64>()
            .map_err(|_| format!("{label} in {env_key} must be an integer")),
        _ => Ok(default),
    }
}

fn rfc3339_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let tod = secs % 86_400;
    let hour = tod / 3600;
    let minute = (tod % 3600) / 60;
    let second = tod % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Howard Hinnant's civil_from_days. `days` is days since the Unix epoch.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (year, month as u32, day as u32)
}
