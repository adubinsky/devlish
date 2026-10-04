//! Durable local receipt reservation. Requires an operator-owned directory.
//! This is storage and signature validation, not signing authority or isolation.
use crate::{sha256, verify, Purpose};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Pending {
    format: String,
    format_version: u32,
    tenant_id: String,
    session_id: String,
    key_id: String,
    key_public_sha256: String,
    receipt_sha256: String,
    receipt: String,
}

/// Operator-selected receipt key identity. Pin the public key as well as its
/// name so rotation or rebinding of a name cannot change an outstanding slot.
pub struct ReceiptKey {
    pub id: String,
    pub public_key_sha256: String,
}

impl ReceiptKey {
    /// Select a current receipt-purpose key from operator-supplied trust. Trust
    /// authenticity, tenant scope and freshness remain the host's responsibility.
    pub fn from_trust(id: &str, trust: &[u8]) -> Result<Self, String> {
        identity(id)?;
        let roots = crate::parse_trust(trust)?;
        let key = roots
            .keys
            .iter()
            .find(|key| key.id == id)
            .ok_or("receipt key is not trusted")?;
        if key.revoked || !key.purposes.contains(&Purpose::AuditReceipt) {
            return Err("receipt key is revoked or not authorized for receipts".into());
        }
        Ok(Self {
            id: id.into(),
            public_key_sha256: sha256(&crate::decode::<32>(&key.public_key_hex)?),
        })
    }
}

/// Created only from a terminal receipt prepared against a verified release.
/// Dropping this object deliberately leaves the reservation on disk. A crash or
/// backend uncertainty must never silently reopen the signing opportunity.
#[must_use = "dropping an issuance reservation leaves it pending for reconciliation"]
pub struct ReceiptReservation {
    directory: PathBuf,
    operation: String,
    pending: Pending,
}

fn identity(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
    {
        return Err(
            "issuance identity must be 1-128 ASCII letters, digits, dots, dashes or underscores"
                .into(),
        );
    }
    Ok(())
}

/// Stable slot identity; changing a release, receipt or key must not reopen it.
pub fn terminal_operation_id(tenant: &str, session: &str) -> Result<String, String> {
    identity(tenant)?;
    identity(session)?;
    Ok(sha256(
        &serde_json::to_vec(&serde_json::json!({
            "tenant_id":tenant,"session_id":session,"kind":"terminal"
        }))
        .map_err(|e| e.to_string())?,
    ))
}

fn check_directory(directory: &Path) -> Result<(), String> {
    #[cfg(not(unix))]
    {
        let _ = directory;
        Err("durable receipt issuance is unsupported on this platform".into())
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::symlink_metadata(directory).map_err(|e| e.to_string())?;
        if !metadata.is_dir() || metadata.mode() & 0o077 != 0 {
            return Err("issuance directory must be a private directory, not a symlink".into());
        }
        // SAFETY: geteuid has no arguments or memory effects.
        if metadata.uid() != unsafe { libc::geteuid() } {
            return Err("issuance directory must belong to the running service identity".into());
        }
        Ok(())
    }
}

fn create_durable(directory: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    check_directory(directory)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(directory.join(name)).map_err(|e| {
        format!(
            "cannot create exclusive issuance record; existing state requires reconciliation: {e}"
        )
    })?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| format!("cannot persist issuance record; reconciliation required: {e}"))?;
    File::open(directory)
        .and_then(|dir| dir.sync_all())
        .map_err(|e| format!("cannot persist issuance directory; reconciliation required: {e}"))
}

pub(crate) fn reserve(
    directory: &Path,
    tenant: &str,
    session: &str,
    key: &ReceiptKey,
    receipt: Vec<u8>,
) -> Result<ReceiptReservation, String> {
    for value in [tenant, session, key.id.as_str()] {
        identity(value)?;
    }
    let fingerprint = crate::decode::<32>(&key.public_key_sha256)?;
    check_directory(directory)?;
    let directory = directory.canonicalize().map_err(|e| e.to_string())?;
    // Neither release, receipt nor key enters this identity: changing any of them
    // must not create another terminal-signing slot for the same tenant/session.
    let operation = terminal_operation_id(tenant, session)?;
    let pending = Pending {
        format: "devlish-receipt-reservation".into(),
        format_version: 1,
        tenant_id: tenant.into(),
        session_id: session.into(),
        key_id: key.id.clone(),
        key_public_sha256: crate::hex(&fingerprint),
        receipt_sha256: sha256(&receipt),
        receipt: String::from_utf8(receipt).map_err(|e| e.to_string())?,
    };
    create_durable(
        &directory,
        &format!("{operation}.pending.json"),
        &serde_json::to_vec(&pending).map_err(|e| e.to_string())?,
    )?;
    Ok(ReceiptReservation {
        directory,
        operation,
        pending,
    })
}

