//! Host-effect interception with an executable Devlish policy.
use crate::{sha256_hex, HostEffects, Vm};
use serde_json::{json, Value};

/// Implementations must persist the record before returning success.
pub trait PolicyRecorder {
    fn record(&mut self, record: &Value) -> Result<(), String>;
}

/// A policy is pinned compiled bytecode, evaluated without external effects.
#[derive(Clone)]
pub struct EffectPolicy {
    package: Value,
    identity: Value,
}

impl EffectPolicy {
    pub fn new(package: Value) -> Result<Self, String> {
        Vm::new(package.clone(), json!({})).map_err(|e| e.message)?;
        let rule = package
            .pointer("/manifest/rule")
            .filter(|r| {
                r.get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.is_empty())
                    && r.get("version")
                        .and_then(Value::as_str)
                        .is_some_and(|s| !s.is_empty())
            })
            .ok_or("effect policy requires a Rule with id and version")?;
        let identity = json!({"rule": rule, "artifact_sha256": sha256_hex(
            &serde_json::to_vec_pretty(&package).expect("JSON values serialize"))});
        Ok(Self { package, identity })
    }

    /// Host assertion that the loaded bytes matched an independently supplied digest.
    /// This is an integrity pin, not a signature or build-provenance claim.
    pub fn set_file_digest(&mut self, digest: String) {
        self.identity["verified_file_sha256"] = json!(digest);
    }

    pub fn identity(&self) -> &Value {
        &self.identity
    }

    pub fn evaluate(&self, kind: &str, request: &Value) -> Result<(bool, String), String> {
        self.evaluate_with_authority(kind, request, &Value::Null)
    }

    /// Evaluate using separately supplied host authority state. This API does not
    /// authenticate that state: the host must obtain it independently of request
    /// data and bind it to the same immutable operation it subsequently performs.
    /// Ordinary PolicyHost calls supply no authority and cannot opt into it via
    /// a request field. The policy still has no external effects or signing keys.
    pub fn evaluate_with_authority(
        &self,
        kind: &str,
        request: &Value,
        authority: &Value,
    ) -> Result<(bool, String), String> {
        let mut vm = Vm::new(
            self.package.clone(),
            json!({"effect": kind, "request": request, "authority": authority}),
        )
        .map_err(|e| e.message)?;
        vm.set_instruction_limit(100_000);
        vm.set_emit_events(false);
        let result = vm.run(&mut PolicyEvaluationHost).map_err(|e| e.message)?;
        let response = result
            .get("response")
            .ok_or("policy must Respond with a decision")?;
        let allow = response
            .get("allow")
            .and_then(Value::as_bool)
            .ok_or("policy decision requires boolean allow")?;
        let reason = response
            .get("reason")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .ok_or("policy decision requires a nonempty reason")?;
        Ok((allow, reason.to_owned()))
    }
}

// Default host methods deny all external effects. Respond only returns the decision.
struct PolicyEvaluationHost;
impl HostEffects for PolicyEvaluationHost {
    fn emit_event(&mut self, _: &Value) {}
    fn write_file(&mut self, _: &Value) -> Result<(), String> {
        Err("policy evaluation cannot write files".into())
    }
    fn respond(&mut self, _: &Value) -> Result<(), String> {
        Ok(())
    }
}

fn digest(value: &Value) -> String {
    sha256_hex(&serde_json::to_vec(value).expect("JSON values serialize"))
}

/// Install outside the real host (and outside a journaling host). The policy
/// cannot access the underlying host or grant permissions denied by the VM.
pub struct PolicyHost<'a> {
    inner: &'a mut dyn HostEffects,
    policy: &'a EffectPolicy,
    recorder: &'a mut dyn PolicyRecorder,
    next_id: u64,
    recording_failed: bool,
    capture_evidence: bool,
    redact_diagnostics: bool,
    allowed_effects: Option<std::collections::BTreeSet<String>>,
    effect_budget: Option<crate::effect_budget::EffectBudget>,
    dispatch_guard: Option<Box<dyn FnMut() -> Result<(), String>>>,
    dispatch_blocked: bool,
}

impl<'a> PolicyHost<'a> {
    pub fn new(
        inner: &'a mut dyn HostEffects,
        policy: &'a EffectPolicy,
        recorder: &'a mut dyn PolicyRecorder,
    ) -> Self {
        Self {
            inner,
            policy,
            recorder,
            next_id: 0,
            recording_failed: false,
            capture_evidence: false,
            redact_diagnostics: false,
            allowed_effects: None,
            effect_budget: None,
            dispatch_guard: None,
            dispatch_blocked: false,
        }
    }

