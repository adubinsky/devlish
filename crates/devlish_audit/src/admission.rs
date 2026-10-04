//! Local durable admission floor. Operator custody is required; not root-proof.
use crate::{
    release::{ReleaseVerification, Requirements},
    sha256, MAX_METADATA_BYTES,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct State {
    format: String,
    format_version: u32,
    scope_sha256: String,
    minimum_sequence: u64,
    accepted_manifest_sha256: Option<String>,
}
fn scope(requirements: &Requirements) -> String {
    sha256(&serde_json::to_vec(&json!({"environment":requirements.environment,
        "target":requirements.target,"repository":requirements.repository,"policy_id":requirements.policy_id})).expect("JSON serializes"))
}
/// Explicit provisioning only. Never initialize missing state during execution.
pub fn initialize(path: &Path, requirements: &[u8]) -> Result<(), String> {
    if requirements.len() as u64 > MAX_METADATA_BYTES {
        return Err("requirements exceed size limit".into());
    }
    let requirements: Requirements =
        serde_json::from_slice(requirements).map_err(|e| e.to_string())?;
    if requirements.format != "devlish-release-requirements"
        || requirements.format_version != 1
        || [
            &requirements.environment,
            &requirements.target,
            &requirements.repository,
            &requirements.policy_id,
        ]
        .iter()
        .any(|s| s.trim().is_empty())
    {
        return Err("unsupported or empty requirements scope".into());
    }
    let state = State {
        format: "devlish-admission-state".into(),
        format_version: 1,
        scope_sha256: scope(&requirements),
        minimum_sequence: requirements.minimum_sequence,
        accepted_manifest_sha256: None,
    };
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|e| format!("cannot initialize new admission state: {e}"))?;
    file.write_all(&serde_json::to_vec(&state).map_err(|e| e.to_string())?)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    File::open(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )
    .and_then(|dir| dir.sync_all())
    .map_err(|e| e.to_string())?;
    Ok(())
}
/// Exclusive OS lock remains held until this guard is dropped. Crash during
/// in-place update may corrupt the state, which fails closed on the next run.
pub struct AdmissionGuard {
    _file: File,
}
impl ReleaseVerification {
    pub fn admit(&self, path: &Path) -> Result<AdmissionGuard, String> {
        #[cfg(not(unix))]
        {
            let _ = path;
            Err("durable admission locking is not supported on this platform".into())
        }
        #[cfg(unix)]
        {
            use std::os::unix::{
                fs::{MetadataExt, OpenOptionsExt},
                io::AsRawFd,
            };
            let mut file = OpenOptions::new()
                .read(true)
                .write(true)
                .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
                .open(path)
                .map_err(|e| format!("cannot open existing admission state: {e}"))?;
            let meta = file.metadata().map_err(|e| e.to_string())?;
            if !meta.is_file()
                || meta.len() > MAX_METADATA_BYTES
                || meta.mode() & 0o077 != 0
                || meta.nlink() != 1
            {
                return Err("admission state must be a private regular file with one link".into());
            }
            // SAFETY: flock operates on this owned live file descriptor.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                return Err("admission state is locked by another run".into());
            }
            let mut bytes = Vec::new();
            (&mut file)
                .take(MAX_METADATA_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            if bytes.len() as u64 > MAX_METADATA_BYTES {
                return Err("admission state exceeds size limit".into());
            }
            let mut state: State = serde_json::from_slice(&bytes)
                .map_err(|e| format!("invalid admission state; operator recovery required: {e}"))?;
            if state.format != "devlish-admission-state"
                || state.format_version != 1
                || state.scope_sha256 != self.verified_scope_digest
            {
                return Err("admission state has wrong format or release scope".into());
            }
            if let Some(digest) = &state.accepted_manifest_sha256 {
                crate::decode::<32>(digest)?;
            }
            if self.verified_sequence < state.minimum_sequence
                || (self.verified_sequence == state.minimum_sequence
                    && state
                        .accepted_manifest_sha256
                        .as_ref()
                        .is_some_and(|d| d != &self.verified_manifest_digest))
            {
                return Err(
                    "release rollback or reuse of accepted sequence for different bytes".into(),
                );
            }
            state.minimum_sequence = self.verified_sequence;
            state.accepted_manifest_sha256 = Some(self.verified_manifest_digest.clone());
            let bytes = serde_json::to_vec(&state).map_err(|e| e.to_string())?;
            file.seek(SeekFrom::Start(0))
                .and_then(|_| file.set_len(0))
                .and_then(|_| file.write_all(&bytes))
                .and_then(|_| file.sync_all())
                .map_err(|e| format!("cannot persist admission state: {e}"))?;
            Ok(AdmissionGuard { _file: file })
        }
    }
}
pub(crate) fn scope_digest(requirements: &Requirements) -> String {
    scope(requirements)
}
