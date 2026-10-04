//! Authenticated external-tool selections, not execution capabilities.
use crate::{
    release::{ReleaseVerification, Role},
    sha256, MAX_METADATA_BYTES,
};
use serde::Deserialize;
use std::collections::BTreeSet;

pub const STATIC_PROFILE: &str = "devlish-linux-static-x86_64-v1";
pub const STATIC_TARGET: &str = "x86_64-unknown-linux-gnu";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalog {
    format: String,
    format_version: u32,
    target: String,
    tools: Vec<Entry>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    id: String,
    artifact_id: String,
    path: String,
    image_profile: String,
    containment_id: String,
    allowed_arguments: Vec<Vec<String>>,
}

/// Exact catalog bytes bound to a verified release. There is no deserialization
/// or public constructor accepting unauthenticated claims.
#[derive(Debug)]
pub struct VerifiedToolCatalog {
    catalog: Catalog,
    digests: Vec<(String, String)>,
    manifest_sha256: String,
    catalog_sha256: String,
    evaluated_at: u64,
    valid_until: u64,
}

/// An immutable borrowed selection. It proves catalog membership at the supplied
/// time only; policy, permissions, admission floor and OS launch checks remain.
#[derive(Debug)]
pub struct ToolSelection<'a> {
    catalog: &'a VerifiedToolCatalog,
    index: usize,
    arguments: &'a [String],
}

impl ReleaseVerification {
    pub fn tool_catalog(&self, id: &str, bytes: &[u8]) -> Result<VerifiedToolCatalog, String> {
        if bytes.len() as u64 > MAX_METADATA_BYTES {
            return Err("tool catalog exceeds metadata limit".into());
        }
        let catalog_sha256 = sha256(bytes);
        if !self
            .verified_artifacts
            .iter()
            .any(|(artifact, role, digest, _)| {
                artifact == id && *role == Role::ToolCatalog && digest == &catalog_sha256
            })
        {
            return Err("tool catalog does not match the verified release".into());
        }
        let catalog: Catalog =
            serde_json::from_slice(bytes).map_err(|_| "invalid external tool catalog")?;
        if catalog.format != "devlish-external-tool-catalog"
            || catalog.format_version != 1
            || catalog.target != self.verified_target
            || catalog.target != STATIC_TARGET
            || catalog.tools.is_empty()
            || catalog.tools.len() > 64
        {
            return Err("unsupported external tool catalog".into());
        }
        let artifact_digest = |id: &str, role: Role| {
            self.verified_artifacts
                .iter()
                .find(|(candidate, actual_role, _, _)| candidate == id && *actual_role == role)
                .map(|(_, _, digest, _)| digest.clone())
                .ok_or_else(|| {
                    "tool references an absent or wrong-role release artifact".to_string()
                })
        };
        let mut ids = BTreeSet::new();
        let mut digests = Vec::new();
        for entry in &catalog.tools {
            if !identifier(&entry.id)
                || !ids.insert(&entry.id)
                || !absolute_path(&entry.path)
                || entry.image_profile != STATIC_PROFILE
                || entry.allowed_arguments.is_empty()
                || entry.allowed_arguments.len() > 32
            {
                return Err("invalid or duplicate catalog tool".into());
            }
            let mut alternatives = BTreeSet::new();
            for arguments in &entry.allowed_arguments {
                if !arguments_valid(arguments) || !alternatives.insert(arguments) {
                    return Err("invalid or duplicate catalog argument alternative".into());
                }
            }
            digests.push((
                artifact_digest(&entry.artifact_id, Role::Tool)?,
                artifact_digest(&entry.containment_id, Role::Containment)?,
            ));
        }
        Ok(VerifiedToolCatalog {
            catalog,
            digests,
            manifest_sha256: self.verified_manifest_digest.clone(),
            catalog_sha256,
            evaluated_at: self.verified_evaluated_at,
            valid_until: self.admission_valid_until(),
        })
    }
}

impl VerifiedToolCatalog {
    /// `now` must come from a protected caller clock, not a model request.
    /// Recheck expiry at dispatch; a selection is not a perpetual approval.
    pub fn select(
        &self,
        id: &str,
        arguments: &[String],
        now: u64,
    ) -> Result<ToolSelection<'_>, String> {
        if now < self.evaluated_at || now >= self.valid_until {
            return Err("tool catalog is outside its verified admission window".into());
        }
        let (index, entry) = self
            .catalog
            .tools
            .iter()
            .enumerate()
            .find(|(_, entry)| entry.id == id)
            .ok_or("tool is not in the verified catalog")?;
        let arguments = entry
            .allowed_arguments
            .iter()
            .find(|allowed| allowed.as_slice() == arguments)
            .ok_or("arguments are outside the verified catalog")?;
        Ok(ToolSelection {
            catalog: self,
            index,
            arguments,
        })
    }
}

impl ToolSelection<'_> {
    fn entry(&self) -> &Entry {
        &self.catalog.catalog.tools[self.index]
    }
    pub fn id(&self) -> &str {
        &self.entry().id
    }
    pub fn path(&self) -> &str {
        &self.entry().path
    }
    pub fn artifact_id(&self) -> &str {
        &self.entry().artifact_id
    }
    pub fn image_profile(&self) -> &str {
        &self.entry().image_profile
    }
    pub fn containment_id(&self) -> &str {
        &self.entry().containment_id
    }
    pub fn arguments(&self) -> &[String] {
        self.arguments
    }
    pub fn tool_sha256(&self) -> &str {
        &self.catalog.digests[self.index].0
    }
    pub fn containment_sha256(&self) -> &str {
        &self.catalog.digests[self.index].1
    }
    pub fn catalog_sha256(&self) -> &str {
        &self.catalog.catalog_sha256
    }
    pub fn manifest_sha256(&self) -> &str {
        &self.catalog.manifest_sha256
    }
    pub fn valid_until(&self) -> u64 {
        self.catalog.valid_until
    }
}

fn identifier(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}
fn absolute_path(path: &str) -> bool {
    path.starts_with('/')
        && path.len() <= 4096
        && !path.contains('\0')
        && path[1..]
            .split('/')
            .all(|component| !component.is_empty() && component != "." && component != "..")
}
fn arguments_valid(arguments: &[String]) -> bool {
    arguments.len() <= 64
        && arguments
            .iter()
            .all(|argument| argument.len() <= 4096 && !argument.contains('\0'))
        && arguments.iter().map(String::len).sum::<usize>() <= 16384
}
