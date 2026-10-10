//! Restricted authoring profile: validate untrusted drafts without executing them.
use crate::{
    compile_source_to_json, parse_source, CompileOptions, Expression, Statement, StatementKind,
};
use devlish_vm::policy::EffectPolicy;
use serde_json::{json, Value};

pub fn validate_requirements(value: &Value) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or("artifact requirements must be an object")?;
    if object.len() != 2
        || !object.contains_key("permissions")
        || !object.contains_key("write_contents")
    {
        return Err("artifact requirements need only permissions and write_contents".into());
    }
    let permissions = value["permissions"]
        .as_array()
        .ok_or("permissions must be an array")?;
    let writes = value["write_contents"]
        .as_object()
        .ok_or("write_contents must be an object")?;
    for permission in permissions {
        let fields = permission
            .as_object()
            .ok_or("permission must be an object")?;
        let kind = permission["kind"]
            .as_str()
            .ok_or("permission kind is required")?;
        if !matches!(
            kind,
            "llm_complete" | "read_file" | "write_file" | "http_request" | "clock" | "random"
        ) {
            return Err(
                "restricted authoring forbids process, filesystem, and service authority".into(),
            );
        }
        if fields.keys().any(|k| k != "kind" && k != "scope") {
            return Err("unsupported permission field".into());
        }
        if matches!(kind, "read_file" | "write_file" | "http_request") {
            let scope = permission["scope"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("file and HTTP permissions require exact scopes")?;
            if kind == "write_file" {
                let lowered = scope.to_ascii_lowercase();
                if [".py", ".pyw", ".pyc", ".pyo", ".sh", ".bash", ".zsh"]
                    .iter()
                    .any(|extension| lowered.ends_with(extension))
                {
                    return Err("restricted authoring forbids Python and shell output paths".into());
                }
            }
            if kind == "write_file" && !writes.contains_key(scope) {
                return Err("writes require operator-approved literal content".into());
            }
        }
    }
    for (path, contents) in writes {
        if !permissions
            .iter()
            .any(|p| p["kind"] == "write_file" && p["scope"] == path.as_str())
        {
            return Err("write_contents must refer to an approved write permission".into());
        }
        let values = contents
            .as_array()
            .filter(|a| !a.is_empty())
            .ok_or("write contents must be a nonempty list")?;
        if values.iter().any(|v| !v.is_string()) {
            return Err("write contents must be literal text".into());
        }
    }
    Ok(())
}

