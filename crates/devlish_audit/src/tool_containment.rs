//! Authenticated containment requirements, never evidence of OS enforcement.
use crate::{
    sha256,
    tool_catalog::{ToolSelection, STATIC_TARGET},
    MAX_METADATA_BYTES,
};
use serde::Deserialize;

pub const PROFILE: &str = "devlish-linux-static-stdio-v1";
pub const CPU_SECONDS: u64 = 1;
pub const ADDRESS_SPACE_BYTES: u64 = 512 * 1024 * 1024;
pub const STACK_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_DESCRIPTORS: u64 = 64;
pub const STREAM_BYTES: u64 = 64 * 1024;
pub const WALL_TIMEOUT_MS: u64 = 5000;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Declaration {
    format: String,
    format_version: u32,
    profile: String,
    target: String,
    filesystem: String,
    network: String,
    process_creation: String,
    subsequent_exec: String,
    environment: String,
    resources: Resources,
    io: Io,
}
#[derive(Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Resources {
    cpu_seconds: u64,
    address_space_bytes: u64,
    stack_bytes: u64,
    max_descriptors: u64,
    core_file_bytes: u64,
    file_growth_bytes: u64,
    locked_memory_bytes: u64,
}
#[derive(Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Io {
    stdin_max_bytes: u64,
    stdout_max_bytes: u64,
    stderr_max_bytes: u64,
    wall_timeout_ms: u64,
}

/// The exact selected signed artifact declares the one recognized fixed profile.
/// This is not a launch capability or a claim that its controls were applied.
#[derive(Debug)]
pub struct VerifiedToolContainment {
    sha256: String,
}
impl VerifiedToolContainment {
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    pub fn profile(&self) -> &'static str {
        PROFILE
    }
}

impl ToolSelection<'_> {
    pub fn verify_containment(&self, bytes: &[u8]) -> Result<VerifiedToolContainment, String> {
        if bytes.len() as u64 > MAX_METADATA_BYTES {
            return Err("tool containment exceeds metadata limit".into());
        }
        let digest = sha256(bytes);
        if digest != self.containment_sha256() {
            return Err("tool containment does not match the selected signed artifact".into());
        }
        let declaration: Declaration =
            serde_json::from_slice(bytes).map_err(|_| "invalid tool containment declaration")?;
        if declaration.format != "devlish-external-tool-containment"
            || declaration.format_version != 1
            || declaration.profile != PROFILE
            || declaration.target != STATIC_TARGET
            || declaration.filesystem != "deny-new-access"
            || declaration.network != "deny"
            || declaration.process_creation != "deny"
            || declaration.subsequent_exec != "deny"
            || declaration.environment != "empty"
            || declaration.resources
                != (Resources {
                    cpu_seconds: CPU_SECONDS,
                    address_space_bytes: ADDRESS_SPACE_BYTES,
                    stack_bytes: STACK_BYTES,
                    max_descriptors: MAX_DESCRIPTORS,
                    core_file_bytes: 0,
                    file_growth_bytes: 0,
                    locked_memory_bytes: 0,
                })
            || declaration.io
                != (Io {
                    stdin_max_bytes: STREAM_BYTES,
                    stdout_max_bytes: STREAM_BYTES,
                    stderr_max_bytes: STREAM_BYTES,
                    wall_timeout_ms: WALL_TIMEOUT_MS,
                })
        {
            return Err("unsupported tool containment requirements".into());
        }
        Ok(VerifiedToolContainment { sha256: digest })
    }
}
