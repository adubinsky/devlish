//! Operator-selected verified CLI admission. Does not protect against host admin.
use devlish_audit::{
    read_bounded,
    release::{verify_release, Manifest, Role},
    sha256, MAX_ARTIFACT_BYTES, MAX_METADATA_BYTES,
};
use devlish_vm::policy::EffectPolicy;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) struct VerifiedInputs {
    pub program: Value,
    pub policy: EffectPolicy,
    pub log_context: Value,
    pub instruction_limit: u64,
    pub allowed_effects: std::collections::BTreeSet<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    format: String,
    format_version: u32,
    manifest: PathBuf,
    signature: PathBuf,
    trust: PathBuf,
    requirements: PathBuf,
    admission_state: PathBuf,
    artifacts: Vec<LocalArtifact>,
    runtime_id: String,
    program_id: String,
    policy_id: String,
    permissions_id: String,
    catalog_id: String,
    containment_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalArtifact {
    id: String,
    path: PathBuf,
}

pub(super) fn run(args: Vec<String>) -> Result<(), String> {
    devlish_core::logutil::set_level(devlish_core::logutil::LogLevel::Error);
    const USAGE: &str = "Usage: DEVLISH_VERIFIED_PROFILE=<operator-profile.json> devlish-core run-verified --policy-log <new-path> --session-id <id> [--input <json>] [--policy-evidence]";
    let mut options = BTreeMap::new();
    let mut evidence = false;
    let mut index = 1;
    while index < args.len() {
        if args[index] == "--policy-evidence" && !evidence {
            evidence = true;
            index += 1;
            continue;
        }
        if !["--policy-log", "--session-id", "--input"].contains(&args[index].as_str())
            || index + 1 >= args.len()
            || options
                .insert(args[index].as_str(), args[index + 1].as_str())
                .is_some()
        {
            return Err(USAGE.into());
        }
        index += 2;
    }
    let log = options.get("--policy-log").ok_or(USAGE)?;
    let session = options.get("--session-id").ok_or(USAGE)?;
    if session.is_empty()
        || session.len() > 128
        || !session
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
    {
        return Err(
            "session ID must be 1-128 ASCII letters, digits, underscores or hyphens".into(),
        );
    }
    let input = options.get("--input").copied().unwrap_or("{}");
    serde_json::from_str::<Value>(input).map_err(|e| format!("invalid input JSON: {e}"))?;
    let path = std::env::var_os("DEVLISH_VERIFIED_PROFILE")
        .map(PathBuf::from)
        .ok_or("run-verified requires an operator-configured DEVLISH_VERIFIED_PROFILE")?;
    let profile_bytes = read_bounded(&path, MAX_METADATA_BYTES)?;
    let profile: Profile = serde_json::from_slice(&profile_bytes)
        .map_err(|e| format!("invalid verified profile: {e}"))?;
    if profile.format != "devlish-verified-profile" || profile.format_version != 1 {
        return Err("unsupported verified profile".into());
    }
    let base = path.parent().unwrap_or(Path::new("."));
    let read = |p: &Path, limit| read_bounded(&base.join(p), limit);
    let manifest_bytes = read(&profile.manifest, MAX_METADATA_BYTES)?;
    let requirements_bytes = read(&profile.requirements, MAX_METADATA_BYTES)?;
    // Validate the original typed document before modifying evaluation time.
    serde_json::from_slice::<devlish_audit::release::Requirements>(&requirements_bytes)
        .map_err(|e| format!("invalid requirements: {e}"))?;
    let mut requirements: Value =
        serde_json::from_slice(&requirements_bytes).map_err(|e| e.to_string())?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    requirements["evaluated_at"] = json!(now);
    let effective_requirements = serde_json::to_vec(&requirements).map_err(|e| e.to_string())?;
    let mut paths = BTreeMap::new();
    for artifact in &profile.artifacts {
        if paths
            .insert(artifact.id.clone(), base.join(&artifact.path))
            .is_some()
        {
            return Err("duplicate operator artifact mapping".into());
        }
    }
    // The approved runtime must be this executable, not a caller-chosen file.
    paths.insert(
        profile.runtime_id.clone(),
        std::env::current_exe().map_err(|e| e.to_string())?,
    );
    let mut retained = BTreeMap::new();
    let release = verify_release(
        &manifest_bytes,
        &read(&profile.signature, MAX_METADATA_BYTES)?,
        &read(&profile.trust, MAX_METADATA_BYTES)?,
        &effective_requirements,
        |id| {
            let bytes = read_bounded(
                paths
                    .get(id)
                    .ok_or_else(|| format!("missing artifact mapping: {id}"))?,
                MAX_ARTIFACT_BYTES,
            )?;
            if [
                &profile.program_id,
                &profile.policy_id,
                &profile.permissions_id,
                &profile.catalog_id,
                &profile.containment_id,
            ]
            .iter()
            .any(|selected| selected.as_str() == id)
            {
                retained.insert(id.to_string(), bytes.clone());
            }
            Ok(bytes)
        },
    )?;
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes).map_err(|e| e.to_string())?;
    for (id, role) in [
        (&profile.runtime_id, Role::Runtime),
        (&profile.program_id, Role::Program),
        (&profile.policy_id, Role::Policy),
        (&profile.permissions_id, Role::Permissions),
        (&profile.catalog_id, Role::ToolCatalog),
        (&profile.containment_id, Role::Containment),
    ] {
        if !manifest
            .artifacts
            .iter()
            .any(|a| &a.id == id && a.role == role)
        {
            return Err(format!(
                "selected artifact has incorrect release role: {id}"
            ));
        }
    }
    let permissions_bytes = retained
        .remove(&profile.permissions_id)
        .ok_or("missing permissions snapshot")?;
    let catalog_bytes = retained
        .remove(&profile.catalog_id)
        .ok_or("missing catalog snapshot")?;
    let containment_bytes = retained
        .remove(&profile.containment_id)
        .ok_or("missing containment snapshot")?;
    let controls = Controls::parse(&permissions_bytes, &catalog_bytes, &containment_bytes)?;
    let policy_bytes = retained
        .remove(&profile.policy_id)
        .ok_or("missing verified policy snapshot")?;
    let mut policy = EffectPolicy::new(
        serde_json::from_slice(&policy_bytes)
            .map_err(|e| format!("invalid verified policy: {e}"))?,
    )?;
    if policy.identity()["rule"]["id"] != manifest.policy_id
        || policy.identity()["rule"]["version"] != manifest.policy_version
    {
        return Err("compiled policy identity differs from approved release".into());
    }
    policy.set_file_digest(sha256(&policy_bytes));
    let program = serde_json::from_slice(
        &retained
            .remove(&profile.program_id)
            .ok_or("missing verified program snapshot")?,
    )
    .map_err(|e| format!("invalid verified program: {e}"))?;
    let runtime = manifest
        .artifacts
        .iter()
        .find(|a| a.id == profile.runtime_id)
        .ok_or("missing runtime")?;
    let loaded = VerifiedInputs {
        instruction_limit: controls.instruction_limit,
        allowed_effects: controls.allowed_effects.clone(),
        program,
        policy,
        log_context: json!({
            "release_manifest_sha256":sha256(&manifest_bytes), "requirements_sha256":release.requirements_sha256,
            "operator_requirements_sha256":sha256(&requirements_bytes), "operator_profile_sha256":sha256(&profile_bytes),
            "evaluated_at":now, "sequence":manifest.sequence,"session_id":session,
            "runtime_file_sha256":runtime.sha256,
        "permissions_sha256":sha256(&permissions_bytes),"catalog_sha256":sha256(&catalog_bytes),"containment_sha256":sha256(&containment_bytes),
        "instruction_limit":controls.instruction_limit,"allowed_effects":controls.allowed_effects,
            "assurance":"in-process-verified-loading", "execution_origin_verified":false
        }),
    };
    let _admission = release.admit(&base.join(&profile.admission_state))?;
    let program_path = paths
        .get(&profile.program_id)
        .ok_or("missing program mapping")?;
    let policy_path = paths
        .get(&profile.policy_id)
        .ok_or("missing policy mapping")?;
    let mut forwarded = vec![
        "run".to_string(),
        program_path.to_string_lossy().into_owned(),
        "--policy".into(),
        policy_path.to_string_lossy().into_owned(),
        "--policy-log".into(),
        (*log).into(),
        "--input".into(),
        input.into(),
        "--quiet".into(),
    ];
    if evidence {
        forwarded.push("--policy-evidence".into());
    }
    // Neither verified package is reopened or recompiled by the runner.
    super::run_execute_loaded(forwarded, Some(loaded))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Permissions {
    format: String,
    format_version: u32,
    allowed_effects: Vec<String>,
    instruction_limit: u64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    format: String,
    format_version: u32,
    host_effects: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Containment {
    format: String,
    format_version: u32,
    mode: String,
}
struct Controls {
    allowed_effects: std::collections::BTreeSet<String>,
    instruction_limit: u64,
}
impl Controls {
    fn parse(permissions: &[u8], catalog: &[u8], containment: &[u8]) -> Result<Self, String> {
        let p: Permissions =
            serde_json::from_slice(permissions).map_err(|e| format!("invalid permissions: {e}"))?;
        let c: Catalog =
            serde_json::from_slice(catalog).map_err(|e| format!("invalid tool catalog: {e}"))?;
        let isolation: Containment =
            serde_json::from_slice(containment).map_err(|e| format!("invalid containment: {e}"))?;
        if p.format != "devlish-runtime-permissions"
            || p.format_version != 1
            || p.instruction_limit == 0
            || p.instruction_limit > 10_000_000
            || c.format != "devlish-tool-catalog"
            || c.format_version != 1
            || isolation.format != "devlish-containment-profile"
            || isolation.format_version != 1
            || isolation.mode != "in-process"
        {
            return Err("unsupported release enforcement controls".into());
        }
        let supported = [
            "write_file",
            "read_file",
            "call_service",
            "http_request",
            "respond",
            "http_download",
            "read_xlsx_rows",
            "file_copy",
            "file_move",
            "file_mkdir",
            "file_delete",
            "file_exists",
            "file_stat",
            "file_list",
            "file_glob",
            "llm_complete",
            "clock_now",
            "random_draw",
        ];
        let allowed_effects: std::collections::BTreeSet<_> =
            p.allowed_effects.iter().cloned().collect();
        let catalog: std::collections::BTreeSet<_> = c.host_effects.iter().cloned().collect();
        if allowed_effects.len() != p.allowed_effects.len()
            || catalog.len() != c.host_effects.len()
            || !allowed_effects.is_subset(&catalog)
            || catalog
                .iter()
                .any(|effect| !supported.contains(&effect.as_str()))
        {
            return Err("unknown, duplicate or uncatalogued release effect".into());
        }
        Ok(Self {
            allowed_effects,
            instruction_limit: p.instruction_limit,
        })
    }
}