pub fn validate_draft(payload: &Value, requirements: &Value) -> Result<(), String> {
    validate_requirements(requirements)?;
    let fields = payload.as_object().ok_or("draft must be an object")?;
    if fields.len() != 2 {
        return Err("draft requires exactly program and policy".into());
    }
    let program = payload["program"].as_str().ok_or("program must be text")?;
    let policy = payload["policy"].as_str().ok_or("policy must be text")?;
    for source in [program, policy] {
        if source.trim().is_empty() || source.len() > 128 * 1024 {
            return Err("artifact text must be nonempty and at most 128 KiB".into());
        }
    }
    let parsed =
        parse_source(program).map_err(|_| "generated program is not valid flat Devlish")?;
    let declared: Vec<Value> = parsed
        .manifest
        .as_ref()
        .map(|m| {
            m.permissions
                .iter()
                .map(|p| {
                    if let Some(scope) = &p.scope {
                        json!({"kind":p.kind,"scope":scope})
                    } else {
                        json!({"kind":p.kind})
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let approved = requirements["permissions"].as_array().unwrap();
    if declared
        .iter()
        .any(|permission| !approved.contains(permission))
    {
        return Err("generated permissions exceed operator authority".into());
    }
    validate_statements(&parsed.statements, requirements, false)?;
    let parsed_policy =
        parse_source(policy).map_err(|_| "generated policy is not valid flat Devlish")?;
    validate_statements(
        &parsed_policy.statements,
        &json!({"permissions":[],"write_contents":{}}),
        true,
    )?;
    for (source, is_policy) in [(program, false), (policy, true)] {
        let compiled = compile_source_to_json(
            source,
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .map_err(|_| "generated artifact failed Devlish compilation")?;
        if is_policy {
            EffectPolicy::new(
                serde_json::from_str(&compiled).map_err(|_| "invalid compiled policy")?,
            )?;
        }
    }
    Ok(())
}

fn literal(expression: &Expression) -> Option<&str> {
    match expression {
        Expression::Literal(value) => value.as_str(),
        _ => None,
    }
}
fn validate_statements(
    statements: &[Statement],
    requirements: &Value,
    policy: bool,
) -> Result<(), String> {
    for statement in statements {
        match &statement.kind {
            StatementKind::Import { .. } | StatementKind::UseModule { .. } => {
                return Err("generated imports are prohibited before resolution".into())
            }
            StatementKind::RunTool { .. }
            | StatementKind::ServiceCall { .. }
            | StatementKind::Route { .. }
            | StatementKind::HttpDownload { .. }
            | StatementKind::FileCopy { .. }
            | StatementKind::FileMove { .. }
            | StatementKind::FileMkdir { .. }
            | StatementKind::FileDelete { .. }
            | StatementKind::FileGlob { .. }
            | StatementKind::FileStat { .. }
            | StatementKind::FileExists { .. }
            | StatementKind::FileList { .. }
            | StatementKind::ExportAssertions { .. }
            | StatementKind::Output { .. }
            | StatementKind::ReadStdin { .. }
            | StatementKind::Trigger { .. } => {
                return Err("generated action is outside restricted authoring profile".into())
            }
            StatementKind::FileWrite { value, path, .. } => {
                let path = literal(path).ok_or("generated writes require literal paths")?;
                let content = literal(value).ok_or("generated writes require literal content")?;
                let allowed = requirements["write_contents"][path]
                    .as_array()
                    .ok_or("generated write path is not approved")?;
                if !allowed.contains(&json!(content)) {
                    return Err("generated write content is not approved".into());
                }
            }
            StatementKind::FileRead { path, .. } => {
                let path = literal(path).ok_or("generated reads require literal paths")?;
                if !requirements["permissions"]
                    .as_array()
                    .unwrap()
                    .contains(&json!({"kind":"read_file","scope":path}))
                {
                    return Err("generated read path is not approved".into());
                }
            }
            StatementKind::LlmComplete { .. }
            | StatementKind::HttpRequest { .. }
            | StatementKind::ClockNow { .. }
            | StatementKind::RandomDraw { .. }
                if policy =>
            {
                return Err("generated policy must have no external effects".into())
            }
            StatementKind::HttpRequest { url, .. } => {
                let url = literal(url)
                    .ok_or("generated HTTP requests require a literal approved endpoint")?;
                if !requirements["permissions"]
                    .as_array()
                    .unwrap()
                    .contains(&json!({"kind":"http_request","scope":url}))
                {
                    return Err("generated HTTP endpoint is not approved".into());
                }
            }
            StatementKind::Branch {
                then_statements,
                else_statements,
                ..
            } => {
                validate_statements(then_statements, requirements, policy)?;
                validate_statements(else_statements, requirements, policy)?;
            }
            StatementKind::WhileLoop { body, .. }
            | StatementKind::UntilLoop { body, .. }
            | StatementKind::ForEach { body, .. } => {
                validate_statements(body, requirements, policy)?
            }
            StatementKind::TryRecover { body, recovery } => {
                validate_statements(body, requirements, policy)?;
                validate_statements(recovery, requirements, policy)?;
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> &'static str {
        "Rule:\n  id: test.authoring\n  version: 1.0.0\nRespond with record with false as allow and \"Denied.\" as reason\n"
    }
    #[test]
    fn rejects_python_paths_and_broad_process_permissions_in_operator_profile() {
        for requirements in [
            json!({"permissions":[{"kind":"write_file","scope":"output.PY"}],"write_contents":{"output.PY":["print(1)"]}}),
            json!({"permissions":[{"kind":"run_tool","scope":"chmod"}],"write_contents":{}}),
            json!({"permissions":[{"kind":"filesystem"}],"write_contents":{}}),
        ] {
            assert!(validate_requirements(&requirements).is_err());
        }
    }
    #[test]
    fn preserves_case_in_approved_file_permissions() {
        let requirements = json!({"permissions":[{"kind":"write_file","scope":"State.TXT"}],"write_contents":{"State.TXT":["done"]}});
        assert!(validate_draft(&json!({"program":"Permissions:\n  Write files to \"State.TXT\"\nExport \"done\" to \"State.TXT\"","policy":policy()}), &requirements).is_ok());
    }
    #[test]
    fn rejects_code_process_import_and_permission_escalation() {
        let requirements = json!({"permissions":[],"write_contents":{}});
        for program in [
            "import os\nos.chmod('x', 511)",
            "#!/bin/sh\nchmod +x x",
            "Import \"/private/source.dvl\"",
            "If true:\n  Import \"/private/source.dvl\"",
            "Permissions:\n  Run catalog tool \"chmod\"\nRespond with \"ok\"",
            "Run catalog tool request as result",
        ] {
            assert!(
                validate_draft(&json!({"program":program,"policy":policy()}), &requirements)
                    .is_err(),
                "{program}"
            );
        }
        assert!(validate_draft(
            &json!({"program":"Respond with \"ok\"","policy":policy()}),
            &requirements
        )
        .is_ok());
    }
    #[test]
    fn restricts_writes_to_exact_operator_content_and_validates_both_artifacts() {
        let requirements = json!({"permissions":[{"kind":"write_file","scope":"state.txt"}],"write_contents":{"state.txt":["done\n"]}});
        let program = "Permissions:\n  Write files to \"state.txt\"\nExport \"done\\n\" to \"state.txt\"\nRespond with \"ok\"";
        assert!(
            validate_draft(&json!({"program":program,"policy":policy()}), &requirements).is_ok()
        );
        for bad in [
            program.replace("done\\n", "print(1)"),
            program.replace("state.txt", "bad.py"),
            program.replace("\"done\\n\"", "model_output"),
        ] {
            assert!(
                validate_draft(&json!({"program":bad,"policy":policy()}), &requirements).is_err()
            );
        }
        assert!(validate_draft(
            &json!({"program":program,"policy":"print(1)"}),
            &requirements
        )
        .is_err());
        assert!(validate_draft(
            &json!({"program":program,"policy":policy(),"extra":true}),
            &requirements
        )
        .is_err());
    }
}
