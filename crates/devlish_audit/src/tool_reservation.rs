//! Offline verification of anchored launch reservation bytes and catalog binding.
//! Local consumption records do not authenticate their writer or prove execution.
use crate::{sha256, tool_catalog::ToolSelection};
use serde::Deserialize;
use serde_json::{json, Value};

pub const MAX_RESERVATION_BYTES: u64 = 8192;
pub const MAX_RESERVED_RECORD_BYTES: usize = 4096;

/// Domain-separated identity shared with the native store. Selection changes
/// cannot change the identity of an existing logical effect.
pub fn operation_id(tenant: &str, session: &str, effect: u64) -> Result<String, String> {
    for identity in [tenant, session] {
        if identity.is_empty()
            || identity.len() > 128
            || !identity
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        {
            return Err("invalid protected launch identity".into());
        }
    }
    if effect == 0 {
        return Err("launch effect number must be positive".into());
    }
    Ok(sha256(
        &serde_json::to_vec(&json!({
            "domain":"devlish-tool-launch-slot-v1", "tenant_id":tenant,
            "session_id":session,"effect_id":effect,
        }))
        .map_err(|_| "invalid launch slot")?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    release_sha256: String,
    catalog_sha256: String,
    tool_id: String,
    tool_sha256: String,
    arguments_sha256: String,
    containment_sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reserved {
    format: String,
    format_version: u32,
    state: String,
    operation_id: String,
    tenant_id: String,
    session_id: String,
    effect_id: u64,
    binding: Binding,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Consumed {
    state: String,
    operation_id: String,
    reserved_sha256: String,
}

impl ToolSelection<'_> {
    /// The digest must be supplied independently for the intended attempt.
    /// This validates exact bytes and metadata, never authorization or execution.
    pub fn verify_launch_reservation(
        &self,
        bytes: &[u8],
        retained_sha256: &str,
    ) -> Result<Value, String> {
        if bytes.len() as u64 > MAX_RESERVATION_BYTES {
            return Err("launch reservation exceeds size limit".into());
        }
        if sha256(bytes) != retained_sha256 {
            return Err("launch reservation does not match retained digest".into());
        }
        if !bytes.ends_with(b"\n") {
            return Err("launch reservation is incomplete".into());
        }
        let lines: Vec<_> = bytes[..bytes.len() - 1].split(|b| *b == b'\n').collect();
        if !(1..=2).contains(&lines.len())
            || lines.iter().any(|line| line.is_empty())
            || lines[0].len() + 1 > MAX_RESERVED_RECORD_BYTES
        {
            return Err("invalid launch reservation record count or size".into());
        }
        let reserved: Reserved =
            serde_json::from_slice(lines[0]).map_err(|_| "invalid launch reservation record")?;
        if reserved.format != "devlish-tool-launch-reservation"
            || reserved.format_version != 1
            || reserved.state != "reserved"
            || reserved.operation_id
                != operation_id(
                    &reserved.tenant_id,
                    &reserved.session_id,
                    reserved.effect_id,
                )?
        {
            return Err("invalid launch reservation identity or state".into());
        }
        let binding = &reserved.binding;
        if binding.release_sha256 != self.manifest_sha256()
            || binding.catalog_sha256 != self.catalog_sha256()
            || binding.tool_id != self.id()
            || binding.tool_sha256 != self.tool_sha256()
            || binding.arguments_sha256
                != sha256(
                    &serde_json::to_vec(self.arguments())
                        .map_err(|_| "invalid argument commitment")?,
                )
            || binding.containment_sha256 != self.containment_sha256()
        {
            return Err("launch reservation does not match authenticated tool selection".into());
        }
        if lines.len() == 2 {
            let consumed: Consumed = serde_json::from_slice(lines[1])
                .map_err(|_| "invalid launch consumption record")?;
            if consumed.state != "consumed"
                || consumed.operation_id != reserved.operation_id
                || consumed.reserved_sha256 != sha256(&bytes[..lines[0].len() + 1])
            {
                return Err("launch consumption does not match reserved bytes".into());
            }
        }
        Ok(json!({
            "format":"devlish-tool-launch-reservation-verification", "format_version":1,
            "reservation_sha256":retained_sha256, "operation_id":reserved.operation_id,
            "reservation_bytes_match_anchor":true,"reservation_binding_verified":true,
            "reservation_recorded_consumed":lines.len()==2,
            "reservation_writer_authenticated":false,"execution_origin_verified":false,
            "policy_enforcement_verified":false,
            "explanation":"Exact bytes match the independently supplied digest and authenticated catalog selection. A reserved or consumed record does not establish whether execution happened, completed or complied with policy. The local writer is not authenticated. A retained earlier reservation cannot prove that no later action occurred. Raw arguments, paths and tenant/session identifiers are omitted."
        }))
    }
}