    /// Opt-in raw requests/responses for offline replay. The recorder must protect
    /// this material as sensitive data; default decision logs contain only hashes.
    pub fn with_evidence(mut self) -> Self {
        self.capture_evidence = true;
        self
    }

    /// Do not copy data-dependent policy reasons into ordinary logs or errors.
    pub fn with_redacted_diagnostics(mut self) -> Self {
        self.redact_diagnostics = true;
        self
    }

    /// Immutable release permissions intersect the Devlish policy decision.
    pub fn with_allowed_effects(mut self, effects: std::collections::BTreeSet<String>) -> Self {
        self.allowed_effects = Some(effects);
        self
    }

    /// Install a fresh trusted budget once, before execution. Attempts consume
    /// it before policy evaluation/dispatch; caught errors cannot refund counts.
    pub fn with_effect_budget(mut self, budget: crate::effect_budget::EffectBudget) -> Self {
        self.effect_budget = Some(budget);
        self
    }

    /// Trusted adapter check immediately before each authorized host dispatch.
    /// Failure permanently blocks this host; it is recorded as a failed outcome.
    pub fn with_dispatch_guard(mut self, guard: Box<dyn FnMut() -> Result<(), String>>) -> Self {
        self.dispatch_guard = Some(guard);
        self
    }

    pub fn recording_failed(&self) -> bool {
        self.recording_failed
    }

    fn persist(&mut self, record: Value) -> Result<(), String> {
        self.recorder.record(&record).map_err(|error| {
            self.recording_failed = true;
            format!("policy recording failed; execution blocked: {error}")
        })
    }

    fn invoke<T>(
        &mut self,
        kind: &str,
        request: Value,
        action: impl FnOnce(&mut dyn HostEffects) -> Result<T, String>,
        value: impl FnOnce(&T) -> Value,
    ) -> Result<T, String> {
        if self.recording_failed {
            return Err("policy recording previously failed; execution blocked".into());
        }
        self.next_id += 1;
        let id = self.next_id;
        let budget_allows = self
            .effect_budget
            .as_mut()
            .is_none_or(|budget| budget.consume_attempt(kind));
        let (allow, reason) = if !budget_allows {
            (false, "Effect attempt budget is exhausted.".to_string())
        } else if self
            .allowed_effects
            .as_ref()
            .is_some_and(|effects| !effects.contains(kind))
        {
            (
                false,
                "Effect is not permitted by the approved release.".to_string(),
            )
        } else {
            self.policy
                .evaluate(kind, &request)
                .unwrap_or_else(|error| (false, format!("policy evaluation failed: {error}")))
        };
        let mut decision = json!({"type": "effect_decision", "effect_id": id,
            "effect": kind, "request_sha256": digest(&request),
            "policy": self.policy.identity(), "allow": allow, "reason": reason});
        if self.redact_diagnostics {
            decision["reason_sha256"] = json!(digest(&json!(reason)));
            decision["reason"] = json!("Policy decision recorded; diagnostic content withheld.");
        }
        if self.capture_evidence {
            decision["request"] = request;
        }
        self.persist(decision)?;
        if !allow {
            return Err(if self.redact_diagnostics {
                format!("Policy denied {kind}; diagnostic content withheld")
            } else {
                format!("Policy denied {kind}: {reason}")
            });
        }
        let gate = if self.dispatch_blocked {
            Err("host dispatch previously blocked".into())
        } else {
            self.dispatch_guard.as_mut().map_or(Ok(()), |guard| guard())
        };
        if gate.is_err() {
            self.dispatch_blocked = true;
        }
        let result = gate.and_then(|()| action(self.inner));
        let exchange = match &result {
            Ok(result) => json!({"ok": value(result)}),
            Err(error) => json!({"err": error}),
        };
        let outcome = if let Some(value) = exchange.get("ok") {
            json!({"status":"succeeded", "result_sha256":digest(value)})
        } else {
            json!({"status":"failed", "error_sha256":digest(&exchange["err"])})
        };
        // A missing outcome means the effect MAY have happened. Never auto-retry it.
        let mut record = json!({"type": "effect_outcome", "effect_id": id,
            "effect": kind, "outcome": outcome});
        if self.capture_evidence {
            record["exchange"] = exchange;
        }
        self.persist(record)?;
        result
    }
}

