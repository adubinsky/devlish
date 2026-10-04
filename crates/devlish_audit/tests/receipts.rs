use devlish_audit::{
    hex,
    receipt::{verify_receipt, ExpectedReceipt},
    sha256, signing_message, Purpose,
};
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use serde_json::{json, Value};

struct Fixture {
    key: Ed25519KeyPair,
    trust: Vec<u8>,
    log: Vec<u8>,
    receipt: Vec<u8>,
    signature: Vec<u8>,
}
fn chain(records: &[Value]) -> Vec<u8> {
    let mut previous = String::new();
    let mut log = Vec::new();
    for (sequence, record) in records.iter().enumerate() {
        let mut envelope = json!({"sequence":sequence,"previous_sha256":previous,"record":record});
        previous = sha256(&serde_json::to_vec(&envelope).unwrap());
        envelope["record_sha256"] = json!(previous);
        log.extend(serde_json::to_vec(&envelope).unwrap());
        log.push(b'\n');
    }
    log
}
fn records() -> Vec<Value> {
    vec![
        json!({"type":"policy_run_started","format_version":3}),
        json!({"type":"effect_decision","effect_id":1,"effect":"clock_now","allow":true}),
        json!({"type":"effect_outcome","effect_id":1,"effect":"clock_now","outcome":{"status":"succeeded"}}),
        json!({"type":"policy_run_finished","success":true,"paused":false}),
    ]
}
impl Fixture {
    fn new(records: &[Value], kind: &str) -> Self {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        let key = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).unwrap();
        let trust=serde_json::to_vec(&json!({"format":"devlish-audit-trust","format_version":1,"keys":[{"id":"receipt-test","public_key_hex":hex(key.public_key().as_ref()),"purposes":["audit-receipt"],"revoked":false}]})).unwrap();
        let mut f = Self {
            key,
            trust,
            log: chain(records),
            receipt: vec![],
            signature: vec![],
        };
        f.reseal(kind);
        f
    }
    fn reseal(&mut self, kind: &str) {
        let lines: Vec<Value> = std::str::from_utf8(&self.log)
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        self.receipt=serde_json::to_vec(&json!({"format":"devlish-audit-receipt","format_version":1,"kind":kind,"session_id":"session-a","release_manifest_sha256":"a".repeat(64),"log_file_sha256":sha256(&self.log),"log_head_sha256":lines.last().unwrap()["record_sha256"],"record_count":lines.len()})).unwrap();
        self.resign();
    }
    fn resign(&mut self) {
        self.signature=serde_json::to_vec(&json!({"format":"devlish-detached-signature","format_version":1,"algorithm":"ed25519","key_id":"receipt-test","purpose":"audit-receipt","signature_hex":hex(self.key.sign(&signing_message(Purpose::AuditReceipt,&self.receipt)).as_ref())})).unwrap();
    }
    fn check(&self) -> Result<devlish_audit::receipt::ReceiptVerification, String> {
        verify_receipt(
            &self.log,
            &self.receipt,
            &self.signature,
            &self.trust,
            ExpectedReceipt {
                sha256: &sha256(&self.receipt),
                session_id: "session-a",
                release_sha256: &"a".repeat(64),
            },
        )
    }
}
#[test]
fn terminal_receipt_authenticates_history_without_proving_execution() {
    let f = Fixture::new(&records(), "terminal");
    let report = f.check().unwrap();
    assert!(
        report.log_chain_verified
            && report.retained_receipt_digest_matched
            && report.terminal_receipt_verified
    );
    assert_eq!(report.record_count, 4);
    assert_eq!(report.recorded_execution_succeeded, Some(true));
    assert!(
        !report.execution_origin_verified
            && !report.policy_enforcement_verified
            && !report.replay_verified
            && !report.release_manifest_verified
    );
}