impl ReceiptReservation {
    pub fn receipt(&self) -> &[u8] {
        self.pending.receipt.as_bytes()
    }
    pub fn receipt_sha256(&self) -> &str {
        &self.pending.receipt_sha256
    }
    pub fn key_id(&self) -> &str {
        &self.pending.key_id
    }
    pub fn operation_id(&self) -> &str {
        &self.operation
    }

    /// Record a signature produced by a separately authorized backend. This
    /// consumes the reservation even on error: retry must reconcile the pending
    /// operation instead of requesting another signature. No key is loaded here.
    pub fn complete(self, signature: &[u8], trust: &[u8]) -> Result<(), String> {
        let verified = verify(self.receipt(), signature, trust, Purpose::AuditReceipt)?;
        if verified.signer_key_id != self.pending.key_id
            || verified.signer_public_key_sha256 != self.pending.key_public_sha256
        {
            return Err("receipt was signed by a different key than reserved".into());
        }
        let signature: serde_json::Value =
            serde_json::from_slice(signature).map_err(|e| e.to_string())?;
        let result = serde_json::json!({"format":"devlish-receipt-issuance","format_version":1,
            "operation_id":self.operation,"receipt_sha256":self.pending.receipt_sha256,
            "key_id":self.pending.key_id,"signature":signature,"signature_verification":verified,
            "signature_verified":true,"signer_authorization_verified":false,"execution_origin_verified":false,"policy_enforcement_verified":false});
        create_durable(
            &self.directory,
            &format!("{}.completed.json", self.operation),
            &serde_json::to_vec(&result).map_err(|e| e.to_string())?,
        )
    }
}

/// Independently obtained expectations, never read from candidate records.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IssuanceExpectations {
    pub tenant_id: String,
    pub session_id: String,
    pub key_id: String,
    pub key_public_sha256: String,
    pub receipt_sha256: String,
    pub release_manifest_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Completed {
    format: String,
    format_version: u32,
    operation_id: String,
    receipt_sha256: String,
    key_id: String,
    signature: serde_json::Value,
    // Historical assertions are required for schema compatibility, but never
    // used as evidence. Fresh trust and cryptographic verification decide.
    #[serde(rename = "signature_verification")]
    _signature_verification: serde_json::Value,
    #[serde(rename = "signature_verified")]
    _signature_verified: bool,
    #[serde(rename = "signer_authorization_verified")]
    _signer_authorization_verified: bool,
    #[serde(rename = "execution_origin_verified")]
    _execution_origin_verified: bool,
    #[serde(rename = "policy_enforcement_verified")]
    _policy_enforcement_verified: bool,
}

#[derive(Debug, Serialize)]
pub struct IssuanceVerification {
    pub format: &'static str,
    pub format_version: u32,
    pub operation_id: String,
    pub pending_file_sha256: String,
    pub completed_file_sha256: String,
    pub expectations_sha256: String,
    pub issuance_records_consistent: bool,
    pub stored_verification_trusted: bool,
    pub tenant_binding_authenticated: bool,
    pub signer_authorization_verified: bool,
    pub issuer_history_replayed: bool,
    pub execution_origin_verified: bool,
    pub policy_enforcement_verified: bool,
    pub receipt: crate::receipt::ReceiptVerification,
    pub explanation: &'static str,
}

