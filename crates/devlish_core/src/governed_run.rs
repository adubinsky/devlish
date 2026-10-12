//! Shared effect-governed execution for trusted CLI/service adapters.
//!
//! This is not release admission: callers must independently authorize the exact
//! program, policy and controls, create the bound start record, and protect host
//! adapters and recorder. HTTP/request data must never select these controls.
use devlish_vm::{
    policy::{EffectPolicy, PolicyHost, PolicyRecorder},
    sha256_hex, HostEffects, Vm,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;

/// Public completion metadata. Private VM context, results, diagnostic text and
/// checkpoint state are deliberately not available through this result.
/// Approved response values go only to the caller's trusted `HostEffects::respond`.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct Completion {
    pub response_emitted: bool,
    pub paused: bool,
}

/// Stable failures contain no input, policy reason, host error or recorder path.
#[derive(Debug, PartialEq, Eq)]
pub enum RunError {
    InvalidControls,
    Initialization,
    Execution,
    Recording,
}
impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidControls => "invalid governed execution controls",
            Self::Initialization => "governed VM initialization failed",
            Self::Execution => "governed program execution failed; details withheld",
            Self::Recording => "policy recording failed; run cannot report success",
        })
    }
}
impl std::error::Error for RunError {}

/// Single-use execution of operator-selected snapshots. Consuming the runner
/// prevents accidental reuse of its VM or effect counter; it does not provide
/// cross-run idempotency or automatic recovery of effects with missing outcomes.
pub struct GovernedRun {
    vm: Vm,
    policy: EffectPolicy,
    allowed_effects: BTreeSet<String>,
    capture_evidence: bool,
    effect_budget: Option<devlish_vm::effect_budget::EffectBudget>,
    dispatch_guard: Option<Box<dyn FnMut() -> Result<(), String>>>,
}
impl GovernedRun {
    pub fn new(
        program: Value,
        input: Value,
        policy: EffectPolicy,
        instruction_limit: u64,
        allowed_effects: BTreeSet<String>,
    ) -> Result<Self, RunError> {
        if instruction_limit == 0 || instruction_limit > 10_000_000 {
            return Err(RunError::InvalidControls);
        }
        let mut vm = Vm::new(program, input).map_err(|_| RunError::Initialization)?;
        vm.set_instruction_limit(instruction_limit);
        vm.set_emit_events(false);
        Ok(Self {
            vm,
            policy,
            allowed_effects,
            capture_evidence: false,
            effect_budget: None,
            dispatch_guard: None,
        })
    }

    /// Install an operator-owned native dispatch precondition. It cannot grant
    /// an effect denied by Devlish policy or release permissions.
    #[cfg(feature = "native")]
    pub(crate) fn with_dispatch_guard(
        mut self,
        guard: Box<dyn FnMut() -> Result<(), String>>,
    ) -> Self {
        self.dispatch_guard = Some(guard);
        self
    }

    pub fn with_effect_budget(
        mut self,
        budget: devlish_vm::effect_budget::EffectBudget,
    ) -> Result<Self, RunError> {
        budget
            .validate_effects(&self.allowed_effects)
            .map_err(|_| RunError::InvalidControls)?;
        self.effect_budget = Some(budget);
        Ok(self)
    }

    /// Trusted operator opt-in only. The caller must also declare capture in the
    /// start record and store raw requests/responses as sensitive evidence.
    pub fn with_replay_evidence(mut self) -> Self {
        self.capture_evidence = true;
        self
    }

    pub fn run(
        mut self,
        host: &mut dyn HostEffects,
        recorder: &mut dyn PolicyRecorder,
    ) -> Result<Completion, RunError> {
        let guarded = PolicyHost::new(host, &self.policy, recorder)
            .with_redacted_diagnostics()
            .with_allowed_effects(self.allowed_effects);
        let guarded = if let Some(guard) = self.dispatch_guard {
            guarded.with_dispatch_guard(guard)
        } else {
            guarded
        };
        let guarded = if let Some(budget) = self.effect_budget {
            guarded.with_effect_budget(budget)
        } else {
            guarded
        };
        let mut guarded = if self.capture_evidence {
            guarded.with_evidence()
        } else {
            guarded
        };
        let result = self.vm.run(&mut guarded);
        if guarded.recording_failed() {
            return Err(RunError::Recording);
        }
        drop(guarded);
        let (result_value, completion) = match result {
            Ok(value) => {
                let completion = Completion {
                    response_emitted: value["responded"].as_bool().unwrap_or(false),
                    paused: value["is_checkpoint"].as_bool().unwrap_or(false),
                };
                (json!({"ok":value}), Ok(completion))
            }
            Err(error) => (json!({"err":error.message}), Err(RunError::Execution)),
        };
        // Record the same private-envelope digest as process replay, but never
        // return the envelope or its data-dependent diagnostics to the caller.
        recorder.record(&json!({
            "type":"policy_run_finished", "success":completion.is_ok(),
            "paused":completion.as_ref().is_ok_and(|c| c.paused),
            "result_sha256":sha256_hex(&serde_json::to_vec(&result_value).expect("JSON serializes")),
        })).map_err(|_| RunError::Recording)?;
        completion
    }
}
