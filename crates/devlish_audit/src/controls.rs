//! Independent checks of recorded controls against signed permission snapshots.
//! This module checks assertions in evidence, never actual execution or VM policy.
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Permissions {
    format: String,
    format_version: u32,
    allowed_effects: Vec<String>,
    instruction_limit: u64,
    #[serde(default)]
    effect_budget: Option<Budget>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Budget {
    total: u64,
    per_effect: BTreeMap<String, u64>,
}
impl Permissions {
    pub(crate) fn parse(bytes: &[u8]) -> Option<Self> {
        // Control metadata is small; release hashing can still handle larger
        // opaque artifacts without retaining them or claiming control semantics.
        if bytes.len() as u64 > crate::MAX_METADATA_BYTES {
            return None;
        }
        let result: Self = serde_json::from_slice(bytes).ok()?;
        let effects: BTreeSet<_> = result.allowed_effects.iter().collect();
        if result.format != "devlish-runtime-permissions"
            || result.format_version != 1
            || result.instruction_limit == 0
            || result.instruction_limit > 10_000_000
            || effects.len() != result.allowed_effects.len()
            || result.allowed_effects.iter().any(|kind| kind.is_empty())
        {
            return None;
        }
        if let Some(budget) = &result.effect_budget {
            if budget.total == 0
                || budget.total > 10_000
                || budget
                    .per_effect
                    .iter()
                    .any(|(kind, limit)| !effects.contains(kind) || *limit > budget.total)
            {
                return None;
            }
        }
        Some(result)
    }
    pub(crate) fn check(&self, binding: &Value, log: &[u8]) -> Result<(), String> {
        let recorded: Vec<String> = serde_json::from_value(binding["allowed_effects"].clone())
            .map_err(|_| "invalid recorded effect permissions")?;
        let effects: BTreeSet<_> = self.allowed_effects.iter().collect();
        let recorded_set: BTreeSet<_> = recorded.iter().collect();
        if recorded_set.len() != recorded.len()
            || recorded_set != effects
            || binding["instruction_limit"] != json!(self.instruction_limit)
        {
            return Err("recorded limits differ from signed permissions".into());
        }
        let expected_budget = self
            .effect_budget
            .as_ref()
            .map(|b| json!({"total":b.total,"per_effect":b.per_effect}))
            .unwrap_or(Value::Null);
        if binding["effect_budget"] != expected_budget {
            return Err("recorded effect budget differs from signed permissions".into());
        }
        let mut total = 0u64;
        let mut per_effect: BTreeMap<String, u64> = self
            .effect_budget
            .as_ref()
            .map(|budget| {
                budget
                    .per_effect
                    .keys()
                    .map(|kind| (kind.clone(), 0))
                    .collect()
            })
            .unwrap_or_default();
        for line in log.split(|b| *b == b'\n').filter(|line| !line.is_empty()) {
            let envelope: Value =
                serde_json::from_slice(line).map_err(|_| "invalid control log record")?;
            let record = &envelope["record"];
            if record["type"] != "effect_decision" {
                continue;
            }
            let kind = record["effect"].as_str().ok_or("missing recorded effect")?;
            // Count all attempts, including denied and failed effects. Log size
            // bounds make overflow unreachable, but checked arithmetic fails closed.
            total = total.checked_add(1).ok_or("attempt count overflow")?;
            if let Some(count) = per_effect.get_mut(kind) {
                *count = count.checked_add(1).ok_or("effect count overflow")?;
            }
            if record["allow"] == true {
                if !effects.iter().any(|effect| effect.as_str() == kind) {
                    return Err("log permits an effect outside signed permissions".into());
                }
                if let Some(budget) = &self.effect_budget {
                    if total > budget.total
                        || budget
                            .per_effect
                            .get(kind)
                            .is_some_and(|limit| per_effect[kind] > *limit)
                    {
                        return Err("log permits an effect beyond signed attempt budget".into());
                    }
                }
            }
        }
        Ok(())
    }
}