/// Read-only verification of one saved terminal issuance. This never opens a
/// signing slot, retries a backend or treats a saved report as trusted evidence.
pub fn verify_issuance(
    log: &[u8],
    pending_bytes: &[u8],
    completed_bytes: &[u8],
    trust: &[u8],
    expectations: &[u8],
) -> Result<IssuanceVerification, String> {
    if [pending_bytes, completed_bytes, expectations]
        .iter()
        .any(|bytes| bytes.len() as u64 > crate::MAX_METADATA_BYTES)
    {
        return Err("issuance metadata exceeds the size limit".into());
    }
    let expected: IssuanceExpectations = serde_json::from_slice(expectations)
        .map_err(|e| format!("invalid independent issuance expectations: {e}"))?;
    let pending: Pending = serde_json::from_slice(pending_bytes)
        .map_err(|e| format!("invalid pending issuance record: {e}"))?;
    let completed: Completed = serde_json::from_slice(completed_bytes)
        .map_err(|e| format!("invalid completed issuance record: {e}"))?;
    let operation = terminal_operation_id(&expected.tenant_id, &expected.session_id)?;
    identity(&expected.key_id)?;
    let expected_key = crate::hex(&crate::decode::<32>(&expected.key_public_sha256)?);
    if pending.format != "devlish-receipt-reservation"
        || pending.format_version != 1
        || completed.format != "devlish-receipt-issuance"
        || completed.format_version != 1
    {
        return Err("unsupported issuance record format".into());
    }
    if pending.tenant_id != expected.tenant_id
        || pending.session_id != expected.session_id
        || pending.key_id != expected.key_id
        || pending.key_public_sha256 != expected_key
        || completed.key_id != expected.key_id
        || completed.operation_id != operation
    {
        return Err("issuance identity does not match independent expectations".into());
    }
    let receipt_digest = sha256(pending.receipt.as_bytes());
    if pending.receipt_sha256 != receipt_digest || completed.receipt_sha256 != receipt_digest {
        return Err("issuance records disagree with the retained receipt bytes".into());
    }
    let signature = serde_json::to_vec(&completed.signature).map_err(|e| e.to_string())?;
    let receipt = crate::receipt::verify_receipt(
        log,
        pending.receipt.as_bytes(),
        &signature,
        trust,
        crate::receipt::ExpectedReceipt {
            sha256: &expected.receipt_sha256,
            session_id: &expected.session_id,
            release_sha256: &expected.release_manifest_sha256,
        },
    )?;
    if !receipt.terminal_receipt_verified
        || receipt.receipt_signature.signer_key_id != expected.key_id
        || receipt.receipt_signature.signer_public_key_sha256 != expected_key
    {
        return Err("issuance requires a terminal receipt signed by the pinned key".into());
    }
    Ok(IssuanceVerification {
        format: "devlish-issuance-verification",
        format_version: 1,
        operation_id: operation,
        pending_file_sha256: sha256(pending_bytes),
        completed_file_sha256: sha256(completed_bytes),
        expectations_sha256: sha256(expectations),
        issuance_records_consistent: true,
        stored_verification_trusted: false,
        tenant_binding_authenticated: false,
        signer_authorization_verified: false,
        issuer_history_replayed: false,
        execution_origin_verified: false,
        policy_enforcement_verified: false,
        receipt,
        explanation: "The saved records agree with independent expectations and a freshly verified terminal receipt and log. Stored verification flags are ignored. The unsigned reservation does not authenticate tenant binding, authorization, issuance order, uniqueness or protected execution.",
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::{hex, signing_message};
    use ring::{
        rand::SystemRandom,
        signature::{Ed25519KeyPair, KeyPair},
    };
    use serde_json::{json, Value};
    use std::{
        os::unix::fs::PermissionsExt,
        sync::{
            atomic::{AtomicU64, Ordering},
            Arc, Barrier,
        },
        time::{SystemTime, UNIX_EPOCH},
    };

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
    struct Directory(PathBuf);
    impl Directory {
        fn new() -> Self {
            Self::new_in(&std::env::temp_dir())
        }
        fn new_in(parent: &Path) -> Self {
            let path = parent.join(format!(
                "devlish-issuance-{}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
    }
    impl Drop for Directory {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn pin(id: &str, public_key: &[u8]) -> ReceiptKey {
        ReceiptKey {
            id: id.into(),
            public_key_sha256: sha256(public_key),
        }
    }
    fn stub_key() -> ReceiptKey {
        pin("key", &[1; 32])
    }
    fn candidate() -> Vec<u8> {
        br#"{"synthetic":"receipt bytes"}"#.to_vec()
    }
    fn key() -> Ed25519KeyPair {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap()
    }
    fn signed(key: &Ed25519KeyPair, id: &str, purpose: Purpose, data: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let signature = json!({"format":"devlish-detached-signature","format_version":1,"algorithm":"ed25519","key_id":id,"purpose":purpose,"signature_hex":hex(key.sign(&signing_message(purpose,data)).as_ref())});
        let trust = json!({"format":"devlish-audit-trust","format_version":1,"keys":[{"id":id,"public_key_hex":hex(key.public_key().as_ref()),"purposes":[purpose],"revoked":false}]});
        (
            serde_json::to_vec(&signature).unwrap(),
            serde_json::to_vec(&trust).unwrap(),
        )
    }
    #[test]
    fn concurrent_reservations_have_one_winner_and_drop_never_reopens() {
        // Arrange: every contender addresses the same tenant/session slot.
        let directory = Directory::new();
        let barrier = Arc::new(Barrier::new(8));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let path = directory.0.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    reserve(&path, "tenant", "session", &stub_key(), candidate()).is_ok()
                })
            })
            .collect();
        // Act and assert: exactly one winner, even after its guard is dropped.
        assert_eq!(
            threads
                .into_iter()
                .map(|t| t.join().unwrap())
                .filter(|v| *v)
                .count(),
            1
        );
        assert!(reserve(&directory.0, "tenant", "session", &stub_key(), candidate()).is_err());
        assert!(reserve(
            &directory.0,
            "tenant",
            "session",
            &pin("rotated-key", &[2; 32]),
            b"different receipt".to_vec()
        )
        .is_err());
        assert!(reserve(
            &directory.0,
            "other-tenant",
            "session",
            &stub_key(),
            candidate()
        )
        .is_ok());
        assert!(reserve(
            &directory.0,
            "tenant",
            "other-session",
            &stub_key(),
            candidate()
        )
        .is_ok());
    }
    #[test]
    fn completion_checks_exact_bytes_key_purpose_and_revocation() {
        let directory = Directory::new();
        let replacement_key = key();
        let key = key();
        for (i, mode) in [
            "valid",
            "different-bytes",
            "wrong-key",
            "rebound-key-id",
            "wrong-purpose",
            "revoked",
        ]
        .iter()
        .enumerate()
        {
            let reservation = reserve(
                &directory.0,
                "tenant",
                &format!("s-{i}"),
                &pin("receipt-key", key.public_key().as_ref()),
                candidate(),
            )
            .unwrap();
            let operation = reservation.operation_id().to_string();
            let (signature, mut trust) = signed(
                if *mode == "rebound-key-id" {
                    &replacement_key
                } else {
                    &key
                },
                if *mode == "wrong-key" {
                    "other"
                } else {
                    "receipt-key"
                },
                if *mode == "wrong-purpose" {
                    Purpose::ReleaseArtifact
                } else {
                    Purpose::AuditReceipt
                },
                if *mode == "different-bytes" {
                    b"replacement"
                } else {
                    reservation.receipt()
                },
            );
            if *mode == "revoked" {
                let mut value: Value = serde_json::from_slice(&trust).unwrap();
                value["keys"][0]["revoked"] = json!(true);
                trust = serde_json::to_vec(&value).unwrap();
            }
            let result = reservation.complete(&signature, &trust);
            assert_eq!(result.is_ok(), *mode == "valid", "{mode}: {result:?}");
            let pending = directory.0.join(format!("{operation}.pending.json"));
            assert!(pending.exists());
            assert_eq!(
                std::fs::metadata(&pending).unwrap().permissions().mode() & 0o777,
                0o600
            );
            let completed = directory.0.join(format!("{operation}.completed.json"));
            assert_eq!(completed.exists(), *mode == "valid");
            if completed.exists() {
                let record: Value =
                    serde_json::from_slice(&std::fs::read(completed).unwrap()).unwrap();
                assert_eq!(record["receipt_sha256"], sha256(&candidate()));
                assert_eq!(record["execution_origin_verified"], false);
            }
            assert!(reserve(
                &directory.0,
                "tenant",
                &format!("s-{i}"),
                &pin("receipt-key", key.public_key().as_ref()),
                candidate()
            )
            .is_err());
        }
    }
    #[test]
    fn unsafe_directory_and_identities_are_rejected_before_creation() {
        let directory = Directory::new();
        for bad in ["", "../escape", "tenant/other", "tenant\n"] {
            assert!(reserve(&directory.0, bad, "session", &stub_key(), candidate()).is_err());
        }
        let malformed_key = ReceiptKey {
            id: "key".into(),
            public_key_sha256: "bad".into(),
        };
        assert!(reserve(
            &directory.0,
            "tenant",
            "session",
            &malformed_key,
            candidate()
        )
        .is_err());
        assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 0);
        std::fs::set_permissions(&directory.0, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(reserve(&directory.0, "tenant", "session", &stub_key(), candidate()).is_err());
        std::fs::set_permissions(&directory.0, std::fs::Permissions::from_mode(0o700)).unwrap();
        let link = directory.0.join("link");
        std::os::unix::fs::symlink(&directory.0, &link).unwrap();
        assert!(reserve(&link, "tenant", "session", &stub_key(), candidate()).is_err());
    }
    #[test]
    fn completion_never_overwrites_existing_record() {
        let directory = Directory::new();
        let key = key();
        let reservation = reserve(
            &directory.0,
            "tenant",
            "session",
            &pin("key", key.public_key().as_ref()),
            candidate(),
        )
        .unwrap();
        let output = directory
            .0
            .join(format!("{}.completed.json", reservation.operation_id()));
        std::fs::write(&output, b"prior incomplete completion").unwrap();
        let (signature, trust) = signed(&key, "key", Purpose::AuditReceipt, reservation.receipt());
        assert!(reservation.complete(&signature, &trust).is_err());
        assert_eq!(
            std::fs::read(&output).unwrap(),
            b"prior incomplete completion"
        );
    }
    #[test]
    fn relative_directory_is_bound_before_later_completion() {
        let directory = Directory::new_in(Path::new("."));
        let reservation =
            reserve(&directory.0, "tenant", "session", &stub_key(), candidate()).unwrap();
        assert!(reservation.directory.is_absolute());
        assert_eq!(reservation.directory, directory.0.canonicalize().unwrap());
    }
}
