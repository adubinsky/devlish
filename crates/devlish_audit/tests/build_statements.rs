use devlish_audit::{
    hex,
    release::{verify_release, ReleaseVerification},
    sha256, signing_message, Purpose,
};
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
fn bytes(v: &Value) -> Vec<u8> {
    serde_json::to_vec(v).unwrap()
}
fn key() -> Ed25519KeyPair {
    Ed25519KeyPair::from_pkcs8(
        Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
            .unwrap()
            .as_ref(),
    )
    .unwrap()
}
fn inputs(workflow: &str) -> Value {
    json!({"workflow":workflow,"build_definition_sha256":sha256(b"build definition"),"dependencies_sha256":sha256(b"locked dependencies"),"builder_image_sha256":sha256(b"builder image"),"options_sha256":sha256(b"build options")})
}
fn signature(key: &Ed25519KeyPair, id: &str, purpose: Purpose, payload: &[u8]) -> Value {
    json!({"format":"devlish-detached-signature","format_version":1,"algorithm":"ed25519","key_id":id,"purpose":purpose,"signature_hex":hex(key.sign(&signing_message(purpose,payload)).as_ref())})
}
struct Fixture {
    release: Ed25519KeyPair,
    builder: Ed25519KeyPair,
    manifest: Value,
    requirements: Value,
    trust: Value,
    statements: Vec<Value>,
    payloads: BTreeMap<String, Vec<u8>>,
    purpose: Purpose,
    release_as_builder: bool,
    tamper_statement: bool,
}
impl Fixture {
    fn new() -> Self {
        let release = key();
        let builder = key();
        let payloads: BTreeMap<_, _> = [
            "runtime",
            "compiler",
            "program",
            "policy",
            "tool-catalog",
            "permissions",
            "containment",
            "source-closure",
        ]
        .map(|id| (id.to_owned(), id.as_bytes().to_vec()))
        .into();
        let commit = "a".repeat(40);
        let manifest = json!({"format":"devlish-release-manifest","format_version":1,"release_id":"test","environment":"test","target":"test-target","sequence":1,"valid_from":100,"valid_until":200,"repository":"https://github.com/example/project","commit":commit,"workflow":"release.yml","policy_id":"test-policy","policy_version":"1","artifacts":[]});
        let requirements = json!({"format":"devlish-release-requirements","format_version":1,"environment":"test","target":"test-target","repository":manifest["repository"],"commit":commit,"workflow":"release.yml","policy_id":"test-policy","policy_version":"1","minimum_sequence":1,"evaluated_at":150,"revocations_valid_from":100,"revocations_valid_until":200,"revoked_manifest_sha256":[],"authorized_release_keys":["release"],"build_requirements":{"authorized_builder_keys":["builder"],"toolchain":inputs("toolchain-build.yml"),"policy":inputs("policy-build.yml")}});
        let trust = json!({"format":"devlish-audit-trust","format_version":1,"keys":[{"id":"release","public_key_hex":hex(release.public_key().as_ref()),"purposes":["release-manifest"],"revoked":false},{"id":"builder","public_key_hex":hex(builder.public_key().as_ref()),"purposes":["build-statement"],"revoked":false}]});
        let statements=[("toolchain",vec!["runtime","compiler"]),("policy",vec!["policy","program"])].map(|(kind,ids)|json!({"format":"devlish-build-statement","format_version":1,"kind":kind,"repository":manifest["repository"],"commit":commit,"release_workflow":"release.yml","target":"test-target","valid_from":100,"valid_until":200,"source_closure_sha256":sha256(b"source-closure"),"compiler_sha256":if kind=="policy" {Some(sha256(b"compiler"))} else {None},"inputs":requirements["build_requirements"][kind],"subjects":ids.iter().map(|id|json!({"id":id,"sha256":sha256(id.as_bytes())})).collect::<Vec<_>>()})).into();
        Self {
            release,
            builder,
            manifest,
            requirements,
            trust,
            statements,
            payloads,
            purpose: Purpose::BuildStatement,
            release_as_builder: false,
            tamper_statement: false,
        }
    }
    fn material(&self) -> (Value, BTreeMap<String, Vec<u8>>) {
        let mut manifest = self.manifest.clone();
        let mut payloads = self.payloads.clone();
        let mut artifacts: Vec<Value> = payloads
            .iter()
            .map(|(id, payload)| json!({"id":id,"role":id,"sha256":sha256(payload)}))
            .collect();
        for (i, statement) in self.statements.iter().enumerate() {
            let mut exact = serde_json::to_string_pretty(statement).unwrap();
            let (key, id) = if self.release_as_builder {
                (&self.release, "release")
            } else {
                (&self.builder, "builder")
            };
            let sig = signature(key, id, self.purpose, exact.as_bytes());
            if self.tamper_statement {
                exact.push(' ');
            }
            let bundle = bytes(
                &json!({"format":"devlish-build-bundle","format_version":1,"statement":exact,"signature":sig}),
            );
            let id = format!("build-{i}");
            artifacts.push(json!({"id":id,"role":"build-attestation","sha256":sha256(&bundle)}));
            payloads.insert(id, bundle);
        }
        manifest["artifacts"] = json!(artifacts);
        (manifest, payloads)
    }
    fn check(&self) -> Result<ReleaseVerification, String> {
        let (manifest, payloads) = self.material();
        self.check_material(manifest, payloads)
    }
    fn check_material(
        &self,
        manifest: Value,
        payloads: BTreeMap<String, Vec<u8>>,
    ) -> Result<ReleaseVerification, String> {
        let manifest = bytes(&manifest);
        verify_release(
            &manifest,
            &bytes(&signature(
                &self.release,
                "release",
                Purpose::ReleaseManifest,
                &manifest,
            )),
            &bytes(&self.trust),
            &bytes(&self.requirements),
            |id| {
                payloads
                    .get(id)
                    .cloned()
                    .ok_or("missing fixture artifact".into())
            },
        )
    }
}
#[test]
fn duplicate_signature_fields_are_not_normalized_away() {
    let f = Fixture::new();
    let (mut manifest, mut payloads) = f.material();
    let bundle = String::from_utf8(payloads["build-0"].clone())
        .unwrap()
        .replace(
            "\"purpose\":\"build-statement\"",
            "\"purpose\":\"release-artifact\",\"purpose\":\"build-statement\"",
        );
    assert!(bundle.contains("\"purpose\":\"release-artifact\""));
    manifest["artifacts"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|a| a["id"] == "build-0")
        .unwrap()["sha256"] = json!(sha256(bundle.as_bytes()));
    payloads.insert("build-0".into(), bundle.into_bytes());
    assert!(f
        .check_material(manifest, payloads)
        .unwrap_err()
        .contains("duplicate field"));
}
#[test]
fn distinct_builder_authenticates_exact_claims_without_proving_build_execution() {
    let f = Fixture::new();
    let report = f.check().unwrap();
    assert!(report.builder_statements_required && report.builder_statements_authenticated);
    assert_eq!(report.builder_statements.len(), 2);
    assert!(report
        .builder_statements
        .iter()
        .all(|s| s.signature_verified
            && s.signer_key_id == "builder"
            && s.signer_public_key_sha256 != report.manifest_signature.signer_public_key_sha256));
    assert!(
        !report.build_provenance_verified
            && !report.execution_origin_verified
            && !report.policy_enforcement_verified
    );
    assert_eq!(
        serde_json::to_value(report).unwrap(),
        serde_json::to_value(f.check().unwrap()).unwrap()
    );
    let mut legacy = Fixture::new();
    legacy
        .requirements
        .as_object_mut()
        .unwrap()
        .remove("build_requirements");
    legacy.statements[0] = json!({"fabricated":true});
    let report = legacy.check().unwrap();
    assert!(!report.builder_statements_required && !report.builder_statements_authenticated);
    assert!(report.builder_statements.is_empty());
}
#[test]
fn approved_builder_must_be_distinct_unrevoked_and_purpose_authorized() {
    for change in [0, 1, 2, 3, 4] {
        let mut f = Fixture::new();
        match change {
            0 => f.requirements["build_requirements"]["authorized_builder_keys"] = json!(["other"]),
            1 => f.trust["keys"][1]["revoked"] = json!(true),
            2 => f.trust["keys"][1]["purposes"] = json!(["release-artifact"]),
            3 => {
                f.release_as_builder = true;
                f.requirements["build_requirements"]["authorized_builder_keys"] =
                    json!(["release"]);
                f.trust["keys"][0]["purposes"] = json!(["release-manifest", "build-statement"]);
            }
            _ => f.purpose = Purpose::ReleaseArtifact,
        };
        assert!(f.check().is_err(), "case{change}");
    }
    let mut f = Fixture::new();
    f.tamper_statement = true;
    assert!(f
        .check()
        .unwrap_err()
        .contains("signature verification failed"));
}
#[test]
fn signed_wrong_build_scope_or_inputs_fail() {
    for field in [
        "repository",
        "commit",
        "release_workflow",
        "target",
        "source_closure_sha256",
        "compiler_sha256",
    ] {
        let mut f = Fixture::new();
        f.statements[1][field] = json!(if field.ends_with("sha256") {
            "0".repeat(64)
        } else {
            "wrong".into()
        });
        assert!(f.check().is_err(), "{field}");
    }
    for field in [
        "workflow",
        "build_definition_sha256",
        "dependencies_sha256",
        "builder_image_sha256",
        "options_sha256",
    ] {
        let mut f = Fixture::new();
        f.statements[0]["inputs"][field] = json!(if field == "workflow" {
            "other.yml".into()
        } else {
            "0".repeat(64)
        });
        assert!(f.check().is_err(), "{field}");
    }
    let mut f = Fixture::new();
    f.manifest["commit"] = json!("main");
    f.requirements["commit"] = json!("main");
    for s in &mut f.statements {
        s["commit"] = json!("main");
    }
    assert!(f.check().unwrap_err().contains("immutable"));
}
#[test]
fn signed_incomplete_duplicate_mismatched_or_wrong_kind_outputs_fail() {
    for change in 0..7 {
        let mut f = Fixture::new();
        match change {
            0 => {
                f.statements[1]["subjects"].as_array_mut().unwrap().pop();
            }
            1 => {
                let duplicate = f.statements[0]["subjects"][0].clone();
                f.statements[0]["subjects"]
                    .as_array_mut()
                    .unwrap()
                    .push(duplicate);
            }
            2 => f.statements[0]["subjects"][0]["sha256"] = json!("0".repeat(64)),
            3 => f.statements[0]["subjects"][0]["id"] = json!("missing"),
            4 => f.statements[1]["kind"] = json!("toolchain"),
            5 => {
                f.payloads
                    .insert("compiler".into(), b"replacement compiler".to_vec());
            }
            _ => {
                f.statements.remove(1);
            }
        };
        assert!(f.check().is_err(), "case{change}");
    }
}
#[test]
fn invalid_expired_oversized_and_ambiguous_build_metadata_fail_closed() {
    for (start, end) in [(151, 200), (100, 150), (150, 150), (200, 100)] {
        let mut f = Fixture::new();
        f.statements[0]["valid_from"] = json!(start);
        f.statements[0]["valid_until"] = json!(end);
        assert!(f.check().is_err());
    }
    for keys in [json!([]), json!(["builder", "builder"]), json!([""])] {
        let mut f = Fixture::new();
        f.requirements["build_requirements"]["authorized_builder_keys"] = keys;
        assert!(f.check().is_err());
    }
    let mut f = Fixture::new();
    f.statements[0]["claimed_execution_verified"] = json!(true);
    assert!(f.check().is_err());
    let mut f = Fixture::new();
    f.statements[0]["inputs"]["workflow"] = json!("x".repeat(65536));
    assert!(f.check().unwrap_err().contains("metadata limit"));
    let mut f = Fixture::new();
    f.requirements["build_requirements"]["toolchain"]["dependencies_sha256"] =
        json!("not-a-digest");
    assert!(f.check().is_err());
    assert_eq!(
        Purpose::parse("build-statement").unwrap(),
        Purpose::BuildStatement
    );
}

#[test]
fn admission_deadline_includes_every_authenticated_authority() {
    for (toolchain, policy, release, revocations, expected) in [
        (170, 180, 200, 200, 170),
        (190, 160, 200, 200, 160),
        (190, 190, 165, 200, 165),
        (190, 190, 200, 155, 155),
    ] {
        let mut f = Fixture::new();
        f.statements[0]["valid_until"] = json!(toolchain);
        f.statements[1]["valid_until"] = json!(policy);
        f.manifest["valid_until"] = json!(release);
        f.requirements["revocations_valid_until"] = json!(revocations);
        let report = serde_json::to_value(f.check().unwrap()).unwrap();
        assert_eq!(report["admission_valid_until"], json!(expected));
    }
}