impl HostEffects for PolicyHost<'_> {
    // Runtime diagnostics and audit plumbing are not model-invoked tools.
    fn emit_event(&mut self, event: &Value) {
        self.inner.emit_event(event);
    }
    fn audit_record(&mut self, record: &Value) -> Result<(), String> {
        if self.recording_failed {
            return Err("policy recording failed".into());
        }
        self.inner.audit_record(record)
    }
    // Credentials stay inside the underlying host; never expose them to policy.
    fn write_file(&mut self, request: &Value) -> Result<(), String> {
        self.invoke(
            "write_file",
            request.clone(),
            |host| host.write_file(request),
            |value| json!(value),
        )
    }
    fn read_file(&mut self, request: &Value) -> Result<Value, String> {
        self.invoke(
            "read_file",
            request.clone(),
            |host| host.read_file(request),
            |value| json!(value),
        )
    }
    fn call_service(&mut self, request: &Value) -> Result<Value, String> {
        self.invoke(
            "call_service",
            request.clone(),
            |host| host.call_service(request),
            |value| json!(value),
        )
    }
    fn http_request(
        &mut self,
        method: &str,
        url: &str,
        body: &Value,
        headers: &Value,
    ) -> Result<Value, String> {
        self.invoke(
            "http_request",
            json!({"method":method,"url":url,"body":body,"headers":headers}),
            |host| host.http_request(method, url, body, headers),
            |value| json!(value),
        )
    }
    fn respond(&mut self, value: &Value) -> Result<(), String> {
        self.invoke(
            "respond",
            json!({"value":value}),
            |host| host.respond(value),
            |value| json!(value),
        )
    }
    fn http_download(&mut self, url: &str, path: &str) -> Result<(), String> {
        self.invoke(
            "http_download",
            json!({"url":url,"path":path}),
            |host| host.http_download(url, path),
            |value| json!(value),
        )
    }
    fn read_xlsx_rows(&mut self, path: &str, sheet: Option<&str>) -> Result<Value, String> {
        self.invoke(
            "read_xlsx_rows",
            json!({"path":path,"sheet":sheet}),
            |host| host.read_xlsx_rows(path, sheet),
            |value| json!(value),
        )
    }
    fn file_copy(&mut self, source: &str, destination: &str) -> Result<(), String> {
        self.invoke(
            "file_copy",
            json!({"source":source,"destination":destination}),
            |host| host.file_copy(source, destination),
            |value| json!(value),
        )
    }
    fn file_move(&mut self, source: &str, destination: &str) -> Result<(), String> {
        self.invoke(
            "file_move",
            json!({"source":source,"destination":destination}),
            |host| host.file_move(source, destination),
            |value| json!(value),
        )
    }
    fn file_mkdir(&mut self, path: &str) -> Result<(), String> {
        self.invoke(
            "file_mkdir",
            json!({"path":path}),
            |host| host.file_mkdir(path),
            |value| json!(value),
        )
    }
    fn file_delete(&mut self, path: &str) -> Result<(), String> {
        self.invoke(
            "file_delete",
            json!({"path":path}),
            |host| host.file_delete(path),
            |value| json!(value),
        )
    }
    fn file_exists(&mut self, path: &str) -> Result<bool, String> {
        self.invoke(
            "file_exists",
            json!({"path":path}),
            |host| host.file_exists(path),
            |value| json!(value),
        )
    }
    fn file_stat(&mut self, path: &str) -> Result<Value, String> {
        self.invoke(
            "file_stat",
            json!({"path":path}),
            |host| host.file_stat(path),
            |value| json!(value),
        )
    }
    fn file_list(&mut self, path: &str) -> Result<Value, String> {
        self.invoke(
            "file_list",
            json!({"path":path}),
            |host| host.file_list(path),
            |value| json!(value),
        )
    }
    fn file_glob(&mut self, pattern: &str, directory: &str) -> Result<Value, String> {
        self.invoke(
            "file_glob",
            json!({"pattern":pattern,"directory":directory}),
            |host| host.file_glob(pattern, directory),
            |value| json!(value),
        )
    }
    fn llm_complete(&mut self, request: &Value) -> Result<Value, String> {
        self.invoke(
            "llm_complete",
            request.clone(),
            |host| host.llm_complete(request),
            |value| json!(value),
        )
    }
    fn clock_now(&mut self, kind: &str) -> Result<Value, String> {
        self.invoke(
            "clock_now",
            json!({"kind":kind}),
            |host| host.clock_now(kind),
            |value| json!(value),
        )
    }
    fn random_draw(&mut self, request: &Value) -> Result<Value, String> {
        self.invoke(
            "random_draw",
            request.clone(),
            |host| host.random_draw(request),
            |value| json!(value),
        )
    }
}
