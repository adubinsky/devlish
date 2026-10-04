//! Transport bounds for catalog tool requests, not authorization to execute.
use serde_json::Value;

/// Returns the logical catalog ID. The host must still match argv against its
/// approved catalog and bind verification to the executed object. Request data
/// cannot set a binary path, environment, cwd, stdin or shell command.
pub fn validate(request: &Value) -> Result<&str, String> {
    let invalid = || "invalid catalog tool request".to_string();
    let object = request.as_object().ok_or_else(invalid)?;
    if object.len() != 2 {
        return Err(invalid());
    }
    let id = object
        .get("tool_id")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
    {
        return Err(invalid());
    }
    let arguments = object
        .get("arguments")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    if arguments.len() > 64 {
        return Err(invalid());
    }
    let mut total = 0;
    for argument in arguments {
        let value = argument.as_str().ok_or_else(invalid)?;
        if value.len() > 4096 || value.contains('\0') {
            return Err(invalid());
        }
        total += value.len();
        if total > 16384 {
            return Err(invalid());
        }
    }
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exact_transport_limits_are_accepted_and_next_byte_is_rejected() {
        let mut request = json!({"tool_id":"a".repeat(128),"arguments":vec!["é".repeat(2048);4]});
        assert!(validate(&request).is_ok());
        request["arguments"]
            .as_array_mut()
            .unwrap()
            .push(json!("x"));
        assert!(validate(&request).is_err());
        request["arguments"] = json!(vec![""; 64]);
        assert!(validate(&request).is_ok());
        request["arguments"].as_array_mut().unwrap().push(json!(""));
        assert!(validate(&request).is_err());
    }

    #[test]
    fn ids_cannot_be_empty_paths_unicode_or_oversized() {
        for id in [
            "".to_string(),
            "/usr/bin/grep".into(),
            "../grep".into(),
            "grép".into(),
            "a".repeat(129),
        ] {
            assert!(validate(&json!({"tool_id":id,"arguments":[]})).is_err());
        }
        assert!(
            validate(&json!({"tool_id":"Grep_1-public","arguments":["$(literal); text"]})).is_ok()
        );
    }
}