#[test]
fn signed_fractional_clock_evidence_keeps_its_original_record_hash() {
    let mut history = records();
    // Without float_roundtrip this decimal parses one IEEE-754 value lower,
    // changing the envelope hash even though the log bytes were untouched.
    history[2]["exchange"] = json!({"ok":f64::from_bits(4745299267831201801)});
    let fixture = Fixture::new(&history, "terminal");
    let verified = fixture.check().unwrap();
    assert!(verified.log_chain_verified && verified.terminal_receipt_verified);
    assert!(!verified.execution_origin_verified);
}
#[test]
fn missing_tail_and_whole_history_replacement_fail_against_retained_receipt() {
    let mut f = Fixture::new(&records(), "terminal");
    f.log = chain(&records()[..3]);
    assert!(f.check().unwrap_err().contains("log bytes"));
    let mut altered = records();
    altered[1]["effect"] = json!("send_secret");
    f.log = chain(&altered);
    assert!(f.check().is_err());
    let original_anchor = sha256(&f.receipt);
    f.reseal("terminal");
    assert!(verify_receipt(
        &f.log,
        &f.receipt,
        &f.signature,
        &f.trust,
        ExpectedReceipt {
            sha256: &original_anchor,
            session_id: "session-a",
            release_sha256: &"a".repeat(64)
        }
    )
    .err()
    .unwrap()
    .contains("retained digest"));
}
#[test]
fn signed_but_broken_chain_is_rejected() {
    let mut f = Fixture::new(&records(), "terminal");
    let mut lines: Vec<Value> = std::str::from_utf8(&f.log)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    lines[1]["previous_sha256"] = json!("b".repeat(64));
    f.log = lines
        .iter()
        .flat_map(|l| {
            let mut b = serde_json::to_vec(l).unwrap();
            b.push(b'\n');
            b
        })
        .collect();
    f.reseal("terminal");
    assert!(f.check().err().unwrap().contains("previous hash"));
}
#[test]
fn receipt_cannot_be_substituted_across_sessions_or_releases() {
    let f = Fixture::new(&records(), "terminal");
    for (session, release) in [("other", "a".repeat(64)), ("session-a", "b".repeat(64))] {
        assert!(verify_receipt(
            &f.log,
            &f.receipt,
            &f.signature,
            &f.trust,
            ExpectedReceipt {
                sha256: &sha256(&f.receipt),
                session_id: session,
                release_sha256: &release
            }
        )
        .err()
        .unwrap()
        .contains("different session or release"));
    }
}
#[test]
fn interrupted_intent_is_uncertain_and_cannot_get_terminal_receipt() {
    let f = Fixture::new(&records()[..2], "checkpoint");
    let report = f.check().unwrap();
    assert_eq!(report.unmatched_intent_count, 1);
    assert!(!report.terminal_receipt_verified && !report.recorded_execution_completed);
    assert_eq!(report.recorded_execution_succeeded, None);
    assert!(Fixture::new(&records()[..2], "terminal").check().is_err());
    let mut r = records();
    r.remove(2);
    assert!(Fixture::new(&r, "terminal")
        .check()
        .err()
        .unwrap()
        .contains("unresolved"));
}
#[test]
fn failed_completion_and_paused_run_are_not_successful_completion() {
    let mut r = records();
    r[3]["success"] = json!(false);
    let report = Fixture::new(&r, "terminal").check().unwrap();
    assert_eq!(report.recorded_execution_succeeded, Some(false));
    r[3]["paused"] = json!(true);
    assert!(Fixture::new(&r, "terminal").check().is_err());
    let report = Fixture::new(&r, "checkpoint").check().unwrap();
    assert!(!report.recorded_execution_completed);
}
#[test]
fn outcome_without_permission_or_with_wrong_id_is_rejected() {
    for change in ["denied", "id", "effect", "after_finish"] {
        let mut r = records();
        match change {
            "denied" => r[1]["allow"] = json!(false),
            "id" => r[2]["effect_id"] = json!(2),
            "effect" => r[2]["effect"] = json!("http_request"),
            _ => r.push(r[1].clone()),
        }
        assert!(Fixture::new(&r, "terminal").check().is_err(), "{change}");
    }
}
#[test]
fn signed_receipt_count_head_and_version_are_checked() {
    for field in ["record_count", "log_head_sha256", "format_version"] {
        let mut f = Fixture::new(&records(), "terminal");
        let mut receipt: Value = serde_json::from_slice(&f.receipt).unwrap();
        receipt[field] = if field == "log_head_sha256" {
            json!("b".repeat(64))
        } else {
            json!(42)
        };
        f.receipt = serde_json::to_vec(&receipt).unwrap();
        f.resign();
        assert!(f.check().is_err(), "{field}");
    }
}

#[test]
fn recorded_session_and_verified_release_cannot_disagree_with_receipt() {
    let mut entries = records();
    entries[0]["session_id"] = json!("session-a");
    entries[0]["verified_release"] =
        json!({"session_id":"session-a","release_manifest_sha256":"a".repeat(64)});
    assert_eq!(
        Fixture::new(&entries, "terminal")
            .check()
            .unwrap()
            .session_binding_source,
        "receipt-signer-and-recorded-host-session"
    );
    entries[0]["session_id"] = json!("session-b");
    assert!(Fixture::new(&entries, "terminal").check().is_err());
    entries[0]["session_id"] = json!("session-a");
    entries[0]["verified_release"]["release_manifest_sha256"] = json!("b".repeat(64));
    assert!(Fixture::new(&entries, "terminal").check().is_err());
}
