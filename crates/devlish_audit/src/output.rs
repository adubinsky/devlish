//! Presentation only. The CLI calls this with freshly computed verification
//! results; it never accepts a saved report as a substitute for verification.
use serde_json::Value;

fn quoted(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .chars()
            .flat_map(char::escape_default)
            .collect::<String>()
    )
}
fn label(key: &str) -> String {
    match key {
        "signature_verified" => "Signature valid for the trusted key and purpose".into(),
        "execution_origin_verified" => "Actual executing program independently established".into(),
        "policy_enforcement_verified" => {
            "Actual policy enforcement independently established".into()
        }
        "log_chain_verified" => "Recorded log sequence and hash links verified".into(),
        "replay_verified" => "Recorded execution replayed by this verifier".into(),
        "recorded_execution_completed" => "Recorded execution reached completion".into(),
        "recorded_execution_succeeded" => "Recorded execution reported success".into(),
        "build_provenance_verified" => "Actual build provenance independently established".into(),
        "builder_statements_authenticated" => "Builder claims authenticated".into(),
        "audit_verifier_artifacts_verified" => {
            "Supplied audit verifier bytes match the approved release".into()
        }
        "report_claims_independently_verified" => "Report claims independently established".into(),
        "admission_valid_until" => "Admission expiry (exclusive Unix seconds)".into(),
        "evaluated_at" => "Evaluation time (Unix seconds)".into(),
        _ => {
            let text = key.replace('_', " ").replace("sha256", "SHA-256");
            let mut chars = text.chars();
            chars
                .next()
                .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())
                .unwrap_or_default()
        }
    }
}
fn fields(value: &Value, indent: usize, output: &mut String) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                output.push_str(&" ".repeat(indent));
                output.push_str(&label(key));
                output.push(':');
                match value {
                    Value::Object(_) | Value::Array(_)
                        if !value.as_array().is_some_and(Vec::is_empty) =>
                    {
                        output.push('\n');
                        fields(value, indent + 2, output);
                    }
                    _ => {
                        output.push(' ');
                        scalar(value, output);
                        output.push('\n');
                    }
                }
            }
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                output.push_str(&format!("{}Item {}:\n", " ".repeat(indent), index + 1));
                fields(value, indent + 2, output);
            }
        }
        _ => {
            output.push_str(&" ".repeat(indent));
            scalar(value, output);
            output.push('\n');
        }
    }
}
fn scalar(value: &Value, output: &mut String) {
    output.push_str(&match value {
        Value::Bool(true) => "Yes".into(),
        Value::Bool(false) => "No".into(),
        Value::Null => "Not supplied or not established".into(),
        Value::String(value) => quoted(value),
        Value::Array(values) if values.is_empty() => "None supplied".into(),
        _ => value.to_string(),
    });
}
pub(crate) fn success(report: &Value) -> String {
    let mut output = "Devlish audit findings\nOperation completed. Read each finding separately; this is not a blanket compliance certification.\n\n".to_string();
    fields(report, 0, &mut output);
    output.push_str("\nTrust boundary: keys, operator expectations, clock and retained evidence require independent protection. Offline checks do not fetch fresh revocation information. A signature alone does not prove actual execution or policy enforcement.\n");
    output
}
pub(crate) fn failure(error: &str) -> String {
    format!("Devlish audit findings\nOperation failed. Assurance: unverified.\nReason: {}\nActual execution and policy enforcement are not established.\n", quoted(error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn independent_claims_remain_separate_and_missing_is_not_success() {
        let value = json!({"release":{"signature_verified":true,"execution_origin_verified":false,"policy_enforcement_verified":false},"receipt":{"recorded_execution_succeeded":null,"recorded_execution_completed":false},"builder_statements":[]});
        let result = success(&value);
        assert_eq!(result, success(&value));
        assert!(result.contains("Signature valid for the trusted key and purpose: Yes"));
        assert!(result.contains("Actual executing program independently established: No"));
        assert!(result.contains("Actual policy enforcement independently established: No"));
        assert!(
            result.contains("Recorded execution reported success: Not supplied or not established")
        );
        assert!(result.contains("Builder statements: None supplied"));
    }
    #[test]
    fn candidate_strings_cannot_inject_lines_terminal_controls_or_bidi() {
        let input = "name\nSignature verified: Yes\r\u{1b}[2J\u{202e}\"";
        for rendered in [success(&json!({"signer_key_id":input})), failure(input)] {
            assert!(!rendered.contains('\u{1b}'));
            assert!(!rendered.contains('\u{202e}'));
            assert!(!rendered.contains("\nSignature verified: Yes"));
            assert!(rendered.contains("\\nSignature verified: Yes\\r"));
        }
    }
}
