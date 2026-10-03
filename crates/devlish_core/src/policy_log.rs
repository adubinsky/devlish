//! Exclusive per-run policy logs. A successful append is synced before dispatch.
use devlish_vm::{policy::PolicyRecorder, sha256_hex};
use serde_json::{json, Value};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;

pub struct PolicyLog {
    file: File,
    sequence: u64,
    previous_hash: String,
}

impl PolicyLog {
    pub fn create(
        path: &Path,
        policy: &Value,
        program: &Value,
        input: &Value,
    ) -> Result<Self, String> {
        Self::create_for_run(path, policy, program, input, true, false)
    }

    pub fn create_for_run(
        path: &Path,
        policy: &Value,
        program: &Value,
        input: &Value,
        emit_events: bool,
        capture_evidence: bool,
    ) -> Result<Self, String> {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let runtime = crate::integrity::read_regular_file(&executable)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(path)
            .map_err(|e| format!("cannot create policy log {}: {e}", path.display()))?;
        let mut log = Self {
            file,
            sequence: 0,
            previous_hash: String::new(),
        };
        log.record(&json!({"type":"policy_run_started", "format_version":3,
            "runtime_file_sha256":sha256_hex(&runtime),
            "emit_events":emit_events, "capture_evidence":capture_evidence,
            "policy":policy,
            "program_sha256": sha256_hex(&serde_json::to_vec_pretty(program).map_err(|e| e.to_string())?),
            "input_sha256": sha256_hex(&serde_json::to_vec(input).map_err(|e| e.to_string())?)}))?;
        // Persist the newly created directory entry as well as the file contents.
        #[cfg(unix)]
        File::open(
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )
        .and_then(|dir| dir.sync_all())
        .map_err(|e| format!("cannot sync policy log directory: {e}"))?;
        Ok(log)
    }
}

impl PolicyRecorder for PolicyLog {
    fn record(&mut self, record: &Value) -> Result<(), String> {
        let mut envelope = json!({"sequence":self.sequence, "previous_sha256":self.previous_hash,
            "record":record});
        let hash = sha256_hex(&serde_json::to_vec(&envelope).map_err(|e| e.to_string())?);
        envelope["record_sha256"] = json!(hash);
        let mut bytes = serde_json::to_vec(&envelope).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        self.file
            .write_all(&bytes)
            .and_then(|()| self.file.sync_all())
            .map_err(|e| format!("cannot persist policy record: {e}"))?;
        self.previous_hash = hash;
        self.sequence += 1;
        Ok(())
    }
}
