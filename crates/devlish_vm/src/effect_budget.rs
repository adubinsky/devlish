//! Immutable host-effect attempt limits and their per-run counters.
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Trusted operator configuration. This is a call-count cap, not a monetary,
/// token, wall-clock or cross-session budget. Denied and failed attempts count.
#[derive(Clone, Debug)]
pub struct EffectBudget {
    total: u64,
    per_effect: BTreeMap<String, u64>,
}
impl EffectBudget {
    pub fn parse(value: &Value) -> Result<Self, String> {
        let object = value.as_object().ok_or("effect budget must be an object")?;
        if object.len() != 2 || !object.contains_key("total") || !object.contains_key("per_effect")
        {
            return Err("effect budget requires only total and per_effect".into());
        }
        let total = value["total"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= 10_000)
            .ok_or("effect budget total must be 1 through 10000")?;
        let limits = value["per_effect"]
            .as_object()
            .ok_or("per_effect must be an object")?;
        let mut per_effect = BTreeMap::new();
        for (kind, limit) in limits {
            let limit = limit
                .as_u64()
                .filter(|n| *n <= total)
                .ok_or("per-effect limit must be an integer from zero through total")?;
            if kind.is_empty()
                || kind.len() > 64
                || !kind.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
            {
                return Err("invalid budget effect name".into());
            }
            per_effect.insert(kind.clone(), limit);
        }
        Ok(Self { total, per_effect })
    }
    pub fn validate_effects(&self, allowed: &BTreeSet<String>) -> Result<(), String> {
        if self.per_effect.keys().any(|key| !allowed.contains(key)) {
            return Err("budget refers to an effect outside approved permissions".into());
        }
        Ok(())
    }
    pub fn to_value(&self) -> Value {
        json!({"total":self.total,"per_effect":self.per_effect})
    }
    pub(crate) fn consume_attempt(&mut self, kind: &str) -> bool {
        let total_available = self.total > 0;
        self.total = self.total.saturating_sub(1);
        let kind_available = self
            .per_effect
            .get_mut(kind)
            .map(|remaining| {
                let available = *remaining > 0;
                *remaining = remaining.saturating_sub(1);
                available
            })
            .unwrap_or(true);
        total_available && kind_available
    }
}
