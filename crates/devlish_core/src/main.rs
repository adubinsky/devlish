use devlish_core::{compile_source_to_json, parse_iso_date, sha256_hex, CompileOptions};
use devlish_vm::{HostEffects, Vm};
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::io::{Read as IoRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const VERSION: &str = "0.1.0";

fn devlish_search_paths_for(source_path: Option<&Path>) -> Vec<String> {
    let mut paths = Vec::new();
    if let Some(project_root) = source_path
        .and_then(|path| path.parent())
        .and_then(find_devlish_project_root)
    {
        push_existing_path(&mut paths, project_root.clone());
        push_existing_path(&mut paths, project_root.join("devlish"));
        push_existing_path(&mut paths, project_root.join("lib"));
    }
    // DEVLISH_PATH env var (colon-separated, like PATH)
    if let Ok(env_paths) = env::var("DEVLISH_PATH") {
        for p in env_paths.split(':') {
            let trimmed = p.trim();
            if !trimmed.is_empty() {
                paths.push(trimmed.to_string());
            }
        }
    }
    // ~/.devlish/lib/ standard library path
    if let Some(home) = env::var_os("HOME") {
        let lib_path = PathBuf::from(home).join(".devlish").join("lib");
        if lib_path.is_dir() {
            paths.push(lib_path.to_string_lossy().to_string());
        }
    }
    paths
}

fn find_devlish_project_root(start: &Path) -> Option<PathBuf> {
    for candidate in start.ancestors() {
        if candidate.join("devlish.toml").is_file() {
            return Some(candidate.to_path_buf());
        }
    }
    None
}

fn push_existing_path(paths: &mut Vec<String>, path: PathBuf) {
    if path.is_dir() {
        let value = path.to_string_lossy().to_string();
        if !paths.contains(&value) {
            paths.push(value);
        }
    }
}

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    if args.is_empty() {
        print_help();
        return Ok(());
    }

    match args[0].as_str() {
        "--help" | "-h" | "help" => {
            print_help();
            Ok(())
        }
        "--version" | "-v" | "version" => {
            println!("Devlish {VERSION}");
            Ok(())
        }
        "compile" => run_compile(args),
        "run" => run_execute(args),
        "disassemble" => run_disassemble(args),
        "validate" => run_validate(args),
        "lint" => run_lint(args),
        "evidence" => run_evidence(args),
        "audit-verify" => run_audit_verify(args),
        "replay" => run_replay(args),
        "release" => run_release(args),
        "new" => run_new(args),
        "mcp" => run_mcp(args),
        "course" => run_course(args),
        "fmt" | "format" => run_format(args),
        "repl" => run_repl(args),
        other if looks_like_file(other) => {
            // Implicit run: treat bare file argument as `run <file>`
            let mut run_args = vec!["run".to_string()];
            run_args.extend(args);
            run_execute(run_args)
        }
        other => Err(format!("unknown command: {other}\n\n{}", usage())),
    }
}

fn looks_like_file(arg: &str) -> bool {
    arg.ends_with(".dvl") || arg.ends_with(".dvlc") || arg.ends_with(".dvlc.json")
}

fn print_help() {
    println!(
        "Devlish {VERSION} - AI-first programming language

Usage: devlish-core <command> [options]

Commands:
  compile <file.dvl>          Compile a Devlish source file to bytecode
  run <file>                  Run a compiled bytecode file or source file
  disassemble <file.dvlc.json>  Disassemble a bytecode package
  validate <file.dvl>         Validate a source file (alias: lint)
  lint <file.dvl>             Validate a source file (alias: validate)
  evidence <rule.dvl>         Run golden cases and emit a signed evidence report
  audit-verify <log.jsonl>    Verify the hash chain of an audit log
  replay <log.jsonl>          Re-run a journaled governed run offline and verify its output
  release <verb>              Release lifecycle: propose, approve, publish, retire, list, verify
  new <project_name>          Create a new Devlish project
  mcp                         Start MCP server (JSON-RPC over stdio)
  course                      Walk through the interactive beginner course
  fmt <file.dvl>              Format a Devlish source file
  repl                        Interactive read-eval-print loop
  version                     Show version
  help                        Show this help

Options:
  -h, --help                  Show this help
  -v, --version               Show version

Compile options:
  --target bytecode           Compilation target (only bytecode supported)
  --output, -o <path>         Write output to file instead of stdout

Run options:
  --input <json>              Input data as JSON string
  --method <name>             Method to invoke (for class-based programs)
  --env KEY=VALUE             Set a credential/environment variable (repeatable)
  --audit-log <path>          Append governed-run audit records to a JSONL log
                              (falls back to DEVLISH_AUDIT_LOG)
  --journal <dir>             Archive input, bytecode, and every effect exchange
                              as content-addressed attachments (enables replay;
                              requires --audit-log)
  --governed <registry.json>  Refuse to run any artifact that is not a published
                              release in the registry
  --quiet                     Suppress VM debug events on stderr

Implicit run:
  devlish-core <file.dvl>     Equivalent to: devlish-core run <file.dvl>"
    );
}

fn run_compile(args: Vec<String>) -> Result<(), String> {
    let config = CompileConfig::parse(args)?;
    let source = fs::read_to_string(&config.input)
        .map_err(|error| format!("failed to read {}: {error}", config.input.display()))?;
    let source_path = config.input.to_string_lossy().to_string();
    let json = compile_source_to_json(
        &source,
        CompileOptions {
            source_path: Some(source_path),
            search_paths: devlish_search_paths_for(Some(&config.input)),
        },
    )
    .map_err(|error| error.to_string())?;

    if let Some(output) = config.output {
        fs::write(&output, format!("{json}\n"))
            .map_err(|error| format!("failed to write {}: {error}", output.display()))?;
    } else {
        println!("{json}");
    }
    Ok(())
}

/// Loads a package from a `.dvl` source file (compiling it) or a compiled
/// bytecode file.
fn load_package(path: &Path) -> Result<Value, String> {
    if path.extension().is_some_and(|ext| ext == "dvl") {
        let source = fs::read_to_string(path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        let json_str = compile_source_to_json(
            &source,
            CompileOptions {
                source_path: Some(path.to_string_lossy().to_string()),
                search_paths: devlish_search_paths_for(Some(path)),
            },
        )
        .map_err(|error| format!("compile error: {error}"))?;
        serde_json::from_str(&json_str)
            .map_err(|error| format!("internal error: invalid compiled JSON: {error}"))
    } else {
        let bytecode_source = fs::read_to_string(path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        serde_json::from_str(&bytecode_source)
            .map_err(|error| format!("invalid bytecode JSON: {error}"))
    }
}

/// Reads `manifest.rule.effective_from` / `effective_until` from a package.
fn rule_effective_window(package: &Value) -> (Option<String>, Option<String>) {
    let rule = package.get("manifest").and_then(|m| m.get("rule"));
    let field = |name: &str| {
        rule.and_then(|r| r.get(name))
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    (field("effective_from"), field("effective_until"))
}

/// Selects the single package whose effective window contains `as_of` from the
/// given (path, package) pairs, which must all be versions of the same rule id.
/// ISO dates are fixed width, so windows compare lexically.
fn select_effective_version(
    versions: Vec<(PathBuf, Value)>,
    as_of: &str,
) -> Result<(PathBuf, Value), String> {
    // Validate the as-of date with the same rule as the compiler and the
    // runtime, so lexical window comparison can never run against a garbage or
    // impossible date and silently pick the wrong version.
    if parse_iso_date(as_of).is_none() {
        return Err(format!(
            "--as-of date '{as_of}' must be a real YYYY-MM-DD date"
        ));
    }
    let rule_id = |pkg: &Value| {
        pkg.get("manifest")
            .and_then(|m| m.get("rule"))
            .and_then(|r| r.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string)
    };

    let mut ids: Vec<String> = Vec::new();
    for (path, pkg) in &versions {
        match rule_id(pkg) {
            Some(id) => {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            None => {
                return Err(format!(
                    "--as-of needs governed rules (a Rule: section); {} has none",
                    path.display()
                ))
            }
        }
    }
    if ids.len() > 1 {
        return Err(format!(
            "--as-of needs one rule id across all inputs; got: {}",
            ids.join(", ")
        ));
    }

    let mut in_force: Vec<(PathBuf, Value)> = versions
        .into_iter()
        .filter(|(_, pkg)| {
            let (from, until) = rule_effective_window(pkg);
            from.as_deref().is_none_or(|f| as_of >= f)
                && until.as_deref().is_none_or(|u| as_of <= u)
        })
        .collect();

    match in_force.len() {
        0 => Err(format!(
            "no version of {} is in force on {as_of}",
            ids.first().map(String::as_str).unwrap_or("the rule")
        )),
        1 => Ok(in_force.remove(0)),
        _ => Err(format!(
            "multiple versions of {} are in force on {as_of} (overlapping effective windows)",
            ids.first().map(String::as_str).unwrap_or("the rule")
        )),
    }
}

fn run_execute(args: Vec<String>) -> Result<(), String> {
    let config = RunConfig::parse(args)?;
    if config.journal.is_some()
        && config.audit_log.is_none()
        && env::var("DEVLISH_AUDIT_LOG")
            .ok()
            .filter(|v| !v.is_empty())
            .is_none()
    {
        return Err(
            "--journal requires --audit-log (the journal attaches to the audit record)".to_string(),
        );
    }

    let package: Value = if let Some(as_of) = &config.as_of {
        // Gather every candidate version, then pick the one in force on the date.
        let mut versions: Vec<(PathBuf, Value)> = Vec::new();
        for path in std::iter::once(&config.input).chain(config.extra_inputs.iter()) {
            versions.push((path.clone(), load_package(path)?));
        }
        if let Some(registry) = &config.governed {
            for (path, package) in &versions {
                assert_published(registry, package, path)?;
            }
        }
        let (chosen_path, chosen) = select_effective_version(versions, as_of)?;
        if let Some(rule) = chosen.get("manifest").and_then(|m| m.get("rule")) {
            let id = rule.get("id").and_then(Value::as_str).unwrap_or("?");
            let version = rule.get("version").and_then(Value::as_str).unwrap_or("?");
            eprintln!(
                "as-of {as_of}: running {id} v{version} ({})",
                chosen_path.display()
            );
        }
        chosen
    } else {
        let package = load_package(&config.input)?;
        if let Some(registry) = &config.governed {
            assert_published(registry, &package, &config.input)?;
        }
        package
    };

    let mut input: Value = match &config.input_json {
        Some(json_str) => serde_json::from_str(json_str)
            .map_err(|error| format!("invalid --input JSON: {error}"))?,
        None => json!({}),
    };

    // If --method is specified, inject __method__ into the input so the VM
    // (or a future method-aware runner) can dispatch to the right entry point.
    if let Some(method_name) = &config.method {
        if let Some(methods) = package.get("methods").and_then(Value::as_array) {
            let found = methods.iter().find(|m| {
                m.get("ruby_name")
                    .and_then(Value::as_str)
                    .is_some_and(|n| n == method_name.as_str())
                    || m.get("name")
                        .and_then(Value::as_str)
                        .is_some_and(|n| n == method_name.as_str())
            });
            if found.is_none() {
                return Err(format!(
                    "method '{}' not found in class. Available methods: {}",
                    method_name,
                    methods
                        .iter()
                        .filter_map(|m| m.get("name").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        if let Value::Object(ref mut obj) = input {
            obj.insert("__method__".to_string(), Value::String(method_name.clone()));
        }
    }

    let audit_path = config.audit_log.clone().or_else(|| {
        env::var("DEVLISH_AUDIT_LOG")
            .ok()
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    });
    if config.journal.is_some()
        && package
            .get("manifest")
            .and_then(|m| m.get("rule"))
            .is_none()
    {
        return Err(
            "--journal requires a governed rule (a Rule: section); this program has none"
                .to_string(),
        );
    }
    let native = NativeHost {
        credentials: CredentialStore::new(&config.env_overrides, Some(&config.input)),
        audit_log: audit_path.map(AuditLogWriter::new),
    };
    let mut journaling_host;
    let mut plain_host;
    let host: &mut dyn HostEffects = match &config.journal {
        Some(dir) => {
            journaling_host = JournalingHost::new(
                native,
                dir.clone(),
                package.clone(),
                input.clone(),
                !config.quiet,
            );
            &mut journaling_host
        }
        None => {
            plain_host = native;
            &mut plain_host
        }
    };
    let vm = Vm::new(package, input);
    match vm {
        Err(error) => {
            let failure = json!({
                "success": false,
                "error": error.message,
                "context": {},
                "results": {}
            });
            println!(
                "{}",
                serde_json::to_string_pretty(&failure).unwrap_or_default()
            );
            Err(error.message)
        }
        Ok(mut vm) => {
            if config.quiet {
                vm.set_emit_events(false);
            }
            match vm.run(host) {
                Ok(result) => {
                    // If the program used "Respond with", the output was already
                    // written to stdout by host.respond(). Don't dump the full
                    // result envelope.
                    let responded = result
                        .get("responded")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    if !responded {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&result).unwrap_or_default()
                        );
                    }
                    if config.test_mode {
                        let assertions = result
                            .get("results")
                            .and_then(|r| r.get("assertions"))
                            .and_then(Value::as_array);
                        if let Some(assertions) = assertions {
                            let total = assertions.len();
                            let passed = assertions
                                .iter()
                                .filter(|a| a.get("passed").and_then(Value::as_bool) == Some(true))
                                .count();
                            let failed = total - passed;
                            eprintln!("{passed}/{total} assertions passed, {failed} failed");
                            if failed > 0 {
                                return Err(format!("{failed} assertion(s) failed"));
                            }
                        }
                    }
                    Ok(())
                }
                Err(error) => {
                    // If the error message is valid JSON (from Fail with record),
                    // write it to stdout as structured output instead of the
                    // generic failure envelope.
                    if serde_json::from_str::<Value>(&error.message).is_ok() {
                        println!("{}", error.message);
                    } else {
                        let failure = json!({
                            "success": false,
                            "error": error.message,
                            "context": {},
                            "results": {}
                        });
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&failure).unwrap_or_default()
                        );
                    }
                    Err(error.message)
                }
            }
        }
    }
}

/// A golden test case for a rule: an input and the output it must produce.
#[derive(Debug)]
struct EvidenceCase {
    name: String,
    input: Value,
    expected: Value,
}

/// A pure host for evidence runs: responses are captured from the VM result,
/// events are dropped, and any side effect (file/http) fails so evidence only
/// covers deterministic rule evaluation.
struct EvidenceHost;

impl HostEffects for EvidenceHost {
    fn emit_event(&mut self, _event: &Value) {}
    fn write_file(&mut self, _request: &Value) -> Result<(), String> {
        Err("evidence runs are pure; file and network effects are not available".to_string())
    }
    fn respond(&mut self, _value: &Value) -> Result<(), String> {
        Ok(())
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Runs a compiled package with `input` and returns its `Respond with` value.
/// A rule that fails, pauses at a checkpoint, or never responds is an error, so
/// a non-answering rule can never be silently certified against an expected
/// `null`.
fn run_case_capture(package: &Value, input: &Value) -> Result<Value, String> {
    let mut host = EvidenceHost;
    let mut vm = Vm::new(package.clone(), input.clone()).map_err(|error| error.message)?;
    vm.set_emit_events(false);
    let result = vm.run(&mut host).map_err(|error| error.message)?;
    match result.get("response") {
        Some(value) => Ok(value.clone()),
        None => Err(
            "rule did not produce a `Respond with` value (paused or returned nothing)".to_string(),
        ),
    }
}

fn load_evidence_cases(path: &Path) -> Result<Vec<EvidenceCase>, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read cases {}: {error}", path.display()))?;
    let raw: Value = serde_json::from_str(&text)
        .map_err(|error| format!("invalid cases JSON in {}: {error}", path.display()))?;
    let array = raw
        .as_array()
        .ok_or_else(|| format!("cases file {} must be a JSON array", path.display()))?;
    let mut cases = Vec::new();
    let mut seen_names = std::collections::HashSet::new();
    for (index, item) in array.iter().enumerate() {
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("case {}", index + 1));
        if !seen_names.insert(name.clone()) {
            // Unique names keep every entry in a signed bundle identifiable, so
            // a duplicate can't hide a failing case from a human reviewer.
            return Err(format!(
                "duplicate case name '{name}' in {}",
                path.display()
            ));
        }
        let input = item.get("input").cloned().unwrap_or_else(|| json!({}));
        let expected = item
            .get("expected")
            .cloned()
            .ok_or_else(|| format!("case '{name}' is missing an 'expected' value"))?;
        cases.push(EvidenceCase {
            name,
            input,
            expected,
        });
    }
    if cases.is_empty() {
        return Err(format!(
            "cases file {} has no cases; evidence must certify at least one",
            path.display()
        ));
    }
    Ok(cases)
}

/// Verifies an evidence report's `report_sha256`: remove that field,
/// canonically re-serialize (sorted keys, compact), and recompute.
fn verify_evidence_report(path: &Path) -> Result<(), String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let mut report: Value =
        serde_json::from_str(&text).map_err(|error| format!("invalid report JSON: {error}"))?;
    let claimed = report
        .get("report_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| "report has no report_sha256 field".to_string())?
        .to_string();
    report
        .as_object_mut()
        .ok_or_else(|| "report must be a JSON object".to_string())?
        .remove("report_sha256");
    let actual = sha256_hex(&serde_json::to_vec(&report).unwrap_or_default());
    if actual == claimed {
        println!("evidence report OK: report_sha256 matches ({actual})");
        Ok(())
    } else {
        Err(format!(
            "evidence report TAMPERED: report_sha256 is {claimed} but recomputes to {actual}"
        ))
    }
}

/// Builds the evidence report and stamps it with a sha256 of its own body (that
/// hash covers everything except `report_sha256` itself, so a verifier removes
/// the field, re-serializes, and recomputes it).
fn build_evidence_report(
    rule_id: &str,
    rule_version: &str,
    artifact_sha256: &str,
    compiler_version: Option<&str>,
    cases: Vec<Value>,
    generated_at: u64,
) -> Value {
    let total = cases.len();
    let passed = cases
        .iter()
        .filter(|c| c.get("passed").and_then(Value::as_bool) == Some(true))
        .count();
    let mut report = json!({
        "format": "devlish-evidence",
        "format_version": 0,
        "rule": { "id": rule_id, "version": rule_version },
        "artifact_sha256": artifact_sha256,
        "compiler_version": compiler_version,
        "generated_at": generated_at,
        "cases": cases,
        "totals": { "total": total, "passed": passed, "failed": total - passed },
    });
    // BTreeMap-backed serde_json keys are sorted, so this serialization is
    // canonical and a verifier can reproduce the hash.
    let body = serde_json::to_vec(&report).unwrap_or_default();
    report["report_sha256"] = json!(sha256_hex(&body));
    report
}

fn run_evidence(args: Vec<String>) -> Result<(), String> {
    let mut rule_path: Option<PathBuf> = None;
    let mut cases_path: Option<PathBuf> = None;
    let mut output_path: Option<PathBuf> = None;
    let mut verify_path: Option<PathBuf> = None;
    let mut index = 1usize;
    while index < args.len() {
        match args[index].as_str() {
            "--cases" => {
                index += 1;
                cases_path = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--cases requires a path".to_string())?,
                ));
            }
            "--output" => {
                index += 1;
                output_path = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--output requires a path".to_string())?,
                ));
            }
            "--verify" => {
                index += 1;
                verify_path =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--verify requires a report path".to_string()
                    })?));
            }
            value if value.starts_with('-') => return Err(format!("unknown option: {value}")),
            value => {
                if rule_path.is_some() {
                    return Err(format!("unexpected extra argument: {value}"));
                }
                rule_path = Some(PathBuf::from(value));
            }
        }
        index += 1;
    }

    if let Some(report_path) = verify_path {
        return verify_evidence_report(&report_path);
    }

    let rule_path = rule_path.ok_or_else(|| {
        "Usage: devlish evidence <rule.dvl> [--cases file.json] [--output evidence.json] | --verify <report.json>"
            .to_string()
    })?;

    let package = load_package(&rule_path)?;
    // Hash the canonical (sorted-keys pretty) serialization of the parsed
    // package -- the same form VM audit records hash, so evidence and audit
    // agree on artifact identity regardless of bytecode file formatting.
    let bytecode_json = serde_json::to_string_pretty(&package).map_err(|e| e.to_string())?;
    let artifact_sha256 = sha256_hex(bytecode_json.as_bytes());

    let rule = package.get("manifest").and_then(|m| m.get("rule"));
    let rule_id = rule
        .and_then(|r| r.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "evidence requires a governed rule (a Rule: section with id and version)".to_string()
        })?;
    let rule_version = rule
        .and_then(|r| r.get("version"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "evidence requires the rule's Rule: section to declare a version".to_string()
        })?;
    let compiler_version = package.get("compiler_version").and_then(Value::as_str);

    let cases_path = cases_path.unwrap_or_else(|| {
        let mut p = rule_path.clone();
        let stem = rule_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("rule");
        p.set_file_name(format!("{stem}.cases.json"));
        p
    });
    let cases = load_evidence_cases(&cases_path)?;

    let mut case_reports = Vec::new();
    let mut failed = 0usize;
    for case in &cases {
        let actual = run_case_capture(&package, &case.input);
        let (passed, actual_value, error) = match actual {
            Ok(value) => (value == case.expected, value, None),
            Err(message) => (false, Value::Null, Some(message)),
        };
        if !passed {
            failed += 1;
        }
        let input_bytes = serde_json::to_vec(&case.input).unwrap_or_default();
        let output_bytes = serde_json::to_vec(&actual_value).unwrap_or_default();
        let expected_bytes = serde_json::to_vec(&case.expected).unwrap_or_default();
        let mut entry = json!({
            "name": case.name,
            "passed": passed,
            "input_sha256": sha256_hex(&input_bytes),
            "output_sha256": sha256_hex(&output_bytes),
            "expected_sha256": sha256_hex(&expected_bytes),
        });
        if let Some(message) = error {
            entry["error"] = json!(message);
        }
        case_reports.push(entry);
    }

    let report = build_evidence_report(
        rule_id,
        rule_version,
        &artifact_sha256,
        compiler_version,
        case_reports,
        now_unix(),
    );
    let report_json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;

    if let Some(output) = &output_path {
        fs::write(output, format!("{report_json}\n"))
            .map_err(|error| format!("failed to write {}: {error}", output.display()))?;
        eprintln!(
            "evidence: {}/{} cases passed for {rule_id} v{rule_version} -> {}",
            cases.len() - failed,
            cases.len(),
            output.display()
        );
    } else {
        println!("{report_json}");
    }

    if failed > 0 {
        return Err(format!("{failed} of {} evidence cases failed", cases.len()));
    }
    Ok(())
}

fn run_disassemble(args: Vec<String>) -> Result<(), String> {
    let file = args
        .get(1)
        .ok_or_else(|| "Usage: devlish-core disassemble <file.dvlc.json>".to_string())?;

    let source =
        fs::read_to_string(file).map_err(|error| format!("failed to read {file}: {error}"))?;
    let package: Value =
        serde_json::from_str(&source).map_err(|error| format!("invalid bytecode JSON: {error}"))?;

    // Print header
    if let Some(path) = package.get("source_path").and_then(Value::as_str) {
        println!("Source: {path}");
    }
    if let Some(hash) = package.get("source_hash").and_then(Value::as_str) {
        println!("Hash:   {hash}");
    }
    println!();

    let instructions = package
        .get("instructions")
        .and_then(Value::as_array)
        .ok_or_else(|| "bytecode package missing 'instructions' array".to_string())?;

    let constant_pool = package.get("constant_pool").and_then(Value::as_array);

    let source_map = package.get("source_map").and_then(Value::as_array);

    // Build address-to-line lookup from source_map
    let line_for_address = |addr: usize| -> Option<i64> {
        source_map?.iter().find_map(|entry| {
            let obj = entry.as_object()?;
            let a = obj.get("address")?.as_i64()?;
            if a as usize == addr {
                obj.get("line").and_then(Value::as_i64)
            } else {
                None
            }
        })
    };

    // Print class/method info if present
    if let Some(methods) = package.get("methods").and_then(Value::as_array) {
        if let Some(class_info) = package.get("class_info").and_then(Value::as_object) {
            if let Some(name) = class_info.get("name").and_then(Value::as_str) {
                println!("Class: {name}");
            }
        }
        for method in methods {
            if let Some(obj) = method.as_object() {
                let name = obj.get("name").and_then(Value::as_str).unwrap_or("?");
                let entry = obj.get("entry_point").and_then(Value::as_i64).unwrap_or(0);
                println!("  Method {name} @ {entry}");
            }
        }
        println!();
    }

    for (addr, instruction) in instructions.iter().enumerate() {
        let obj = match instruction.as_object() {
            Some(o) => o,
            None => continue,
        };

        let op = obj.get("op").and_then(Value::as_str).unwrap_or("???");

        // Collect operand fields (everything except "op")
        let mut operands = Vec::new();
        for (key, val) in obj {
            if key == "op" {
                continue;
            }
            let val_str = match val {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                Value::Null => "null".to_string(),
                _ => serde_json::to_string(val).unwrap_or_default(),
            };

            // For const operands, show the constant value
            if key == "const" || key.ends_with("_const") {
                if let Some(idx) = val.as_i64() {
                    if let Some(pool) = constant_pool {
                        if let Some(constant) = pool.get(idx as usize) {
                            let display = format_constant(constant);
                            operands.push(format!("{key}={val_str} ({display})"));
                            continue;
                        }
                    }
                }
            }

            operands.push(format!("{key}={val_str}"));
        }

        let operands_str = operands.join(" ");
        let line_comment = match line_for_address(addr) {
            Some(line) => format!(" ; line {line}"),
            None => String::new(),
        };

        println!("{addr:04} {op:<14} {operands_str}{line_comment}");
    }

    Ok(())
}

fn format_constant(value: &Value) -> String {
    match value {
        Value::String(s) => format!("\"{s}\""),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        _ => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn run_validate(args: Vec<String>) -> Result<(), String> {
    let file = args
        .get(1)
        .ok_or_else(|| "Usage: devlish-core validate <file.dvl>".to_string())?;

    let source =
        fs::read_to_string(file).map_err(|error| format!("failed to read {file}: {error}"))?;
    let source_path = file.clone();

    match compile_source_to_json(
        &source,
        CompileOptions {
            source_path: Some(source_path),
            search_paths: devlish_search_paths_for(Some(Path::new(file))),
        },
    ) {
        Ok(_) => {
            println!("Valid");
            Ok(())
        }
        Err(error) => {
            eprintln!("Invalid: {error}");
            Err(format!("Invalid: {error}"))
        }
    }
}

fn run_lint(args: Vec<String>) -> Result<(), String> {
    let file = args
        .get(1)
        .ok_or_else(|| "Usage: devlish-core lint <file.dvl> [--json]".to_string())?;
    let json_output = args.iter().any(|a| a == "--json");

    let source =
        fs::read_to_string(file).map_err(|error| format!("failed to read {file}: {error}"))?;
    let source_path = file.clone();

    match compile_source_to_json(
        &source,
        CompileOptions {
            source_path: Some(source_path.clone()),
            search_paths: devlish_search_paths_for(Some(Path::new(file))),
        },
    ) {
        Ok(_) => {
            // Compilation succeeded; surface non-fatal lint findings (e.g.
            // identifiers referenced before they are bound). These are warnings,
            // not errors, so the file is still reported as valid.
            //
            // NOTE: this re-parses the source (once to compile above, once to
            // lint here). The double parse is accepted for now; the lint pass
            // can theoretically fail where the compile pass succeeded (e.g. an
            // import removed from disk between the two reads), so surface that
            // to stderr rather than silently dropping it. The file still reports
            // as valid because compilation itself succeeded.
            let warnings = match devlish_core::lint_source(
                &source,
                CompileOptions {
                    source_path: Some(source_path.clone()),
                    search_paths: devlish_search_paths_for(Some(Path::new(file))),
                },
            ) {
                Ok(warnings) => warnings,
                Err(error) => {
                    eprintln!(
                        "Note: lint pass could not run ({}); reporting compile result only.",
                        error
                            .diagnostics
                            .first()
                            .map(|d| d.message.clone())
                            .unwrap_or_else(|| "unknown error".to_string())
                    );
                    Vec::new()
                }
            };
            if json_output {
                let diagnostics: Vec<Value> = warnings
                    .iter()
                    .map(|w| {
                        json!({
                            "line": w.line,
                            "severity": "warning",
                            "message": w.message,
                            "source_text": w.source_text
                        })
                    })
                    .collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "file": source_path,
                        "valid": true,
                        "diagnostics": diagnostics
                    }))
                    .unwrap_or_default()
                );
            } else {
                for warning in &warnings {
                    eprintln!(
                        "Warning: line {}: {}",
                        warning.line, warning.message
                    );
                }
                println!("Valid");
            }
            Ok(())
        }
        Err(error) => {
            if json_output {
                let diagnostics: Vec<Value> = error
                    .diagnostics
                    .iter()
                    .map(|d| {
                        json!({
                            "line": d.line,
                            "severity": "error",
                            "message": d.message,
                            "source_text": d.source_text
                        })
                    })
                    .collect();
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "file": source_path,
                        "valid": false,
                        "diagnostics": diagnostics
                    }))
                    .unwrap_or_default()
                );
            } else {
                eprintln!("Invalid: {error}");
            }
            Err(format!("Invalid: {error}"))
        }
    }
}

fn run_new(args: Vec<String>) -> Result<(), String> {
    let project_name = args
        .get(1)
        .ok_or_else(|| "Usage: devlish-core new <project_name>".to_string())?;

    // Validate project name
    if project_name.is_empty() {
        return Err("Project name cannot be empty".to_string());
    }
    if !project_name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err(
            "Project name can only contain letters, numbers, hyphens, and underscores".to_string(),
        );
    }

    let project_path = PathBuf::from(project_name);
    if project_path.exists() {
        return Err(format!("Directory '{project_name}' already exists"));
    }

    // Create directory structure
    fs::create_dir_all(project_path.join("examples"))
        .map_err(|e| format!("failed to create directories: {e}"))?;
    fs::create_dir_all(project_path.join("lib"))
        .map_err(|e| format!("failed to create directories: {e}"))?;
    fs::create_dir_all(project_path.join("documents"))
        .map_err(|e| format!("failed to create directories: {e}"))?;

    // documents/.gitkeep
    fs::write(project_path.join("documents/.gitkeep"), "")
        .map_err(|e| format!("failed to write .gitkeep: {e}"))?;

    // lib/.gitkeep
    fs::write(project_path.join("lib/.gitkeep"), "")
        .map_err(|e| format!("failed to write .gitkeep: {e}"))?;

    // Capitalize first letter for display
    let capitalized = capitalize(project_name);

    // main.dvl
    let main_dvl = format!(
        "# {capitalized} - Main Devlish Program
#
# This is the main entry point for your Devlish validation logic.
# Replace this template with your actual validation rules.

Load document

# Example validation rules:
# Document must contain terms and conditions
# Document must contain privacy policy

# Example data extraction:
# Find effective date and save as effective_date
# Find company name and save as company_name

# Example validation logic:
# effective_date must be after 2020-01-01
"
    );
    fs::write(project_path.join("main.dvl"), main_dvl)
        .map_err(|e| format!("failed to write main.dvl: {e}"))?;

    // devlish.toml
    let manifest = format!("name = \"{project_name}\"\nversion = \"0.1.0\"\n");
    fs::write(project_path.join("devlish.toml"), manifest)
        .map_err(|e| format!("failed to write devlish.toml: {e}"))?;

    // examples/sample.dvl
    let sample_dvl = "# Sample Contract Validation
# This example shows common validation patterns

Load document

# Check for required clauses
Document must contain liability clause
Document must contain termination clause
Document should have indemnification clause

# Extract key information
Find contract value and save as contract_value
Find liability cap and save as liability_cap
Find effective date and save as effective_date

# Validate extracted values
contract_value must be at least 10000
liability_cap must be at least 1000000
effective_date must be after 2024-01-01
";
    fs::write(project_path.join("examples/sample.dvl"), sample_dvl)
        .map_err(|e| format!("failed to write sample.dvl: {e}"))?;

    // .gitignore
    let gitignore = "# Environment files
.env
.env.local

# Compiled bytecode
*.dvlc.json

# IDE
.idea/
.vscode/
*.swp
*.swo
*~

# OS
.DS_Store
Thumbs.db

# Logs
*.log

# Temporary files
tmp/
temp/
";
    fs::write(project_path.join(".gitignore"), gitignore)
        .map_err(|e| format!("failed to write .gitignore: {e}"))?;

    // README.md
    let readme = format!(
        "# {capitalized}

A Devlish validation project for deterministic document processing.

## Getting Started

Run the main validation script:

```bash
devlish-core run main.dvl
```

Try the example validation:

```bash
devlish-core run examples/sample.dvl
```

## Project Structure

```text
{project_name}/
  main.dvl              # Main validation script
  devlish.toml          # Project manifest and import boundary
  README.md             # This file
  lib/                  # Shared rules imported by workflows
  examples/             # Example validation scripts
    sample.dvl          # Sample validation
  documents/            # Place your documents here
```

## Commands

- `devlish-core compile <file>` - Compile to bytecode
- `devlish-core run <file>` - Run a Devlish script
- `devlish-core validate <file>` - Validate syntax
- `devlish-core disassemble <file>` - Inspect bytecode
- `devlish-core help` - Show all commands
"
    );
    fs::write(project_path.join("README.md"), readme)
        .map_err(|e| format!("failed to write README.md: {e}"))?;

    println!("Created Devlish project: {project_name}");
    println!();
    println!("Next steps:");
    println!("  cd {project_name}");
    println!("  devlish-core run main.dvl");

    Ok(())
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let upper: String = first.to_uppercase().collect();
            format!("{upper}{}", chars.as_str())
        }
    }
}

/// Credential resolution chain: CLI --env > program-local .env > ~/.devlish/.env > system env.
/// Credentials flow only to host methods, never to program variables.
struct CredentialStore {
    entries: Vec<(String, String)>,
}

impl CredentialStore {
    fn new(cli_env: &[(String, String)], source_path: Option<&Path>) -> Self {
        let mut entries = Vec::new();

        // Lowest priority: global ~/.devlish/.env
        if let Some(home) = env::var_os("HOME") {
            let global_env = PathBuf::from(home).join(".devlish").join(".env");
            if let Ok(content) = fs::read_to_string(&global_env) {
                parse_dotenv(&content, &mut entries);
            }
        }

        // Medium priority: program-local .env (same directory as the .dvl file)
        if let Some(path) = source_path.and_then(|p| p.parent()) {
            let local_env = path.join(".env");
            if let Ok(content) = fs::read_to_string(&local_env) {
                parse_dotenv(&content, &mut entries);
            }
        }

        // Highest priority: CLI --env overrides (applied last so they win)
        for (key, value) in cli_env {
            entries.push((key.clone(), value.clone()));
        }

        Self { entries }
    }

    fn resolve(&self, key: &str) -> Option<String> {
        // Walk entries in reverse so later (higher priority) entries win
        for (k, v) in self.entries.iter().rev() {
            if k == key {
                return Some(v.clone());
            }
        }
        // Fall back to system environment
        env::var(key).ok()
    }
}

fn parse_dotenv(content: &str, entries: &mut Vec<(String, String)>) {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = trimmed.split_once('=') {
            let key = key.trim().to_string();
            let mut value = value.trim().to_string();
            // Strip surrounding quotes
            if (value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\''))
            {
                value = value[1..value.len() - 1].to_string();
            }
            if !key.is_empty() {
                entries.push((key, value));
            }
        }
    }
}

struct NativeHost {
    credentials: CredentialStore,
    /// Present when `--audit-log` / `DEVLISH_AUDIT_LOG` is set: governed
    /// runs append hash-chained provenance records to this log.
    audit_log: Option<AuditLogWriter>,
}

/// Appends hash-chained audit records to a JSONL log. Every line carries the
/// sha256 of the previous line, so `devlish audit-verify` detects a modified
/// or deleted record anywhere in the chain.
struct AuditLogWriter {
    path: PathBuf,
}

impl AuditLogWriter {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn append(&mut self, record: &Value) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|error| {
                    format!("failed to create directory {}: {error}", parent.display())
                })?;
            }
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| {
                format!("failed to open audit log {}: {error}", self.path.display())
            })?;
        // Exclusive lock for the whole read-tail-then-append critical
        // section, so concurrent runs serialize instead of forking the
        // chain (two records claiming the same prev_sha256).
        lock_exclusive(&file, &self.path)?;

        let mut content = String::new();
        file.read_to_string(&mut content).map_err(|error| {
            format!("failed to read audit log {}: {error}", self.path.display())
        })?;
        let prev = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .next_back()
            .map(|line| sha256_hex(line.as_bytes()));

        let mut full = record
            .as_object()
            .cloned()
            .ok_or_else(|| "audit record must be an object".to_string())?;
        full.insert(
            "prev_sha256".to_string(),
            prev.map(Value::String).unwrap_or(Value::Null),
        );
        full.insert("timestamp".to_string(), json!(now_unix()));
        full.insert(
            "runtime".to_string(),
            json!({ "kind": "native", "version": VERSION }),
        );
        // serde_json maps are sorted, so the line is canonical: re-serializing
        // the parsed record reproduces the exact bytes the chain hashes.
        let mut line = serde_json::to_string(&Value::Object(full))
            .map_err(|error| format!("failed to serialize audit record: {error}"))?;
        line.push('\n');
        // One write_all for line + newline: interleaved partial lines from a
        // torn two-part write would poison the log for every future verify.
        file.write_all(line.as_bytes()).map_err(|error| {
            format!(
                "failed to append audit log {}: {error}",
                self.path.display()
            )
        })
        // The advisory lock releases when `file` drops.
    }
}

#[cfg(unix)]
fn lock_exclusive(file: &fs::File, path: &Path) -> Result<(), String> {
    use std::os::unix::io::AsRawFd;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(format!(
            "failed to lock audit log {}: {}",
            path.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn lock_exclusive(_file: &fs::File, _path: &Path) -> Result<(), String> {
    // No advisory locking on this platform; concurrent writers may fork
    // the chain. Single-writer usage is documented in docs/AUDIT.md.
    Ok(())
}

fn run_audit_verify(args: Vec<String>) -> Result<(), String> {
    let path = args
        .get(1)
        .ok_or_else(|| "Usage: devlish-core audit-verify <log.jsonl>".to_string())?;
    let content =
        fs::read_to_string(path).map_err(|error| format!("failed to read {path}: {error}"))?;
    let count = verify_audit_log(&content)?;
    println!("audit log OK: {count} record(s), hash chain intact");
    // The chain cannot see edits at or after its last record; printing the
    // tail hash lets an operator anchor it externally (see docs/AUDIT.md).
    if let Some(last) = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .next_back()
    {
        println!("latest record sha256: {}", sha256_hex(last.as_bytes()));
    }
    Ok(())
}

/// Walks an audit log's hash chain. Each record's `prev_sha256` must equal the
/// sha256 of the previous line's exact bytes (null for the first record), so
/// any modified, reordered, or deleted interior record breaks the chain at the
/// first line after the tamper point.
fn verify_audit_log(content: &str) -> Result<usize, String> {
    let mut prev: Option<String> = None;
    let mut count = 0usize;
    for (index, line) in content.lines().enumerate() {
        let line_number = index + 1;
        if line.trim().is_empty() {
            continue;
        }
        let record: Value = serde_json::from_str(line)
            .map_err(|error| format!("line {line_number}: invalid JSON: {error}"))?;
        let claimed = match record.get("prev_sha256") {
            Some(Value::Null) => None,
            Some(Value::String(hash)) => Some(hash.clone()),
            _ => {
                return Err(format!(
                    "line {line_number}: record has no prev_sha256 field"
                ))
            }
        };
        if claimed != prev {
            return Err(format!(
                "audit chain broken at line {line_number}: prev_sha256 is {}, expected {} \
                 (a record before this line was modified or removed)",
                claimed.as_deref().unwrap_or("null"),
                prev.as_deref().unwrap_or("null"),
            ));
        }
        prev = Some(sha256_hex(line.as_bytes()));
        count += 1;
    }
    Ok(count)
}

/// Wraps a host and records every effect exchange, so a governed run can be
/// re-executed later against the journaled responses instead of the live
/// world (`devlish replay`). The finished journal is written as a
/// content-addressed attachment and linked from the audit record via
/// `journal_sha256`. Credentials are never journaled: they are resolved
/// inside the inner host, below this boundary.
struct JournalingHost<H: HostEffects> {
    inner: H,
    dir: PathBuf,
    /// Parsed bytecode package; archived so replay runs the exact artifact.
    bytecode: Value,
    /// The full run input; archived, not just hashed.
    input: Value,
    /// Event emission changes the result envelope, so replay must reproduce it.
    emit_events: bool,
    effects: Vec<Value>,
}

impl<H: HostEffects> JournalingHost<H> {
    fn new(inner: H, dir: PathBuf, bytecode: Value, input: Value, emit_events: bool) -> Self {
        Self {
            inner,
            dir,
            bytecode,
            input,
            emit_events,
            effects: Vec::new(),
        }
    }

    fn journal_value(&mut self, kind: &str, request: Value, result: &Result<Value, String>) {
        let response = match result {
            Ok(value) => json!({ "ok": value }),
            Err(error) => json!({ "err": error }),
        };
        self.effects
            .push(json!({ "kind": kind, "request": request, "response": response }));
    }

    fn journal_unit(&mut self, kind: &str, request: Value, result: &Result<(), String>) {
        let mapped = result.clone().map(|()| Value::Null);
        self.journal_value(kind, request, &mapped);
    }
}

impl<H: HostEffects> HostEffects for JournalingHost<H> {
    fn emit_event(&mut self, event: &Value) {
        // Events are produced by the VM deterministically; replay regenerates
        // them, so they are not part of the journal.
        self.inner.emit_event(event);
    }

    fn resolve_credential(&self, key: &str) -> Option<String> {
        // Never journaled: secrets must not end up in replay attachments.
        self.inner.resolve_credential(key)
    }

    fn write_file(&mut self, request: &Value) -> Result<(), String> {
        let result = self.inner.write_file(request);
        self.journal_unit("write_file", request.clone(), &result);
        result
    }

    fn read_file(&mut self, request: &Value) -> Result<Value, String> {
        let result = self.inner.read_file(request);
        self.journal_value("read_file", request.clone(), &result);
        result
    }

    fn call_service(&mut self, request: &Value) -> Result<Value, String> {
        let result = self.inner.call_service(request);
        self.journal_value("call_service", request.clone(), &result);
        result
    }

    fn http_request(
        &mut self,
        method: &str,
        url: &str,
        body: &Value,
        headers: &Value,
    ) -> Result<Value, String> {
        let result = self.inner.http_request(method, url, body, headers);
        // The journaled request is what the program asked for. Auth headers
        // are injected by the inner host below this boundary, so they never
        // reach the journal.
        let request = json!({ "method": method, "url": url, "body": body, "headers": headers });
        self.journal_value("http_request", request, &result);
        result
    }

    fn respond(&mut self, value: &Value) -> Result<(), String> {
        let result = self.inner.respond(value);
        self.journal_unit("respond", value.clone(), &result);
        result
    }

    fn http_download(&mut self, url: &str, path: &str) -> Result<(), String> {
        let result = self.inner.http_download(url, path);
        self.journal_unit(
            "http_download",
            json!({ "url": url, "path": path }),
            &result,
        );
        result
    }

    fn read_xlsx_rows(&mut self, path: &str, sheet: Option<&str>) -> Result<Value, String> {
        let result = self.inner.read_xlsx_rows(path, sheet);
        self.journal_value(
            "read_xlsx_rows",
            json!({ "path": path, "sheet": sheet }),
            &result,
        );
        result
    }

    fn file_copy(&mut self, source: &str, destination: &str) -> Result<(), String> {
        let result = self.inner.file_copy(source, destination);
        self.journal_unit(
            "file_copy",
            json!({ "source": source, "destination": destination }),
            &result,
        );
        result
    }

    fn file_move(&mut self, source: &str, destination: &str) -> Result<(), String> {
        let result = self.inner.file_move(source, destination);
        self.journal_unit(
            "file_move",
            json!({ "source": source, "destination": destination }),
            &result,
        );
        result
    }

    fn file_mkdir(&mut self, path: &str) -> Result<(), String> {
        let result = self.inner.file_mkdir(path);
        self.journal_unit("file_mkdir", json!({ "path": path }), &result);
        result
    }

    fn file_delete(&mut self, path: &str) -> Result<(), String> {
        let result = self.inner.file_delete(path);
        self.journal_unit("file_delete", json!({ "path": path }), &result);
        result
    }

    fn file_exists(&mut self, path: &str) -> Result<bool, String> {
        let result = self.inner.file_exists(path);
        let mapped = result.clone().map(Value::Bool);
        self.journal_value("file_exists", json!({ "path": path }), &mapped);
        result
    }

    fn file_stat(&mut self, path: &str) -> Result<Value, String> {
        let result = self.inner.file_stat(path);
        self.journal_value("file_stat", json!({ "path": path }), &result);
        result
    }

    fn file_list(&mut self, path: &str) -> Result<Value, String> {
        let result = self.inner.file_list(path);
        self.journal_value("file_list", json!({ "path": path }), &result);
        result
    }

    fn file_glob(&mut self, pattern: &str, directory: &str) -> Result<Value, String> {
        let result = self.inner.file_glob(pattern, directory);
        self.journal_value(
            "file_glob",
            json!({ "pattern": pattern, "directory": directory }),
            &result,
        );
        result
    }

    fn audit_record(&mut self, record: &Value) -> Result<(), String> {
        // The run is complete: finalize the journal as a content-addressed
        // attachment and link it from the record before it is persisted.
        let journal = json!({
            "format": "devlish-journal",
            "format_version": 0,
            "bytecode": self.bytecode,
            "input": self.input,
            "emit_events": self.emit_events,
            "effects": self.effects,
        });
        let bytes = serde_json::to_vec(&journal)
            .map_err(|error| format!("failed to serialize effect journal: {error}"))?;
        let hash = sha256_hex(&bytes);
        fs::create_dir_all(&self.dir).map_err(|error| {
            format!(
                "failed to create journal directory {}: {error}",
                self.dir.display()
            )
        })?;
        let path = self.dir.join(format!("{hash}.json"));
        fs::write(&path, &bytes)
            .map_err(|error| format!("failed to write journal {}: {error}", path.display()))?;

        let mut full = record
            .as_object()
            .cloned()
            .ok_or_else(|| "audit record must be an object".to_string())?;
        full.insert("journal_sha256".to_string(), json!(hash));
        self.inner.audit_record(&Value::Object(full))
    }
}

/// Replays a journaled run: every effect request must match the journal in
/// kind, shape, and order, and receives the journaled response instead of
/// touching the live world. Any divergence fails the run -- which is the
/// point: after DEVL-122, a replay mismatch is itself evidence.
struct ReplayHost {
    effects: Vec<Value>,
    position: usize,
    /// First divergence, poisoned permanently: a rule's own Try/Otherwise
    /// can swallow the effect error, so the outcome check cannot be the
    /// only reporter.
    diverged: Option<String>,
}

impl ReplayHost {
    fn new(effects: Vec<Value>) -> Self {
        Self {
            effects,
            position: 0,
            diverged: None,
        }
    }

    fn diverge(&mut self, message: String) -> String {
        if self.diverged.is_none() {
            self.diverged = Some(message.clone());
        }
        message
    }

    fn next(&mut self, kind: &str, request: &Value) -> Result<Value, String> {
        let Some(entry) = self.effects.get(self.position).cloned() else {
            return Err(self.diverge(format!(
                "replay diverged: run requested a {kind} effect but the journal ended after {} effect(s)",
                self.effects.len()
            )));
        };
        let journal_kind = entry
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let journal_request = entry.get("request").cloned().unwrap_or(Value::Null);
        if journal_kind != kind || &journal_request != request {
            let message = format!(
                "replay diverged at effect #{}: journal recorded {journal_kind} {journal_request}, run requested {kind} {request}",
                self.position + 1
            );
            return Err(self.diverge(message));
        }
        let response = entry.get("response").cloned().unwrap_or(Value::Null);
        self.position += 1;
        if let Some(error) = response.get("err").and_then(Value::as_str) {
            return Err(error.to_string());
        }
        Ok(response.get("ok").cloned().unwrap_or(Value::Null))
    }

    fn next_unit(&mut self, kind: &str, request: &Value) -> Result<(), String> {
        self.next(kind, request).map(|_| ())
    }

    fn fully_consumed(&self) -> Result<(), String> {
        if self.position == self.effects.len() {
            Ok(())
        } else {
            Err(format!(
                "replay diverged: run used {} of {} journaled effect(s)",
                self.position,
                self.effects.len()
            ))
        }
    }
}

impl HostEffects for ReplayHost {
    fn emit_event(&mut self, _event: &Value) {}

    fn resolve_credential(&self, _key: &str) -> Option<String> {
        // Journaled responses stand in for anything credentials unlocked.
        None
    }

    fn audit_record(&mut self, _record: &Value) -> Result<(), String> {
        // A replay verifies history; it must not append to it.
        Ok(())
    }

    fn write_file(&mut self, request: &Value) -> Result<(), String> {
        self.next_unit("write_file", request)
    }

    fn read_file(&mut self, request: &Value) -> Result<Value, String> {
        self.next("read_file", request)
    }

    fn call_service(&mut self, request: &Value) -> Result<Value, String> {
        self.next("call_service", request)
    }

    fn http_request(
        &mut self,
        method: &str,
        url: &str,
        body: &Value,
        headers: &Value,
    ) -> Result<Value, String> {
        let request = json!({ "method": method, "url": url, "body": body, "headers": headers });
        self.next("http_request", &request)
    }

    fn respond(&mut self, value: &Value) -> Result<(), String> {
        self.next_unit("respond", value)
    }

    fn http_download(&mut self, url: &str, path: &str) -> Result<(), String> {
        self.next_unit("http_download", &json!({ "url": url, "path": path }))
    }

    fn read_xlsx_rows(&mut self, path: &str, sheet: Option<&str>) -> Result<Value, String> {
        self.next("read_xlsx_rows", &json!({ "path": path, "sheet": sheet }))
    }

    fn file_copy(&mut self, source: &str, destination: &str) -> Result<(), String> {
        self.next_unit(
            "file_copy",
            &json!({ "source": source, "destination": destination }),
        )
    }

    fn file_move(&mut self, source: &str, destination: &str) -> Result<(), String> {
        self.next_unit(
            "file_move",
            &json!({ "source": source, "destination": destination }),
        )
    }

    fn file_mkdir(&mut self, path: &str) -> Result<(), String> {
        self.next_unit("file_mkdir", &json!({ "path": path }))
    }

    fn file_delete(&mut self, path: &str) -> Result<(), String> {
        self.next_unit("file_delete", &json!({ "path": path }))
    }

    fn file_exists(&mut self, path: &str) -> Result<bool, String> {
        let value = self.next("file_exists", &json!({ "path": path }))?;
        value
            .as_bool()
            .ok_or_else(|| "journaled file_exists response is not a boolean".to_string())
    }

    fn file_stat(&mut self, path: &str) -> Result<Value, String> {
        self.next("file_stat", &json!({ "path": path }))
    }

    fn file_list(&mut self, path: &str) -> Result<Value, String> {
        self.next("file_list", &json!({ "path": path }))
    }

    fn file_glob(&mut self, pattern: &str, directory: &str) -> Result<Value, String> {
        self.next(
            "file_glob",
            &json!({ "pattern": pattern, "directory": directory }),
        )
    }
}

fn run_replay(args: Vec<String>) -> Result<(), String> {
    let mut log_path: Option<PathBuf> = None;
    let mut journal_dir: Option<PathBuf> = None;
    let mut line_number: Option<usize> = None;
    let mut index = 1usize;
    while index < args.len() {
        match args[index].as_str() {
            "--journal" => {
                index += 1;
                journal_dir =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--journal requires a directory path".to_string()
                    })?));
            }
            "--line" => {
                index += 1;
                line_number = Some(
                    args.get(index)
                        .and_then(|value| value.parse().ok())
                        .ok_or_else(|| "--line requires a 1-based record number".to_string())?,
                );
            }
            value if value.starts_with('-') => return Err(format!("unknown option: {value}")),
            value => {
                if log_path.is_some() {
                    return Err(format!("unexpected extra argument: {value}"));
                }
                log_path = Some(PathBuf::from(value));
            }
        }
        index += 1;
    }
    let log_path = log_path.ok_or_else(replay_usage)?;
    let journal_dir = journal_dir
        .unwrap_or_else(|| PathBuf::from(format!("{}.attachments", log_path.to_string_lossy())));

    let content = fs::read_to_string(&log_path)
        .map_err(|error| format!("failed to read {}: {error}", log_path.display()))?;
    // A record is only worth replaying if the log it sits in is intact:
    // verify the hash chain before trusting anything on the chosen line.
    verify_audit_log(&content)?;
    let records: Vec<&str> = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    if records.is_empty() {
        return Err(format!("{} has no audit records", log_path.display()));
    }
    if line_number == Some(0) {
        return Err("--line is 1-based; there is no record 0".to_string());
    }
    let chosen = match line_number {
        Some(number) => *records.get(number.saturating_sub(1)).ok_or_else(|| {
            format!(
                "record {number} not found: {} has {} record(s)",
                log_path.display(),
                records.len()
            )
        })?,
        None => records[records.len() - 1],
    };
    let record: Value =
        serde_json::from_str(chosen).map_err(|error| format!("invalid audit record: {error}"))?;

    let journal_hash = record
        .get("journal_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "audit record has no journal_sha256 -- the run was not journaled \
             (re-run with --journal <dir>)"
                .to_string()
        })?;

    if journal_hash.len() != 64
        || !journal_hash
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(format!(
            "journal_sha256 '{journal_hash}' is not a sha256 hex digest"
        ));
    }
    // Content-addressed integrity: the attachment must hash to its own name.
    let journal_path = journal_dir.join(format!("{journal_hash}.json"));
    let journal_bytes = fs::read(&journal_path)
        .map_err(|error| format!("failed to read journal {}: {error}", journal_path.display()))?;
    let actual_hash = sha256_hex(&journal_bytes);
    if actual_hash != journal_hash {
        return Err(format!(
            "journal {} does not match its content address (sha256 {actual_hash}) -- the attachment was modified",
            journal_path.display()
        ));
    }
    let journal: Value = serde_json::from_slice(&journal_bytes)
        .map_err(|error| format!("invalid journal JSON: {error}"))?;
    if journal.get("format").and_then(Value::as_str) != Some("devlish-journal") {
        return Err("not a devlish effect journal".to_string());
    }

    let bytecode = journal
        .get("bytecode")
        .cloned()
        .ok_or_else(|| "journal is missing the archived bytecode".to_string())?;
    let input = journal
        .get("input")
        .cloned()
        .ok_or_else(|| "journal is missing the archived input".to_string())?;
    let emit_events = journal
        .get("emit_events")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let effects = journal
        .get("effects")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    // The archived artifact and input must be the ones the record attests to.
    let artifact_sha256 = sha256_hex(
        serde_json::to_string_pretty(&bytecode)
            .map_err(|error| error.to_string())?
            .as_bytes(),
    );
    let recorded_artifact = record.get("artifact_sha256").and_then(Value::as_str);
    if recorded_artifact != Some(artifact_sha256.as_str()) {
        return Err(format!(
            "archived bytecode (sha256 {artifact_sha256}) does not match the audit record's artifact_sha256 {}",
            recorded_artifact.unwrap_or("null")
        ));
    }
    let input_sha256 = sha256_hex(&serde_json::to_vec(&input).map_err(|error| error.to_string())?);
    if record.get("input_sha256").and_then(Value::as_str) != Some(input_sha256.as_str()) {
        return Err(format!(
            "archived input (sha256 {input_sha256}) does not match the audit record's input_sha256"
        ));
    }

    let mut host = ReplayHost::new(effects);
    let mut vm = Vm::new(bytecode, input)
        .map_err(|error| format!("archived bytecode failed to load: {}", error.message))?;
    vm.set_emit_events(emit_events);
    let outcome = vm.run(&mut host);
    let executed = vm.executed_instructions();
    let output = match &outcome {
        Ok(value) => value.clone(),
        Err(error) => json!({ "success": false, "error": error.message }),
    };
    if let Some(divergence) = &host.diverged {
        return Err(format!("REPLAY MISMATCH: {divergence}"));
    }

    let output_sha256 =
        sha256_hex(&serde_json::to_vec(&output).map_err(|error| error.to_string())?);
    let recorded_output = record
        .get("output_sha256")
        .and_then(Value::as_str)
        .unwrap_or("");
    if output_sha256 != recorded_output {
        return Err(format!(
            "REPLAY MISMATCH: replayed output sha256 {output_sha256} != recorded {recorded_output}. \
             The recorded result cannot be reproduced from the archived input and effects -- \
             evidence of tampering or a toolchain change."
        ));
    }
    let recorded_count = record
        .get("instruction_count")
        .and_then(Value::as_u64)
        .ok_or_else(|| "audit record has no instruction_count".to_string())?;
    if recorded_count != executed {
        return Err(format!(
            "REPLAY MISMATCH: replay executed {executed} instructions, record says {recorded_count} \
             -- same output, different execution path (toolchain change?)"
        ));
    }
    host.fully_consumed()?;

    let recorded_runtime = record
        .get("runtime")
        .and_then(|r| r.get("version"))
        .and_then(Value::as_str);
    if let Some(version) = recorded_runtime {
        if version != VERSION {
            eprintln!("note: record was produced by runtime v{version}, replaying with v{VERSION}");
        }
    }
    println!(
        "replay OK: {} v{} reproduced output sha256 {output_sha256} ({executed} instructions)",
        record.get("rule_id").and_then(Value::as_str).unwrap_or("?"),
        record
            .get("rule_version")
            .and_then(Value::as_str)
            .unwrap_or("?"),
    );
    Ok(())
}

fn replay_usage() -> String {
    "Usage: devlish-core replay <audit-log.jsonl> [--journal <dir>] [--line N]".to_string()
}

// ---------------------------------------------------------------------------
// Release registry (DEVL-115): the lifecycle that turns an approved artifact
// into the only thing production runs. The registry is an append-only,
// hash-chained event log; per-(rule, version) status is derived by folding
// events, so releases are never edited, only superseded.
// ---------------------------------------------------------------------------

/// Like `load_registry`, but a missing file is an error. Every verb except
/// `propose` (which bootstraps a new registry) uses this, so a typo'd path
/// can never "verify" an absent registry as OK.
fn load_registry_required(path: &Path) -> Result<Vec<Value>, String> {
    if !path.exists() {
        return Err(format!("registry {} does not exist", path.display()));
    }
    load_registry(path)
}

fn load_registry(path: &Path) -> Result<Vec<Value>, String> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("failed to read {}: {error}", path.display())),
    };
    let registry: Value = serde_json::from_str(&content)
        .map_err(|error| format!("invalid registry JSON in {}: {error}", path.display()))?;
    if registry.get("format").and_then(Value::as_str) != Some("devlish-registry") {
        return Err(format!(
            "{} is not a devlish release registry",
            path.display()
        ));
    }
    let events = registry
        .get("events")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    verify_registry_chain(&events)?;
    Ok(events)
}

fn save_registry(path: &Path, events: &[Value]) -> Result<(), String> {
    let registry = json!({
        "format": "devlish-registry",
        "format_version": 0,
        "events": events,
    });
    let body = serde_json::to_string_pretty(&registry).map_err(|error| error.to_string())?;
    // Write-then-rename so a crash cannot leave a torn registry behind.
    // The temp name includes the pid so concurrent writers do not clobber
    // each other's temp file (last rename still wins; see docs/RELEASE.md).
    let tmp = path.with_extension(format!("json.tmp.{}", std::process::id()));
    fs::write(&tmp, format!("{body}\n"))
        .map_err(|error| format!("failed to write {}: {error}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|error| format!("failed to replace {}: {error}", path.display()))
}

/// Each event carries the sha256 of the previous event's canonical bytes, so
/// editing or removing any interior event breaks the chain.
fn verify_registry_chain(events: &[Value]) -> Result<(), String> {
    let mut prev: Option<String> = None;
    for (index, event) in events.iter().enumerate() {
        let claimed = match event.get("prev_sha256") {
            Some(Value::Null) => None,
            Some(Value::String(hash)) => Some(hash.clone()),
            _ => return Err(format!("registry event #{} has no prev_sha256", index + 1)),
        };
        if claimed != prev {
            return Err(format!(
                "registry chain broken at event #{}: an earlier event was modified or removed",
                index + 1
            ));
        }
        prev = Some(event_sha256(event)?);
    }
    Ok(())
}

fn event_sha256(event: &Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(event).map_err(|error| error.to_string())?;
    Ok(sha256_hex(&bytes))
}

/// Appends an event, stamping the chain link and timestamp.
fn append_registry_event(
    events: &mut Vec<Value>,
    mut event: serde_json::Map<String, Value>,
) -> Result<(), String> {
    let prev = match events.last() {
        Some(last) => Value::String(event_sha256(last)?),
        None => Value::Null,
    };
    event.insert("prev_sha256".to_string(), prev);
    event.insert("timestamp".to_string(), json!(now_unix()));
    events.push(Value::Object(event));
    Ok(())
}

/// Latest lifecycle action per (rule_id, version), in first-seen order.
fn release_states(events: &[Value]) -> Vec<(String, String, String)> {
    let mut order: Vec<(String, String)> = Vec::new();
    let mut latest: std::collections::BTreeMap<(String, String), String> =
        std::collections::BTreeMap::new();
    for event in events {
        let rule_id = event.get("rule_id").and_then(Value::as_str).unwrap_or("");
        let version = event.get("version").and_then(Value::as_str).unwrap_or("");
        let action = event.get("action").and_then(Value::as_str).unwrap_or("");
        let key = (rule_id.to_string(), version.to_string());
        if !order.contains(&key) {
            order.push(key.clone());
        }
        let status = match action {
            "propose" => "draft",
            "approve" => "approved",
            "publish" => "published",
            "retire" => "retired",
            other => other,
        };
        latest.insert(key, status.to_string());
    }
    order
        .into_iter()
        .map(|(rule_id, version)| {
            let status = latest
                .get(&(rule_id.clone(), version.clone()))
                .cloned()
                .unwrap_or_default();
            (rule_id, version, status)
        })
        .collect()
}

/// The propose event for a (rule_id, version): the authoritative record of
/// its artifact hash, evidence hash, author, and effective window.
fn propose_event<'a>(events: &'a [Value], rule_id: &str, version: &str) -> Option<&'a Value> {
    events.iter().find(|event| {
        event.get("action").and_then(Value::as_str) == Some("propose")
            && event.get("rule_id").and_then(Value::as_str) == Some(rule_id)
            && event.get("version").and_then(Value::as_str) == Some(version)
    })
}

fn release_status(events: &[Value], rule_id: &str, version: &str) -> Option<String> {
    release_states(events)
        .into_iter()
        .find(|(id, ver, _)| id == rule_id && ver == version)
        .map(|(_, _, status)| status)
}

/// Effective windows may not overlap between published versions of one rule.
/// ISO dates are fixed-width, so bounds compare lexically; an absent bound is
/// open-ended on that side.
fn windows_overlap(a: (Option<&str>, Option<&str>), b: (Option<&str>, Option<&str>)) -> bool {
    let start_after_end = |start: Option<&str>, end: Option<&str>| match (start, end) {
        (Some(start), Some(end)) => start > end,
        _ => false,
    };
    !(start_after_end(a.0, b.1) || start_after_end(b.0, a.1))
}

fn effective_window(event: &Value) -> (Option<&str>, Option<&str>) {
    (
        event.get("effective_from").and_then(Value::as_str),
        event.get("effective_until").and_then(Value::as_str),
    )
}

/// Artifact hashes with a current status of `published`.
fn published_artifacts(events: &[Value]) -> Vec<(String, String, String)> {
    release_states(events)
        .into_iter()
        .filter(|(_, _, status)| status == "published")
        .filter_map(|(rule_id, version, _)| {
            propose_event(events, &rule_id, &version)
                .and_then(|event| event.get("artifact_sha256").and_then(Value::as_str))
                .map(|hash| (rule_id, version, hash.to_string()))
        })
        .collect()
}

fn parse_rule_at_version(spec: &str) -> Result<(String, String), String> {
    spec.split_once('@')
        .map(|(id, version)| (id.to_string(), version.to_string()))
        .filter(|(id, version)| !id.is_empty() && !version.is_empty())
        .ok_or_else(|| format!("expected <rule_id>@<version>, got: {spec}"))
}

fn run_release(args: Vec<String>) -> Result<(), String> {
    let usage = || {
        "Usage: devlish-core release <propose|approve|publish|retire|list|verify> [args] [--registry registry.json]\n\
         \x20 release propose <rule.dvl> --author NAME [--cases file.json] [--evidence-output file.json]\n\
         \x20 release approve <rule_id>@<version> --approver NAME\n\
         \x20 release publish <rule_id>@<version> [--by NAME]\n\
         \x20 release retire <rule_id>@<version> [--by NAME]"
            .to_string()
    };
    let verb = args.get(1).cloned().ok_or_else(usage)?;

    let mut registry_path = PathBuf::from("registry.json");
    let mut positional: Vec<String> = Vec::new();
    let mut author: Option<String> = None;
    let mut approver: Option<String> = None;
    let mut by: Option<String> = None;
    let mut cases_path: Option<PathBuf> = None;
    let mut evidence_output: Option<PathBuf> = None;
    let mut index = 2usize;
    while index < args.len() {
        match args[index].as_str() {
            "--registry" => {
                index += 1;
                registry_path = PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--registry requires a path".to_string())?,
                );
            }
            "--author" => {
                index += 1;
                author = Some(
                    args.get(index)
                        .ok_or_else(|| "--author requires a name".to_string())?
                        .to_string(),
                );
            }
            "--approver" => {
                index += 1;
                approver = Some(
                    args.get(index)
                        .ok_or_else(|| "--approver requires a name".to_string())?
                        .to_string(),
                );
            }
            "--by" => {
                index += 1;
                by = Some(
                    args.get(index)
                        .ok_or_else(|| "--by requires a name".to_string())?
                        .to_string(),
                );
            }
            "--cases" => {
                index += 1;
                cases_path = Some(PathBuf::from(
                    args.get(index)
                        .ok_or_else(|| "--cases requires a path".to_string())?,
                ));
            }
            "--evidence-output" => {
                index += 1;
                evidence_output =
                    Some(PathBuf::from(args.get(index).ok_or_else(|| {
                        "--evidence-output requires a path".to_string()
                    })?));
            }
            value if value.starts_with('-') => return Err(format!("unknown option: {value}")),
            value => positional.push(value.to_string()),
        }
        index += 1;
    }

    match verb.as_str() {
        "propose" => {
            let rule_path = PathBuf::from(positional.first().ok_or_else(usage)?);
            let author = author.ok_or_else(|| {
                "release propose requires --author NAME (separation of duties needs a recorded author)"
                    .to_string()
            })?;
            let author = author.trim().to_string();
            if author.is_empty() {
                return Err("--author must not be empty".to_string());
            }
            release_propose(
                &registry_path,
                &rule_path,
                &author,
                cases_path,
                evidence_output,
            )
        }
        "approve" => {
            let (rule_id, version) = parse_rule_at_version(positional.first().ok_or_else(usage)?)?;
            let approver =
                approver.ok_or_else(|| "release approve requires --approver NAME".to_string())?;
            let approver = approver.trim().to_string();
            if approver.is_empty() {
                return Err("--approver must not be empty".to_string());
            }
            let mut events = load_registry_required(&registry_path)?;
            let status = release_status(&events, &rule_id, &version)
                .ok_or_else(|| format!("{rule_id}@{version} is not in the registry"))?;
            if status != "draft" {
                return Err(format!(
                    "{rule_id}@{version} is {status}; only a draft can be approved"
                ));
            }
            let proposal = propose_event(&events, &rule_id, &version)
                .ok_or_else(|| format!("{rule_id}@{version} has no propose event"))?;
            let recorded_author = proposal.get("author").and_then(Value::as_str).unwrap_or("");
            // Names are unauthenticated labels; at minimum, don't let
            // whitespace or letter case dress the author up as someone else.
            if recorded_author.trim().to_lowercase() == approver.to_lowercase() {
                return Err(format!(
                    "separation of duties: {approver} authored {rule_id}@{version} and cannot approve it"
                ));
            }
            let mut event = serde_json::Map::new();
            event.insert("action".to_string(), json!("approve"));
            event.insert("rule_id".to_string(), json!(rule_id));
            event.insert("version".to_string(), json!(version));
            event.insert("approver".to_string(), json!(approver));
            append_registry_event(&mut events, event)?;
            save_registry(&registry_path, &events)?;
            println!("approved {rule_id}@{version}");
            Ok(())
        }
        "publish" => {
            let (rule_id, version) = parse_rule_at_version(positional.first().ok_or_else(usage)?)?;
            let mut events = load_registry_required(&registry_path)?;
            let status = release_status(&events, &rule_id, &version)
                .ok_or_else(|| format!("{rule_id}@{version} is not in the registry"))?;
            // Rollback is publishing a previously approved version again, so
            // a retired release may return to published; a draft may not.
            if status != "approved" && status != "retired" {
                return Err(format!(
                    "{rule_id}@{version} is {status}; only an approved (or retired, for rollback) release can be published"
                ));
            }
            let window = propose_event(&events, &rule_id, &version)
                .map(effective_window)
                .map(|(from, until)| (from.map(str::to_string), until.map(str::to_string)))
                .unwrap_or((None, None));
            for (other_id, other_version, other_status) in release_states(&events) {
                if other_id != rule_id || other_version == version || other_status != "published" {
                    continue;
                }
                let other_window = propose_event(&events, &other_id, &other_version)
                    .map(effective_window)
                    .map(|(from, until)| (from.map(str::to_string), until.map(str::to_string)))
                    .unwrap_or((None, None));
                if windows_overlap(
                    (window.0.as_deref(), window.1.as_deref()),
                    (other_window.0.as_deref(), other_window.1.as_deref()),
                ) {
                    return Err(format!(
                        "cannot publish {rule_id}@{version}: its effective window overlaps published version {other_version} \
                         (retire it first, or fix the effective dates)"
                    ));
                }
            }
            let mut event = serde_json::Map::new();
            event.insert("action".to_string(), json!("publish"));
            event.insert("rule_id".to_string(), json!(rule_id));
            event.insert("version".to_string(), json!(version));
            if let Some(by) = by {
                event.insert("by".to_string(), json!(by));
            }
            append_registry_event(&mut events, event)?;
            save_registry(&registry_path, &events)?;
            println!("published {rule_id}@{version}");
            Ok(())
        }
        "retire" => {
            let (rule_id, version) = parse_rule_at_version(positional.first().ok_or_else(usage)?)?;
            let mut events = load_registry_required(&registry_path)?;
            let status = release_status(&events, &rule_id, &version)
                .ok_or_else(|| format!("{rule_id}@{version} is not in the registry"))?;
            if status != "published" {
                return Err(format!(
                    "{rule_id}@{version} is {status}; only a published release can be retired"
                ));
            }
            let mut event = serde_json::Map::new();
            event.insert("action".to_string(), json!("retire"));
            event.insert("rule_id".to_string(), json!(rule_id));
            event.insert("version".to_string(), json!(version));
            if let Some(by) = by {
                event.insert("by".to_string(), json!(by));
            }
            append_registry_event(&mut events, event)?;
            save_registry(&registry_path, &events)?;
            println!("retired {rule_id}@{version}");
            Ok(())
        }
        "list" => {
            let events = load_registry_required(&registry_path)?;
            if events.is_empty() {
                println!("registry is empty");
                return Ok(());
            }
            for (rule_id, version, status) in release_states(&events) {
                let artifact = propose_event(&events, &rule_id, &version)
                    .and_then(|event| event.get("artifact_sha256").and_then(Value::as_str))
                    .unwrap_or("?");
                println!("{status:<10} {rule_id}@{version}  artifact {artifact}");
            }
            Ok(())
        }
        "verify" => {
            let events = load_registry_required(&registry_path)?;
            println!("registry OK: {} event(s), hash chain intact", events.len());
            // The chain cannot see a whole-registry rewrite; print the tail
            // hash so operators can anchor it externally (docs/RELEASE.md).
            if let Some(last) = events.last() {
                println!("latest event sha256: {}", event_sha256(last)?);
            }
            Ok(())
        }
        other => Err(format!(
            "unknown release subcommand: {other}\n\n{}",
            usage()
        )),
    }
}

/// `run --governed`: only artifacts whose hash is currently a published
/// release may execute. A tampered file compiles to a different hash and is
/// refused; --as-of candidates are all checked, so effective-date resolution
/// happens over published releases only.
fn assert_published(registry_path: &Path, package: &Value, source: &Path) -> Result<(), String> {
    let events = load_registry(registry_path)?;
    let bytecode_json = serde_json::to_string_pretty(package).map_err(|e| e.to_string())?;
    let hash = sha256_hex(bytecode_json.as_bytes());
    if published_artifacts(&events)
        .iter()
        .any(|(_, _, published_hash)| published_hash == &hash)
    {
        return Ok(());
    }
    Err(format!(
        "{}: artifact {hash} is not a published release in {} -- refusing to run under --governed",
        source.display(),
        registry_path.display()
    ))
}

fn release_propose(
    registry_path: &Path,
    rule_path: &Path,
    author: &str,
    cases_path: Option<PathBuf>,
    evidence_output: Option<PathBuf>,
) -> Result<(), String> {
    let package = load_package(rule_path)?;
    let bytecode_json = serde_json::to_string_pretty(&package).map_err(|e| e.to_string())?;
    let artifact_sha256 = sha256_hex(bytecode_json.as_bytes());

    let rule = package.get("manifest").and_then(|m| m.get("rule"));
    let rule_id = rule
        .and_then(|r| r.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "release propose requires a governed rule (a Rule: section with id and version)"
                .to_string()
        })?
        .to_string();
    let rule_version = rule
        .and_then(|r| r.get("version"))
        .and_then(Value::as_str)
        .ok_or_else(|| "the rule's Rule: section must declare a version".to_string())?
        .to_string();
    let effective_from = rule
        .and_then(|r| r.get("effective_from"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let effective_until = rule
        .and_then(|r| r.get("effective_until"))
        .and_then(Value::as_str)
        .map(str::to_string);

    let mut events = load_registry(registry_path)?;
    if release_status(&events, &rule_id, &rule_version).is_some() {
        return Err(format!(
            "{rule_id}@{rule_version} is already in the registry; bump the rule's version to propose a new release"
        ));
    }

    // A draft binds artifact hash to evidence hash: the golden cases must
    // pass against this exact artifact before it can enter the registry.
    let cases_path = cases_path.unwrap_or_else(|| {
        let stem = rule_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("rule");
        let mut p = rule_path.to_path_buf();
        p.set_file_name(format!("{stem}.cases.json"));
        p
    });
    let evidence_path = evidence_output.unwrap_or_else(|| {
        let stem = rule_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("rule");
        let mut p = rule_path.to_path_buf();
        p.set_file_name(format!("{stem}.evidence.json"));
        p
    });
    run_evidence(vec![
        "evidence".to_string(),
        rule_path.to_string_lossy().to_string(),
        "--cases".to_string(),
        cases_path.to_string_lossy().to_string(),
        "--output".to_string(),
        evidence_path.to_string_lossy().to_string(),
    ])
    .map_err(|error| format!("release propose refused: {error}"))?;
    let evidence: Value = serde_json::from_str(
        &fs::read_to_string(&evidence_path)
            .map_err(|error| format!("failed to read {}: {error}", evidence_path.display()))?,
    )
    .map_err(|error| format!("invalid evidence report: {error}"))?;
    // The evidence run re-read the rule from disk: if the file changed
    // between the two loads, the report certifies a different artifact than
    // the one this propose is about to record. Refuse the bind.
    let evidence_artifact = evidence
        .get("artifact_sha256")
        .and_then(Value::as_str)
        .unwrap_or("");
    if evidence_artifact != artifact_sha256 {
        return Err(format!(
            "release propose refused: evidence certifies artifact {evidence_artifact}, \
             but the rule on disk hashes to {artifact_sha256} (file changed during propose?)"
        ));
    }
    let evidence_rule = evidence.get("rule");
    let evidence_id = evidence_rule
        .and_then(|r| r.get("id"))
        .and_then(Value::as_str);
    let evidence_version = evidence_rule
        .and_then(|r| r.get("version"))
        .and_then(Value::as_str);
    if evidence_id != Some(rule_id.as_str()) || evidence_version != Some(rule_version.as_str()) {
        return Err(format!(
            "release propose refused: evidence is for {}@{}, not {rule_id}@{rule_version}",
            evidence_id.unwrap_or("?"),
            evidence_version.unwrap_or("?")
        ));
    }
    let evidence_sha256 = evidence
        .get("report_sha256")
        .and_then(Value::as_str)
        .ok_or_else(|| "evidence report has no report_sha256".to_string())?
        .to_string();

    let mut event = serde_json::Map::new();
    event.insert("action".to_string(), json!("propose"));
    event.insert("rule_id".to_string(), json!(rule_id));
    event.insert("version".to_string(), json!(rule_version));
    event.insert("artifact_sha256".to_string(), json!(artifact_sha256));
    event.insert("evidence_sha256".to_string(), json!(evidence_sha256));
    if let Some(from) = effective_from {
        event.insert("effective_from".to_string(), json!(from));
    }
    if let Some(until) = effective_until {
        event.insert("effective_until".to_string(), json!(until));
    }
    event.insert("author".to_string(), json!(author));
    append_registry_event(&mut events, event)?;
    save_registry(registry_path, &events)?;
    println!(
        "proposed {rule_id}@{rule_version} as draft (artifact {artifact_sha256}, evidence {evidence_sha256})"
    );
    Ok(())
}

/// Uniform error for an effect that requires the `native` feature in a build
/// where it is compiled out.
#[cfg(not(feature = "native"))]
fn native_effect_disabled(effect: &str) -> String {
    format!("{effect} effects are not available in this build (native feature disabled)")
}

impl HostEffects for NativeHost {
    fn audit_record(&mut self, record: &Value) -> Result<(), String> {
        match &mut self.audit_log {
            Some(writer) => writer.append(record),
            None => Ok(()),
        }
    }

    fn emit_event(&mut self, event: &Value) {
        if let Ok(line) = serde_json::to_string(event) {
            eprintln!("{line}");
        }
    }

    fn write_file(&mut self, request: &Value) -> Result<(), String> {
        let path = request
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| "write_file request missing path".to_string())?;
        let content = request
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let mode = request
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("write");

        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!("failed to create directory {}: {error}", parent.display())
            })?;
        }
        match mode {
            "append" => {
                let mut file = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .map_err(|error| format!("failed to open {path}: {error}"))?;
                file.write_all(content.as_bytes())
                    .map_err(|error| format!("failed to append {path}: {error}"))?;
            }
            "assertions" | "csv" | "export" | "overwrite" | "write" => {
                fs::write(path, content)
                    .map_err(|error| format!("failed to write {path}: {error}"))?;
            }
            other => return Err(format!("unsupported write_file mode: {other}")),
        }
        Ok(())
    }

    fn read_file(&mut self, request: &Value) -> Result<Value, String> {
        let path = request
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| "read_file request missing path".to_string())?;
        let content =
            fs::read_to_string(path).map_err(|error| format!("failed to read {path}: {error}"))?;
        Ok(Value::String(content))
    }

    fn call_service(&mut self, request: &Value) -> Result<Value, String> {
        // Native host cannot make outbound service calls.
        // Return the request as-is so the caller can see what was attempted.
        let service = request
            .get("service")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let action = request
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        Err(format!(
            "Service call {service}.{action} is not available in the native runner. \
             Use the WASM runner with a host that provides service bindings."
        ))
    }

    fn file_copy(&mut self, source: &str, destination: &str) -> Result<(), String> {
        if let Some(parent) = Path::new(destination).parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create directory {}: {e}", parent.display()))?;
        }
        let meta = fs::metadata(source).map_err(|e| format!("failed to read {source}: {e}"))?;
        if meta.is_dir() {
            copy_dir_recursive(source, destination)?;
        } else {
            fs::copy(source, destination)
                .map_err(|e| format!("failed to copy {source} to {destination}: {e}"))?;
        }
        Ok(())
    }

    fn file_move(&mut self, source: &str, destination: &str) -> Result<(), String> {
        if let Some(parent) = Path::new(destination).parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create directory {}: {e}", parent.display()))?;
        }
        fs::rename(source, destination)
            .map_err(|e| format!("failed to move {source} to {destination}: {e}"))
    }

    fn file_mkdir(&mut self, path: &str) -> Result<(), String> {
        fs::create_dir_all(path).map_err(|e| format!("failed to create directory {path}: {e}"))
    }

    fn file_delete(&mut self, path: &str) -> Result<(), String> {
        let meta = fs::metadata(path).map_err(|e| format!("failed to read {path}: {e}"))?;
        if meta.is_dir() {
            fs::remove_dir_all(path).map_err(|e| format!("failed to delete directory {path}: {e}"))
        } else {
            fs::remove_file(path).map_err(|e| format!("failed to delete {path}: {e}"))
        }
    }

    fn file_exists(&mut self, path: &str) -> Result<bool, String> {
        Ok(Path::new(path).exists())
    }

    fn file_stat(&mut self, path: &str) -> Result<Value, String> {
        let meta = fs::metadata(path).map_err(|e| format!("failed to stat {path}: {e}"))?;
        let file_type = if meta.is_dir() {
            "directory"
        } else if meta.is_symlink() {
            "symlink"
        } else {
            "file"
        };
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs());
        let mut record = serde_json::Map::new();
        record.insert("path".to_string(), json!(path));
        record.insert("type".to_string(), json!(file_type));
        record.insert("size".to_string(), json!(meta.len()));
        if let Some(ts) = modified {
            record.insert("modified".to_string(), json!(ts));
        }
        Ok(Value::Object(record))
    }

    fn file_list(&mut self, path: &str) -> Result<Value, String> {
        let entries = fs::read_dir(path).map_err(|e| format!("failed to list {path}: {e}"))?;
        let mut names: Vec<Value> = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| format!("failed to read entry in {path}: {e}"))?;
            names.push(Value::String(
                entry.file_name().to_string_lossy().to_string(),
            ));
        }
        names.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
        Ok(Value::Array(names))
    }

    fn file_glob(&mut self, pattern: &str, directory: &str) -> Result<Value, String> {
        let full_pattern = if directory.is_empty() || directory == "." {
            pattern.to_string()
        } else {
            let dir = directory.trim_end_matches('/');
            format!("{dir}/{pattern}")
        };
        let paths = glob::glob(&full_pattern)
            .map_err(|e| format!("invalid glob pattern {full_pattern}: {e}"))?;
        let mut results: Vec<Value> = Vec::new();
        for entry in paths {
            let path = entry.map_err(|e| format!("glob error: {e}"))?;
            results.push(Value::String(path.to_string_lossy().to_string()));
        }
        results.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
        Ok(Value::Array(results))
    }

    #[cfg(feature = "native")]
    fn http_download(&mut self, url: &str, path: &str) -> Result<(), String> {
        let response = ureq::get(url)
            .call()
            .map_err(|e| format!("HTTP GET {url}: {e}"))?;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .read_to_end(&mut bytes)
            .map_err(|e| format!("Failed to read response body: {e}"))?;
        fs::write(path, &bytes).map_err(|e| format!("Failed to write {path}: {e}"))?;
        Ok(())
    }

    #[cfg(not(feature = "native"))]
    fn http_download(&mut self, _url: &str, _path: &str) -> Result<(), String> {
        Err(native_effect_disabled("HTTP"))
    }

    #[cfg(feature = "native")]
    fn read_xlsx_rows(&mut self, path: &str, sheet: Option<&str>) -> Result<Value, String> {
        use calamine::{Reader, Xlsx};

        let mut workbook: Xlsx<_> = calamine::open_workbook(path)
            .map_err(|e| format!("Failed to open XLSX {path}: {e}"))?;
        let sheet_name = if let Some(name) = sheet {
            name.to_string()
        } else {
            workbook
                .sheet_names()
                .first()
                .cloned()
                .ok_or_else(|| format!("No sheets found in {path}"))?
        };
        let range = workbook
            .worksheet_range(&sheet_name)
            .map_err(|e| format!("Failed to read sheet {sheet_name}: {e}"))?;

        let mut rows_iter = range.rows();
        let headers: Vec<String> = match rows_iter.next() {
            Some(row) => row.iter().map(|cell| cell_to_string(cell)).collect(),
            None => return Ok(Value::Array(Vec::new())),
        };

        let mut records = Vec::new();
        for row in rows_iter {
            let mut record = serde_json::Map::new();
            for (i, cell) in row.iter().enumerate() {
                let key = headers
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| format!("column_{i}"));
                let value = match cell {
                    calamine::Data::Int(n) => json!(*n),
                    calamine::Data::Float(f) => json!(*f),
                    calamine::Data::Bool(b) => json!(*b),
                    calamine::Data::Empty => Value::Null,
                    other => Value::String(cell_to_string(other)),
                };
                record.insert(key, value);
            }
            records.push(Value::Object(record));
        }
        Ok(Value::Array(records))
    }

    #[cfg(not(feature = "native"))]
    fn read_xlsx_rows(&mut self, _path: &str, _sheet: Option<&str>) -> Result<Value, String> {
        Err(native_effect_disabled("Spreadsheet"))
    }

    fn respond(&mut self, value: &Value) -> Result<(), String> {
        let json = serde_json::to_string_pretty(value)
            .map_err(|e| format!("Failed to serialize response: {e}"))?;
        println!("{json}");
        Ok(())
    }

    #[cfg(feature = "native")]
    fn http_request(
        &mut self,
        method: &str,
        url: &str,
        body: &Value,
        headers: &Value,
    ) -> Result<Value, String> {
        let mut request = match method {
            "GET" => ureq::get(url),
            "POST" => ureq::post(url),
            "PUT" => ureq::put(url),
            "PATCH" => ureq::patch(url),
            "DELETE" => ureq::delete(url),
            other => return Err(format!("Unsupported HTTP method: {other}")),
        };

        // Inject auth from credentials: check BEARER_TOKEN, HTTP_AUTH_TOKEN,
        // or API_KEY in the credential store
        if let Some(token) = self
            .credentials
            .resolve("BEARER_TOKEN")
            .or_else(|| self.credentials.resolve("HTTP_AUTH_TOKEN"))
        {
            request = request.set("Authorization", &format!("Bearer {token}"));
        } else if let Some(api_key) = self.credentials.resolve("API_KEY") {
            request = request.set("X-API-Key", &api_key);
        }

        // Apply custom headers from the headers parameter
        if let Some(obj) = headers.as_object() {
            for (key, val) in obj {
                if let Some(v) = val.as_str() {
                    request = request.set(key, v);
                }
            }
        }

        let has_body = matches!(method, "POST" | "PUT" | "PATCH");
        let response = if has_body {
            let body_str = match body {
                Value::String(s) => s.clone(),
                Value::Null => String::new(),
                other => serde_json::to_string(other)
                    .map_err(|e| format!("Failed to serialize body: {e}"))?,
            };
            if body_str.is_empty() {
                request.call()
            } else {
                request
                    .set("Content-Type", "application/json")
                    .send_string(&body_str)
            }
            .map_err(|e| format!("HTTP {method} {url}: {e}"))?
        } else {
            request
                .call()
                .map_err(|e| format!("HTTP {method} {url}: {e}"))?
        };

        let status = response.status();
        let content_type = response.header("Content-Type").unwrap_or("").to_string();
        let response_body = response
            .into_string()
            .map_err(|e| format!("Failed to read response body: {e}"))?;

        let body_value = if content_type.contains("application/json") {
            serde_json::from_str::<Value>(&response_body).unwrap_or(Value::String(response_body))
        } else {
            Value::String(response_body)
        };

        Ok(json!({
            "status": status,
            "content_type": content_type,
            "body": body_value
        }))
    }

    #[cfg(not(feature = "native"))]
    fn http_request(
        &mut self,
        _method: &str,
        _url: &str,
        _body: &Value,
        _headers: &Value,
    ) -> Result<Value, String> {
        Err(native_effect_disabled("HTTP"))
    }

    fn resolve_credential(&self, key: &str) -> Option<String> {
        self.credentials.resolve(key)
    }
}

fn run_course(_args: Vec<String>) -> Result<(), String> {
    // Find the course directory relative to the binary or current dir
    let course_dir = find_course_dir()?;

    // Discover all lessons in order
    let mut lessons: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
    let mut unit_dirs: Vec<PathBuf> = Vec::new();

    for entry in
        fs::read_dir(&course_dir).map_err(|e| format!("Cannot read course directory: {e}"))?
    {
        let entry = entry.map_err(|e| format!("Read error: {e}"))?;
        let path = entry.path();
        if path.is_dir() {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if name.starts_with(|c: char| c.is_ascii_digit()) {
                unit_dirs.push(path);
            }
        }
    }
    unit_dirs.sort();

    for unit_dir in &unit_dirs {
        let mut lesson_files: Vec<PathBuf> = Vec::new();
        for entry in
            fs::read_dir(unit_dir).map_err(|e| format!("Cannot read unit directory: {e}"))?
        {
            let entry = entry.map_err(|e| format!("Read error: {e}"))?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "md")
                && path.file_name().unwrap_or_default() != "README.md"
            {
                lesson_files.push(path);
            }
        }
        lesson_files.sort();

        for lesson_path in lesson_files {
            let lesson_name = lesson_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let num_prefix: String = lesson_name
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();

            // Find companion example
            let examples_dir = unit_dir.join("examples");
            let example = if examples_dir.is_dir() && !num_prefix.is_empty() {
                fs::read_dir(&examples_dir).ok().and_then(|entries| {
                    entries.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| {
                        let name = p
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string();
                        name.starts_with(&format!("{num_prefix}_")) && name.ends_with(".dvl")
                    })
                })
            } else {
                None
            };

            lessons.push((lesson_path, example));
        }
    }

    if lessons.is_empty() {
        return Err("No lessons found in course directory.".to_string());
    }

    let term_height = terminal_height();

    println!("\n  Devlish Interactive Course");
    println!(
        "  {} lessons across {} units\n",
        lessons.len(),
        unit_dirs.len()
    );

    let stdin = std::io::stdin();
    let mut current = 0usize;

    loop {
        let (lesson_path, example_path) = &lessons[current];

        // Read and render the lesson into display lines
        let content =
            fs::read_to_string(lesson_path).map_err(|e| format!("Cannot read lesson: {e}"))?;

        let mut display_lines: Vec<String> = Vec::new();
        display_lines.push("=".repeat(72));
        display_lines.push(format!(
            "  Lesson {}/{}: {}",
            current + 1,
            lessons.len(),
            lesson_path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .replace('-', " ")
        ));
        if let Some(ref ex) = example_path {
            display_lines.push(format!(
                "  Example: {} (press r to run)",
                ex.file_name().unwrap_or_default().to_string_lossy()
            ));
        }
        display_lines.push("=".repeat(72));
        display_lines.push(String::new());

        let mut in_code_block = false;
        for line in content.lines() {
            if line.starts_with("```") {
                in_code_block = !in_code_block;
                if in_code_block && line.len() > 3 {
                    display_lines.push(String::new());
                    display_lines.push(format!("  [{} code]", &line[3..]));
                } else if !in_code_block {
                    display_lines.push(String::new());
                }
            } else if in_code_block {
                display_lines.push(format!("    {line}"));
            } else if line.starts_with("# ") {
                display_lines.push(String::new());
                display_lines.push(format!("  {}", &line[2..]));
                display_lines.push(String::new());
            } else if line.starts_with("## ") {
                display_lines.push(String::new());
                display_lines.push(format!("  -- {} --", &line[3..]));
                display_lines.push(String::new());
            } else if line.starts_with("### ") {
                display_lines.push(format!("  > {}", &line[4..]));
                display_lines.push(String::new());
            } else if line.starts_with("- ") {
                display_lines.push(format!("    * {}", &line[2..]));
            } else if line.trim().is_empty() {
                display_lines.push(String::new());
            } else {
                display_lines.push(format!("  {line}"));
            }
        }

        // Paginate: show term_height-2 lines at a time
        let page_size = if term_height > 4 { term_height - 2 } else { 20 };
        let mut line_pos = 0usize;

        while line_pos < display_lines.len() {
            let end = std::cmp::min(line_pos + page_size, display_lines.len());
            for line in &display_lines[line_pos..end] {
                println!("{line}");
            }
            line_pos = end;

            if line_pos < display_lines.len() {
                // More content to show
                print!("  -- more (Enter) | [s] skip to end | [q] quit --  ");
                std::io::Write::flush(&mut std::io::stdout()).ok();
                let mut input = String::new();
                stdin
                    .read_line(&mut input)
                    .map_err(|e| format!("Read error: {e}"))?;
                let cmd = input.trim().to_ascii_lowercase();
                if cmd == "q" || cmd == "quit" {
                    println!(
                        "\n  Course ended at lesson {}/{}.\n",
                        current + 1,
                        lessons.len()
                    );
                    return Ok(());
                }
                if cmd == "s" || cmd == "skip" {
                    // Print remaining lines
                    for line in &display_lines[line_pos..] {
                        println!("{line}");
                    }
                    break;
                }
            }
        }

        // Lesson navigation prompt
        println!();
        let prompt = if let Some(ref ex) = example_path {
            format!(
                "  [{}/{}] [Enter] next | [p] prev | [r] run {} | [q] quit: ",
                current + 1,
                lessons.len(),
                ex.file_name().unwrap_or_default().to_string_lossy()
            )
        } else {
            format!(
                "  [{}/{}] [Enter] next | [p] prev | [q] quit: ",
                current + 1,
                lessons.len()
            )
        };
        print!("{prompt}");
        std::io::Write::flush(&mut std::io::stdout()).ok();

        let mut input = String::new();
        stdin
            .read_line(&mut input)
            .map_err(|e| format!("Read error: {e}"))?;
        let cmd = input.trim().to_ascii_lowercase();

        match cmd.as_str() {
            "q" | "quit" | "exit" => {
                println!(
                    "\n  Course ended at lesson {}/{}. See you next time!\n",
                    current + 1,
                    lessons.len()
                );
                return Ok(());
            }
            "p" | "prev" | "previous" => {
                if current > 0 {
                    current -= 1;
                } else {
                    println!("\n  (Already at the first lesson)\n");
                }
            }
            "r" | "run" => {
                if let Some(ref ex) = example_path {
                    println!("\n  Running: {}\n", ex.display());
                    println!("{}", "-".repeat(60));

                    if let Ok(source) = fs::read_to_string(ex) {
                        for (i, line) in source.lines().enumerate() {
                            println!("  {:>3} | {line}", i + 1);
                        }
                    }
                    println!("{}", "-".repeat(60));

                    let source =
                        fs::read_to_string(ex).map_err(|e| format!("Cannot read example: {e}"))?;
                    let source_path = ex.to_string_lossy().to_string();
                    match compile_source_to_json(
                        &source,
                        CompileOptions {
                            source_path: Some(source_path),
                            search_paths: devlish_search_paths_for(Some(ex.as_path())),
                        },
                    ) {
                        Err(error) => println!("  Compile error: {error}"),
                        Ok(json_str) => {
                            let package: Value =
                                serde_json::from_str(&json_str).unwrap_or_else(|_| json!({}));
                            let mut host = NativeHost {
                                credentials: CredentialStore::new(&[], Some(ex.as_path())),
                                audit_log: None,
                            };
                            match Vm::new(package, json!({})) {
                                Err(error) => println!("  VM error: {}", error.message),
                                Ok(mut vm) => match vm.run(&mut host) {
                                    Ok(result) => {
                                        let responded = result
                                            .get("responded")
                                            .and_then(Value::as_bool)
                                            .unwrap_or(false);
                                        if responded {
                                            println!("  (Program responded with structured output above)");
                                        } else {
                                            println!("  Result: success");
                                        }
                                    }
                                    Err(error) => println!("  Runtime: {}", error.message),
                                },
                            }
                        }
                    }
                    println!("{}\n", "-".repeat(60));

                    // Wait for user before continuing
                    print!("  Press Enter to continue... ");
                    std::io::Write::flush(&mut std::io::stdout()).ok();
                    let mut discard = String::new();
                    stdin.read_line(&mut discard).ok();
                } else {
                    println!("\n  (No example for this lesson)\n");
                }
            }
            "" | "n" | "next" => {
                if current + 1 < lessons.len() {
                    current += 1;
                } else {
                    println!(
                        "\n  Congratulations! You have completed all {} lessons.\n",
                        lessons.len()
                    );
                    return Ok(());
                }
            }
            _ => {
                println!("  Unknown command. Use [Enter], [p], [r], or [q].");
            }
        }
    }
}

fn terminal_height() -> usize {
    // Try LINES env var first, then ioctl, then default
    if let Ok(lines) = env::var("LINES") {
        if let Ok(n) = lines.parse::<usize>() {
            if n > 4 {
                return n;
            }
        }
    }
    #[cfg(unix)]
    {
        use std::mem::MaybeUninit;
        unsafe {
            let mut ws = MaybeUninit::<libc::winsize>::zeroed().assume_init();
            if libc::ioctl(1, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_row > 0 {
                return ws.ws_row as usize;
            }
        }
    }
    40
}

fn run_format(args: Vec<String>) -> Result<(), String> {
    let file = args
        .get(1)
        .ok_or_else(|| "Usage: devlish fmt <file.dvl>".to_string())?;
    let path = PathBuf::from(file);
    let source =
        fs::read_to_string(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;

    let mut output = String::new();
    let mut indent_level: usize = 0;
    let mut in_manifest = false;

    for original_line in source.lines() {
        let trimmed = original_line.trim();

        // Empty lines and comments pass through
        if trimmed.is_empty() {
            output.push('\n');
            continue;
        }
        if trimmed.starts_with('#') {
            let indent = "  ".repeat(indent_level);
            output.push_str(&format!("{indent}{trimmed}\n"));
            continue;
        }

        let lower = trimmed.to_ascii_lowercase();

        // Manifest headers at column 0
        if lower == "permissions:" || lower == "boundaries:" || lower == "callers:" {
            in_manifest = true;
            output.push_str(&format!("{trimmed}\n"));
            continue;
        }

        // Manifest body lines at indent 2
        if in_manifest {
            if leading_spaces(original_line) > 0
                || trimmed.starts_with("Read ")
                || trimmed.starts_with("Write ")
                || trimmed.starts_with("HTTP ")
                || trimmed.starts_with("Filesystem ")
                || trimmed.starts_with("Call ")
                || trimmed.starts_with("No ")
                || trimmed.starts_with("Any ")
            {
                output.push_str(&format!("  {trimmed}\n"));
                continue;
            }
            in_manifest = false;
        }

        // Dedent before Otherwise
        if lower == "otherwise:" || lower == "otherwise" {
            if indent_level > 0 {
                indent_level -= 1;
            }
        }

        // Write the line at current indent
        let indent = "  ".repeat(indent_level);
        output.push_str(&format!("{indent}{trimmed}\n"));

        // Indent after block openers
        let is_block_opener = lower.starts_with("if ")
            || lower == "otherwise:"
            || lower == "otherwise"
            || lower.starts_with("for each ")
            || lower.starts_with("while ")
            || lower.starts_with("until ")
            || lower == "try:"
            || lower == "try";
        if is_block_opener {
            indent_level += 1;
        }
    }

    // Check if we should write back or print
    let write_back = args.iter().any(|a| a == "--write" || a == "-w");
    if write_back {
        fs::write(&path, &output)
            .map_err(|e| format!("failed to write {}: {e}", path.display()))?;
        println!("Formatted {}", path.display());
    } else {
        print!("{output}");
    }
    Ok(())
}

fn run_repl(_args: Vec<String>) -> Result<(), String> {
    println!("Devlish {VERSION} REPL");
    println!("Type Devlish statements. Enter a blank line to run. Type 'quit' to exit.\n");

    let stdin = std::io::stdin();
    let mut accumulated = String::new();
    let mut line_count = 0u32;

    loop {
        if accumulated.is_empty() {
            print!("dvl> ");
        } else {
            print!("...> ");
        }
        std::io::Write::flush(&mut std::io::stdout()).ok();

        let mut input = String::new();
        if stdin
            .read_line(&mut input)
            .map_err(|e| format!("Read error: {e}"))?
            == 0
        {
            // EOF
            break;
        }

        let trimmed = input.trim();

        if trimmed == "quit" || trimmed == "exit" {
            println!("Bye!");
            return Ok(());
        }

        if trimmed == "clear" {
            accumulated.clear();
            line_count = 0;
            println!("(cleared)");
            continue;
        }

        if trimmed == "help" {
            println!("  Type Devlish statements, one per line.");
            println!("  Press Enter on a blank line to compile and run.");
            println!("  Commands: clear, help, quit\n");
            continue;
        }

        // Blank line triggers execution
        if trimmed.is_empty() && !accumulated.is_empty() {
            // Compile and run the accumulated source
            match compile_source_to_json(
                &accumulated,
                CompileOptions {
                    source_path: None,
                    search_paths: devlish_search_paths_for(None),
                },
            ) {
                Err(error) => {
                    eprintln!("  Compile error: {error}");
                }
                Ok(json_str) => {
                    let package: Value =
                        serde_json::from_str(&json_str).unwrap_or_else(|_| json!({}));
                    let mut host = ReplHost(CredentialStore::new(&[], None));
                    match Vm::new(package, json!({})) {
                        Err(error) => eprintln!("  VM error: {}", error.message),
                        Ok(mut vm) => match vm.run(&mut host) {
                            Ok(result) => {
                                // Show context variables (skip internal ones)
                                if let Some(ctx) = result.get("context").and_then(Value::as_object)
                                {
                                    for (key, val) in ctx {
                                        if !key.starts_with('r') || key.parse::<u32>().is_err() {
                                            let display = match val {
                                                Value::String(s) => format!("\"{s}\""),
                                                other => serde_json::to_string(other)
                                                    .unwrap_or_else(|_| format!("{other:?}")),
                                            };
                                            println!("  {key} = {display}");
                                        }
                                    }
                                }
                            }
                            Err(error) => eprintln!("  Runtime error: {}", error.message),
                        },
                    }
                }
            }
            accumulated.clear();
            line_count = 0;
            println!();
            continue;
        }

        // Accumulate the line
        if !trimmed.is_empty() {
            accumulated.push_str(&input);
            line_count += 1;
        }
    }
    Ok(())
}

/// Host for the REPL that suppresses VM event output but prints Print/Show output.
struct ReplHost(CredentialStore);

impl HostEffects for ReplHost {
    fn emit_event(&mut self, _event: &Value) {
        // Suppress VM events in the REPL
    }
    fn write_file(&mut self, request: &Value) -> Result<(), String> {
        let mut native = NativeHost {
            audit_log: None,
            credentials: CredentialStore {
                entries: Vec::new(),
            },
        };
        native.write_file(request)
    }
    fn read_file(&mut self, request: &Value) -> Result<Value, String> {
        let mut native = NativeHost {
            audit_log: None,
            credentials: CredentialStore {
                entries: Vec::new(),
            },
        };
        native.read_file(request)
    }
    fn respond(&mut self, value: &Value) -> Result<(), String> {
        let json = serde_json::to_string_pretty(value)
            .map_err(|e| format!("Failed to serialize response: {e}"))?;
        println!("{json}");
        Ok(())
    }
    fn resolve_credential(&self, key: &str) -> Option<String> {
        self.0.resolve(key)
    }
}

fn leading_spaces(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

fn find_course_dir() -> Result<PathBuf, String> {
    // Try relative to current directory
    let candidates = [
        PathBuf::from("docs/course"),
        PathBuf::from("../docs/course"),
    ];
    for candidate in &candidates {
        if candidate.is_dir() {
            return Ok(candidate.clone());
        }
    }
    // Try relative to the binary location
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            // Binary might be in crates/devlish_core/target/release/
            for ancestor in parent.ancestors().take(6) {
                let course = ancestor.join("docs/course");
                if course.is_dir() {
                    return Ok(course);
                }
            }
        }
    }
    Err(
        "Course directory not found. Run from the devlish project root \
         or a subdirectory."
            .to_string(),
    )
}

fn copy_dir_recursive(source: &str, destination: &str) -> Result<(), String> {
    fs::create_dir_all(destination)
        .map_err(|e| format!("failed to create directory {destination}: {e}"))?;
    for entry in
        fs::read_dir(source).map_err(|e| format!("failed to read directory {source}: {e}"))?
    {
        let entry = entry.map_err(|e| format!("failed to read entry: {e}"))?;
        let src_path = entry.path();
        let dst_path = Path::new(destination).join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path.to_string_lossy(), &dst_path.to_string_lossy())?;
        } else {
            fs::copy(&src_path, &dst_path).map_err(|e| {
                format!(
                    "failed to copy {} to {}: {e}",
                    src_path.display(),
                    dst_path.display()
                )
            })?;
        }
    }
    Ok(())
}

#[cfg(feature = "native")]
fn cell_to_string(cell: &calamine::Data) -> String {
    match cell {
        calamine::Data::Int(i) => i.to_string(),
        calamine::Data::Float(f) => {
            if *f == (*f as i64) as f64 {
                (*f as i64).to_string()
            } else {
                f.to_string()
            }
        }
        calamine::Data::String(s) => s.clone(),
        calamine::Data::Bool(b) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        calamine::Data::Empty => String::new(),
        calamine::Data::DateTime(dt) => dt.to_string(),
        calamine::Data::DateTimeIso(s) => s.clone(),
        calamine::Data::DurationIso(s) => s.clone(),
        calamine::Data::Error(e) => format!("{e:?}"),
    }
}

/// A .dvl tool discovered from a devlish.toml manifest.
struct DvlTool {
    name: String,
    description: String,
    source_path: PathBuf,
    inputs: Vec<(String, String, String)>, // (name, type, description)
}

/// Scan a directory for devlish.toml with [[tools]] entries.
fn discover_tools_from_dir(dir: &Path) -> Vec<DvlTool> {
    let manifest_path = dir.join("devlish.toml");
    if !manifest_path.is_file() {
        // No manifest: scan for .dvl files directly, use filename as tool name
        let mut tools = Vec::new();
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("dvl") {
                    let name = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    // Read first comment block as description
                    let description = fs::read_to_string(&path)
                        .ok()
                        .map(|src| {
                            src.lines()
                                .take_while(|l| l.starts_with('#'))
                                .map(|l| l.trim_start_matches('#').trim())
                                .filter(|l| !l.is_empty())
                                .next()
                                .unwrap_or("")
                                .to_string()
                        })
                        .unwrap_or_default();
                    tools.push(DvlTool {
                        name,
                        description,
                        source_path: path,
                        inputs: Vec::new(),
                    });
                }
            }
        }
        return tools;
    }

    let content = match fs::read_to_string(&manifest_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    parse_tools_from_toml(&content, dir)
}

/// Parse [[tools]] entries from a devlish.toml string.
fn parse_tools_from_toml(content: &str, base_dir: &Path) -> Vec<DvlTool> {
    let mut tools = Vec::new();
    let mut in_tool = false;
    let mut in_inputs = false;
    let mut current_name = String::new();
    let mut current_desc = String::new();
    let mut current_source = String::new();
    let mut current_inputs: Vec<(String, String, String)> = Vec::new();
    // Set while inside a per-parameter subsection like [tools.parameters.NAME]
    let mut pending_input: Option<(String, String, String)> = None;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed == "[[tools]]" {
            if let Some(input) = pending_input.take() {
                current_inputs.push(input);
            }
            // Flush previous tool if any
            if in_tool && !current_source.is_empty() {
                let source_path = base_dir.join(&current_source);
                if source_path.is_file() {
                    let name = if current_name.is_empty() {
                        source_path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("unknown")
                            .to_string()
                    } else {
                        current_name.clone()
                    };
                    tools.push(DvlTool {
                        name,
                        description: current_desc.clone(),
                        source_path,
                        inputs: current_inputs.clone(),
                    });
                }
            }
            in_tool = true;
            in_inputs = false;
            current_name.clear();
            current_desc.clear();
            current_source.clear();
            current_inputs.clear();
            continue;
        }

        if trimmed == "[tools.inputs]" || trimmed == "[tools.parameters]" {
            if let Some(input) = pending_input.take() {
                current_inputs.push(input);
            }
            in_inputs = true;
            continue;
        }

        // Per-parameter subsection: [tools.inputs.NAME] / [tools.parameters.NAME]
        if let Some(param_name) = trimmed
            .strip_prefix("[tools.inputs.")
            .or_else(|| trimmed.strip_prefix("[tools.parameters."))
            .and_then(|rest| rest.strip_suffix(']'))
        {
            if let Some(input) = pending_input.take() {
                current_inputs.push(input);
            }
            pending_input = Some((param_name.to_string(), "string".to_string(), String::new()));
            in_inputs = false;
            continue;
        }

        if trimmed.starts_with('[') {
            // Another section starts, flush if needed
            if let Some(input) = pending_input.take() {
                current_inputs.push(input);
            }
            if in_inputs {
                in_inputs = false;
            }
            if in_tool && trimmed != "[[tools]]" && !trimmed.starts_with("[tools.") {
                // Non-tools section, flush current tool
                if !current_source.is_empty() {
                    let source_path = base_dir.join(&current_source);
                    if source_path.is_file() {
                        let name = if current_name.is_empty() {
                            source_path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("unknown")
                                .to_string()
                        } else {
                            current_name.clone()
                        };
                        tools.push(DvlTool {
                            name,
                            description: current_desc.clone(),
                            source_path,
                            inputs: current_inputs.clone(),
                        });
                    }
                }
                in_tool = false;
            }
            continue;
        }

        if !in_tool {
            continue;
        }

        // Parse key = "value" lines
        if let Some((key, val)) = trimmed.split_once('=') {
            let key = key.trim();
            let val = val.trim().trim_matches('"');

            if let Some((_, input_type, input_desc)) = pending_input.as_mut() {
                match key {
                    "type" => *input_type = val.to_string(),
                    "description" => *input_desc = val.to_string(),
                    _ => {}
                }
            } else if in_inputs {
                // input_name = { type = "number", description = "..." }
                // Simple parse: extract type and description from inline table
                let input_type = extract_inline_field(val, "type").unwrap_or("string".to_string());
                let input_desc = extract_inline_field(val, "description").unwrap_or_default();
                current_inputs.push((key.to_string(), input_type, input_desc));
            } else {
                match key {
                    "name" => current_name = val.to_string(),
                    "description" => current_desc = val.to_string(),
                    "source" => current_source = val.to_string(),
                    _ => {}
                }
            }
        }
    }

    // Flush last tool
    if let Some(input) = pending_input.take() {
        current_inputs.push(input);
    }
    if in_tool && !current_source.is_empty() {
        let source_path = base_dir.join(&current_source);
        if source_path.is_file() {
            let name = if current_name.is_empty() {
                source_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string()
            } else {
                current_name.clone()
            };
            tools.push(DvlTool {
                name,
                description: current_desc,
                source_path,
                inputs: current_inputs,
            });
        }
    }

    tools
}

/// Extract a field value from a TOML inline table like { type = "number", description = "..." }
fn extract_inline_field(inline: &str, field: &str) -> Option<String> {
    let search = format!("{field} = \"");
    if let Some(start) = inline.find(&search) {
        let after = &inline[start + search.len()..];
        if let Some(end) = after.find('"') {
            return Some(after[..end].to_string());
        }
    }
    None
}

fn dvl_tool_to_mcp_schema(tool: &DvlTool) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for (name, typ, desc) in &tool.inputs {
        let mut prop = serde_json::Map::new();
        prop.insert("type".to_string(), json!(typ));
        if !desc.is_empty() {
            prop.insert("description".to_string(), json!(desc));
        }
        properties.insert(name.clone(), Value::Object(prop));
        required.push(json!(name));
    }
    // If no inputs defined, accept any object
    if properties.is_empty() {
        json!({
            "name": tool.name,
            "description": tool.description,
            "inputSchema": {
                "type": "object",
                "additionalProperties": true
            }
        })
    } else {
        json!({
            "name": tool.name,
            "description": tool.description,
            "inputSchema": {
                "type": "object",
                "properties": properties,
                "required": required
            }
        })
    }
}

fn mcp_run_dvl_tool(tool: &DvlTool, arguments: &Value) -> Value {
    let source = match fs::read_to_string(&tool.source_path) {
        Ok(s) => s,
        Err(e) => {
            return json!([{
                "type": "text",
                "text": format!("Failed to read {}: {e}", tool.source_path.display())
            }])
        }
    };
    let source_path = tool.source_path.to_string_lossy().to_string();
    let json_str = match compile_source_to_json(
        &source,
        CompileOptions {
            source_path: Some(source_path.clone()),
            search_paths: devlish_search_paths_for(Some(tool.source_path.as_path())),
        },
    ) {
        Ok(j) => j,
        Err(error) => return json!([{"type": "text", "text": format!("Compile error: {error}")}]),
    };
    let package: Value = match serde_json::from_str(&json_str) {
        Ok(p) => p,
        Err(error) => return json!([{"type": "text", "text": format!("Internal error: {error}")}]),
    };
    let input = arguments.clone();
    let mut host = NativeHost {
        credentials: CredentialStore::new(&[], Some(tool.source_path.as_path())),
        audit_log: None,
    };
    match Vm::new(package, input) {
        Err(error) => json!([{"type": "text", "text": format!("VM error: {}", error.message)}]),
        Ok(mut vm) => match vm.run(&mut host) {
            Ok(result) => {
                // If program used Respond, return just the response value
                let responded = result
                    .get("responded")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if responded {
                    let response = result.get("response").cloned().unwrap_or(Value::Null);
                    let text = serde_json::to_string_pretty(&response).unwrap_or_default();
                    json!([{"type": "text", "text": text}])
                } else {
                    let text = serde_json::to_string_pretty(&result).unwrap_or_default();
                    json!([{"type": "text", "text": text}])
                }
            }
            Err(error) => {
                // If error is structured JSON (from Fail with record), return it
                if let Ok(structured) = serde_json::from_str::<Value>(&error.message) {
                    let text = serde_json::to_string_pretty(&structured).unwrap_or_default();
                    json!([{"type": "text", "text": text, "isError": true}])
                } else {
                    json!([{"type": "text", "text": format!("Runtime error: {}", error.message)}])
                }
            }
        },
    }
}

fn run_mcp(args: Vec<String>) -> Result<(), String> {
    use std::io::{self, BufRead, Write};

    // Parse --tools-dir arguments
    let mut tools_dirs: Vec<PathBuf> = Vec::new();
    let mut i = 1; // skip "mcp"
    while i < args.len() {
        if args[i] == "--tools-dir" {
            if let Some(dir) = args.get(i + 1) {
                tools_dirs.push(PathBuf::from(dir));
                i += 2;
                continue;
            }
        }
        i += 1;
    }

    // Add default tools directory
    if let Some(home) = env::var_os("HOME") {
        let default_dir = PathBuf::from(home).join(".devlish").join("tools");
        if default_dir.is_dir() {
            tools_dirs.push(default_dir);
        }
    }

    // Discover tools from all directories
    let mut dvl_tools: Vec<DvlTool> = Vec::new();
    for dir in &tools_dirs {
        dvl_tools.extend(discover_tools_from_dir(dir));
    }

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    // Built-in tool definitions
    let builtin_tools = json!([
        {
            "name": "compile",
            "description": "Compile a .dvl source string to bytecode JSON",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "Devlish source code" },
                    "source_path": { "type": "string", "description": "Optional source file path for import resolution" }
                },
                "required": ["source"]
            }
        },
        {
            "name": "run",
            "description": "Compile and run a .dvl source string, returning execution results",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "Devlish source code" },
                    "input": { "type": "object", "description": "Input variables as JSON object" },
                    "source_path": { "type": "string", "description": "Optional source file path for import resolution" }
                },
                "required": ["source"]
            }
        },
        {
            "name": "validate",
            "description": "Check if Devlish source code is syntactically valid",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "Devlish source code" }
                },
                "required": ["source"]
            }
        },
        {
            "name": "lint",
            "description": "Lint Devlish source code and return structured diagnostics",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "source": { "type": "string", "description": "Devlish source code" }
                },
                "required": ["source"]
            }
        }
    ]);

    for line in stdin.lock().lines() {
        let line = line.map_err(|e| format!("stdin read error: {e}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");

        let response = match method {
            "initialize" => {
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": { "tools": {} },
                        "serverInfo": {
                            "name": "devlish",
                            "version": VERSION
                        }
                    }
                })
            }
            "notifications/initialized" => continue,
            "tools/list" => {
                let mut all_tools = builtin_tools.as_array().cloned().unwrap_or_default();
                for tool in &dvl_tools {
                    all_tools.push(dvl_tool_to_mcp_schema(tool));
                }
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "tools": all_tools
                    }
                })
            }
            "tools/call" => {
                let params = request.get("params").cloned().unwrap_or(json!({}));
                let tool_name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

                let tool_result = match tool_name {
                    "compile" => mcp_compile(&arguments),
                    "run" => mcp_run(&arguments),
                    "validate" | "lint" => mcp_lint(&arguments),
                    _ => {
                        // Check discovered .dvl tools
                        if let Some(tool) = dvl_tools.iter().find(|t| t.name == tool_name) {
                            mcp_run_dvl_tool(tool, &arguments)
                        } else {
                            json!([{"type": "text", "text": format!("Unknown tool: {tool_name}")}])
                        }
                    }
                };

                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "content": tool_result
                    }
                })
            }
            _ => {
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": format!("Method not found: {method}")
                    }
                })
            }
        };

        let out = serde_json::to_string(&response).unwrap_or_default();
        writeln!(stdout, "{out}").map_err(|e| format!("stdout write error: {e}"))?;
        stdout
            .flush()
            .map_err(|e| format!("stdout flush error: {e}"))?;
    }

    Ok(())
}

fn mcp_compile(args: &Value) -> Value {
    let source = args.get("source").and_then(Value::as_str).unwrap_or("");
    let source_path = args
        .get("source_path")
        .and_then(Value::as_str)
        .map(String::from);
    match compile_source_to_json(
        source,
        CompileOptions {
            source_path,
            search_paths: devlish_search_paths_for(None),
        },
    ) {
        Ok(bytecode_json) => json!([{"type": "text", "text": bytecode_json}]),
        Err(error) => json!([{"type": "text", "text": format!("Compile error: {error}")}]),
    }
}

fn mcp_run(args: &Value) -> Value {
    let source = args.get("source").and_then(Value::as_str).unwrap_or("");
    let source_path = args
        .get("source_path")
        .and_then(Value::as_str)
        .map(String::from);
    let input = args.get("input").cloned().unwrap_or(json!({}));

    let json_str = match compile_source_to_json(
        source,
        CompileOptions {
            source_path,
            search_paths: devlish_search_paths_for(None),
        },
    ) {
        Ok(j) => j,
        Err(error) => return json!([{"type": "text", "text": format!("Compile error: {error}")}]),
    };

    let package: Value = match serde_json::from_str(&json_str) {
        Ok(p) => p,
        Err(error) => return json!([{"type": "text", "text": format!("Internal error: {error}")}]),
    };

    let source_file = args
        .get("source_path")
        .and_then(Value::as_str)
        .map(Path::new);
    let mut host = NativeHost {
        credentials: CredentialStore::new(&[], source_file),
        audit_log: None,
    };
    match Vm::new(package, input) {
        Err(error) => json!([{"type": "text", "text": format!("VM error: {}", error.message)}]),
        Ok(mut vm) => match vm.run(&mut host) {
            Ok(result) => {
                let text = serde_json::to_string_pretty(&result).unwrap_or_default();
                json!([{"type": "text", "text": text}])
            }
            Err(error) => {
                json!([{"type": "text", "text": format!("Runtime error: {}", error.message)}])
            }
        },
    }
}

fn mcp_lint(args: &Value) -> Value {
    let source = args.get("source").and_then(Value::as_str).unwrap_or("");
    match compile_source_to_json(
        source,
        CompileOptions {
            source_path: None,
            search_paths: vec![],
        },
    ) {
        Ok(_) => {
            // Compilation succeeded; surface the same non-fatal lint findings the
            // CLI reports so the MCP lint tool does not diverge (DEVL-127).
            let diagnostics: Vec<Value> = match devlish_core::lint_source(
                source,
                CompileOptions {
                    source_path: None,
                    search_paths: vec![],
                },
            ) {
                Ok(warnings) => warnings
                    .iter()
                    .map(|w| {
                        json!({
                            "line": w.line,
                            "severity": "warning",
                            "message": w.message,
                            "source_text": w.source_text
                        })
                    })
                    .collect(),
                Err(_) => Vec::new(),
            };
            let result = json!({
                "valid": true,
                "diagnostics": diagnostics
            });
            json!([{"type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_default()}])
        }
        Err(error) => {
            let diagnostics: Vec<Value> = error
                .diagnostics
                .iter()
                .map(|d| {
                    json!({
                        "line": d.line,
                        "severity": "error",
                        "message": d.message,
                        "source_text": d.source_text
                    })
                })
                .collect();
            let result = json!({
                "valid": false,
                "diagnostics": diagnostics
            });
            json!([{"type": "text", "text": serde_json::to_string_pretty(&result).unwrap_or_default()}])
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CompileConfig {
    input: PathBuf,
    output: Option<PathBuf>,
}

impl CompileConfig {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut input = None;
        let mut output = None;
        let mut target = "bytecode".to_string();
        let mut index = 1usize;
        while index < args.len() {
            match args[index].as_str() {
                "--output" | "-o" => {
                    index += 1;
                    output = Some(PathBuf::from(
                        args.get(index)
                            .ok_or_else(|| "--output requires a path".to_string())?,
                    ));
                }
                "--target" => {
                    index += 1;
                    target = args
                        .get(index)
                        .ok_or_else(|| "--target requires a value".to_string())?
                        .to_string();
                }
                value if value.starts_with('-') => return Err(format!("unknown option: {value}")),
                value => {
                    if input.is_some() {
                        return Err(format!("unexpected extra argument: {value}"));
                    }
                    input = Some(PathBuf::from(value));
                }
            }
            index += 1;
        }

        if target != "bytecode" {
            return Err(format!(
                "unsupported native compile target: {target}. Only `bytecode` is available"
            ));
        }

        Ok(Self {
            input: input.ok_or_else(compile_usage)?,
            output,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RunConfig {
    input: PathBuf,
    /// Additional rule versions, only allowed together with `--as-of`.
    extra_inputs: Vec<PathBuf>,
    /// Effective date (`YYYY-MM-DD`); when set, run the version in force on it.
    as_of: Option<String>,
    input_json: Option<String>,
    method: Option<String>,
    test_mode: bool,
    quiet: bool,
    env_overrides: Vec<(String, String)>,
    /// Append audit records for governed runs to this JSONL log
    /// (`--audit-log`; falls back to `DEVLISH_AUDIT_LOG`).
    audit_log: Option<PathBuf>,
    /// Archive the input, bytecode, and every effect exchange for governed
    /// runs into this directory, enabling `devlish replay`.
    journal: Option<PathBuf>,
    /// Refuse to execute any artifact whose hash is not a published release
    /// in this registry (`--governed`).
    governed: Option<PathBuf>,
}

impl RunConfig {
    fn parse(args: Vec<String>) -> Result<Self, String> {
        let mut inputs: Vec<PathBuf> = Vec::new();
        let mut as_of = None;
        let mut input_json = None;
        let mut method = None;
        let mut test_mode = false;
        let mut quiet = false;
        let mut env_overrides = Vec::new();
        let mut audit_log = None;
        let mut journal = None;
        let mut governed = None;
        let mut index = 1usize;
        while index < args.len() {
            match args[index].as_str() {
                "--as-of" => {
                    index += 1;
                    as_of = Some(
                        args.get(index)
                            .ok_or_else(|| "--as-of requires a YYYY-MM-DD date".to_string())?
                            .to_string(),
                    );
                }
                "--input" => {
                    index += 1;
                    input_json = Some(
                        args.get(index)
                            .ok_or_else(|| "--input requires a JSON string".to_string())?
                            .to_string(),
                    );
                }
                "--method" => {
                    index += 1;
                    method = Some(
                        args.get(index)
                            .ok_or_else(|| "--method requires a method name".to_string())?
                            .to_string(),
                    );
                }
                "--env" => {
                    index += 1;
                    let pair = args
                        .get(index)
                        .ok_or_else(|| "--env requires KEY=VALUE".to_string())?;
                    let (key, value) = pair
                        .split_once('=')
                        .ok_or_else(|| format!("--env value must be KEY=VALUE, got: {pair}"))?;
                    env_overrides.push((key.to_string(), value.to_string()));
                }
                "--audit-log" => {
                    index += 1;
                    audit_log =
                        Some(PathBuf::from(args.get(index).ok_or_else(|| {
                            "--audit-log requires a file path".to_string()
                        })?));
                }
                "--journal" => {
                    index += 1;
                    journal =
                        Some(PathBuf::from(args.get(index).ok_or_else(|| {
                            "--journal requires a directory path".to_string()
                        })?));
                }
                "--governed" => {
                    index += 1;
                    governed =
                        Some(PathBuf::from(args.get(index).ok_or_else(|| {
                            "--governed requires a registry path".to_string()
                        })?));
                }
                "--test" => {
                    test_mode = true;
                }
                "--quiet" => {
                    quiet = true;
                }
                value if value.starts_with('-') => return Err(format!("unknown option: {value}")),
                value => {
                    inputs.push(PathBuf::from(value));
                }
            }
            index += 1;
        }

        if inputs.is_empty() {
            return Err(run_usage());
        }
        if as_of.is_none() && inputs.len() > 1 {
            return Err(
                "multiple input files are only allowed with --as-of (to pick the version in force)"
                    .to_string(),
            );
        }
        let input = inputs.remove(0);
        Ok(Self {
            input,
            extra_inputs: inputs,
            as_of,
            input_json,
            method,
            test_mode,
            quiet,
            env_overrides,
            audit_log,
            journal,
            governed,
        })
    }
}

fn usage() -> String {
    "Usage: devlish-core <command> [options]\n\nRun 'devlish-core help' for available commands."
        .to_string()
}

fn compile_usage() -> String {
    "Usage: devlish-core compile <file.dvl> --target bytecode [--output file.dvlc.json]".to_string()
}

fn run_usage() -> String {
    "Usage: devlish-core run <file> [<file>...] [--input '{\"key\":\"value\"}'] [--method <name>] [--as-of YYYY-MM-DD] [--audit-log <path>] [--quiet]"
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn versioned(id: &str, version: &str, from: Option<&str>, until: Option<&str>) -> Value {
        let mut rule = serde_json::Map::new();
        rule.insert("id".into(), json!(id));
        rule.insert("version".into(), json!(version));
        if let Some(f) = from {
            rule.insert("effective_from".into(), json!(f));
        }
        if let Some(u) = until {
            rule.insert("effective_until".into(), json!(u));
        }
        json!({ "manifest": { "rule": Value::Object(rule) } })
    }

    #[test]
    fn select_effective_version_picks_the_version_in_force() {
        let v1 = versioned(
            "credit.dti",
            "1.0.0",
            Some("2025-01-01"),
            Some("2025-12-31"),
        );
        let v2 = versioned("credit.dti", "2.0.0", Some("2026-01-01"), None);
        let versions = vec![(PathBuf::from("v1.dvl"), v1), (PathBuf::from("v2.dvl"), v2)];
        let (path, _) = select_effective_version(versions.clone(), "2025-06-15").unwrap();
        assert_eq!(path, PathBuf::from("v1.dvl"));
        let (path, _) = select_effective_version(versions, "2030-01-01").unwrap();
        assert_eq!(
            path,
            PathBuf::from("v2.dvl"),
            "open-ended window stays in force"
        );
    }

    #[test]
    fn select_effective_version_errors_when_none_in_force() {
        let v1 = versioned(
            "credit.dti",
            "1.0.0",
            Some("2025-01-01"),
            Some("2025-12-31"),
        );
        let err = select_effective_version(vec![(PathBuf::from("v1.dvl"), v1)], "2024-01-01")
            .expect_err("no version in force");
        assert!(
            err.contains("no version of credit.dti is in force"),
            "got: {err}"
        );
    }

    #[test]
    fn select_effective_version_rejects_overlap_and_mixed_ids() {
        let a = versioned(
            "credit.dti",
            "1.0.0",
            Some("2026-01-01"),
            Some("2026-12-31"),
        );
        let b = versioned(
            "credit.dti",
            "1.5.0",
            Some("2026-06-01"),
            Some("2027-06-01"),
        );
        let overlap = select_effective_version(
            vec![(PathBuf::from("a"), a), (PathBuf::from("b"), b)],
            "2026-08-01",
        )
        .expect_err("overlap");
        assert!(overlap.contains("multiple versions"), "got: {overlap}");

        let c = versioned("credit.dti", "1.0.0", Some("2026-01-01"), None);
        let d = versioned("pricing.tier", "1.0.0", Some("2026-01-01"), None);
        let mixed = select_effective_version(
            vec![(PathBuf::from("c"), c), (PathBuf::from("d"), d)],
            "2026-06-01",
        )
        .expect_err("mixed ids");
        assert!(mixed.contains("one rule id"), "got: {mixed}");
    }

    #[test]
    fn select_effective_version_rejects_impossible_as_of_date() {
        let v1 = versioned("credit.dti", "1.0.0", Some("2026-01-01"), None);
        for bad in ["2026-02-31", "2026-13-01", "garbage", "2026-1-1"] {
            let err = select_effective_version(vec![(PathBuf::from("v1.dvl"), v1.clone())], bad)
                .expect_err("invalid as-of rejected");
            assert!(
                err.contains("must be a real YYYY-MM-DD"),
                "for {bad}, got: {err}"
            );
        }
    }

    #[test]
    fn select_effective_version_rejects_ungoverned() {
        let ungoverned = json!({ "manifest": { "permissions": [] } });
        let err =
            select_effective_version(vec![(PathBuf::from("plain.dvl"), ungoverned)], "2026-06-01")
                .expect_err("ungoverned");
        assert!(err.contains("needs governed rules"), "got: {err}");
    }

    #[test]
    fn run_config_rejects_multiple_inputs_without_as_of() {
        let err = RunConfig::parse(vec![
            "run".to_string(),
            "a.dvl".to_string(),
            "b.dvl".to_string(),
        ])
        .expect_err("multiple inputs need --as-of");
        assert!(err.contains("--as-of"), "got: {err}");
    }

    #[test]
    fn run_config_parses_as_of() {
        let config = RunConfig::parse(vec![
            "run".to_string(),
            "a.dvl".to_string(),
            "b.dvl".to_string(),
            "--as-of".to_string(),
            "2026-06-01".to_string(),
        ])
        .expect("valid as-of args");
        assert_eq!(config.as_of.as_deref(), Some("2026-06-01"));
        assert_eq!(config.extra_inputs, vec![PathBuf::from("b.dvl")]);
    }

    fn sample_case(name: &str, passed: bool) -> Value {
        json!({
            "name": name,
            "passed": passed,
            "input_sha256": sha256_hex(name.as_bytes()),
            "output_sha256": sha256_hex(b"out"),
            "expected_sha256": sha256_hex(b"out"),
        })
    }

    #[test]
    fn evidence_report_hash_is_recomputable_and_deterministic() {
        let cases = vec![sample_case("a", true), sample_case("b", true)];
        let report = build_evidence_report(
            "credit.dti",
            "1.0.0",
            "artifacthash",
            Some("0.1.0"),
            cases.clone(),
            1000,
        );
        assert_eq!(report["totals"]["total"], json!(2));
        assert_eq!(report["totals"]["passed"], json!(2));

        // A verifier removes report_sha256, re-serializes, and recomputes.
        let claimed = report["report_sha256"].as_str().unwrap().to_string();
        let mut body = report.clone();
        body.as_object_mut().unwrap().remove("report_sha256");
        assert_eq!(sha256_hex(&serde_json::to_vec(&body).unwrap()), claimed);

        // Same inputs + timestamp -> identical report hash.
        let again = build_evidence_report(
            "credit.dti",
            "1.0.0",
            "artifacthash",
            Some("0.1.0"),
            cases,
            1000,
        );
        assert_eq!(again["report_sha256"], report["report_sha256"]);
    }

    #[test]
    fn evidence_report_hash_changes_with_artifact_or_case() {
        let cases = vec![sample_case("a", true)];
        let base = build_evidence_report("credit.dti", "1.0.0", "hashA", None, cases.clone(), 1000);
        let diff_artifact =
            build_evidence_report("credit.dti", "1.0.0", "hashB", None, cases, 1000);
        assert_ne!(base["report_sha256"], diff_artifact["report_sha256"]);

        let diff_case = build_evidence_report(
            "credit.dti",
            "1.0.0",
            "hashA",
            None,
            vec![sample_case("a", false)],
            1000,
        );
        assert_ne!(base["report_sha256"], diff_case["report_sha256"]);
    }

    #[test]
    fn run_case_capture_returns_the_respond_value() {
        let json_str = compile_source_to_json(
            "Respond with income",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect("compiles");
        let package: Value = serde_json::from_str(&json_str).unwrap();
        let output = run_case_capture(&package, &json!({ "income": 5000 })).unwrap();
        assert_eq!(output, json!(5000));
    }

    #[test]
    fn load_evidence_cases_requires_expected() {
        let dir = std::env::temp_dir().join("devlish_test_evidence_cases");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.cases.json");
        std::fs::write(&path, r#"[{ "name": "x", "input": {} }]"#).unwrap();
        let err = load_evidence_cases(&path).expect_err("missing expected");
        assert!(err.contains("missing an 'expected'"), "got: {err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_evidence_cases_rejects_empty_and_duplicate() {
        let dir = std::env::temp_dir().join("devlish_test_evidence_edge");
        std::fs::create_dir_all(&dir).unwrap();
        let empty = dir.join("empty.cases.json");
        std::fs::write(&empty, "[]").unwrap();
        assert!(load_evidence_cases(&empty)
            .expect_err("empty rejected")
            .contains("no cases"));
        let dup = dir.join("dup.cases.json");
        std::fs::write(
            &dup,
            r#"[{"name":"a","expected":1},{"name":"a","expected":2}]"#,
        )
        .unwrap();
        assert!(load_evidence_cases(&dup)
            .expect_err("duplicate rejected")
            .contains("duplicate case name"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn run_case_capture_fails_when_rule_does_not_respond() {
        let json_str = compile_source_to_json(
            "x equals 1",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect("compiles");
        let package: Value = serde_json::from_str(&json_str).unwrap();
        let err = run_case_capture(&package, &json!({})).expect_err("no respond");
        assert!(err.contains("did not produce"), "got: {err}");
    }

    #[test]
    fn verify_evidence_report_detects_tampering() {
        let dir = std::env::temp_dir().join("devlish_test_evidence_verify");
        std::fs::create_dir_all(&dir).unwrap();
        let report = build_evidence_report(
            "credit.dti",
            "1.0.0",
            "hashA",
            None,
            vec![sample_case("a", true)],
            1000,
        );
        let path = dir.join("evidence.json");
        std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
        verify_evidence_report(&path).expect("untampered report verifies");

        // Flip a case's pass flag without updating report_sha256.
        let mut tampered = report;
        tampered["cases"][0]["passed"] = json!(false);
        std::fs::write(&path, serde_json::to_string_pretty(&tampered).unwrap()).unwrap();
        assert!(verify_evidence_report(&path)
            .expect_err("tamper detected")
            .contains("TAMPERED"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn run_config_parses_quiet_flag() {
        let config = RunConfig::parse(vec![
            "run".to_string(),
            "file.dvl".to_string(),
            "--quiet".to_string(),
        ])
        .expect("valid run args");
        assert!(config.quiet);

        let config = RunConfig::parse(vec!["run".to_string(), "file.dvl".to_string()])
            .expect("valid run args");
        assert!(!config.quiet);
    }

    #[test]
    fn toml_pending_parameter_is_flushed_at_non_tool_section() {
        let dir = std::env::temp_dir().join("devlish_test_toml_flush_section");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("tool.dvl"), "x equals 1\n").unwrap();

        let content = r#"
[[tools]]
name = "calc"
source = "tool.dvl"

[tools.parameters.amount]
type = "number"
description = "Amount"

[metadata]
author = "someone"
"#;
        let tools = parse_tools_from_toml(content, &dir);
        assert_eq!(tools.len(), 1);
        assert_eq!(
            tools[0].inputs,
            vec![(
                "amount".to_string(),
                "number".to_string(),
                "Amount".to_string()
            )]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn toml_parameter_subsections_are_parsed() {
        let dir = std::env::temp_dir().join("devlish_test_toml_subsections");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("tool.dvl"), "x equals 1\n").unwrap();

        let content = r#"
[[tools]]
name = "calc"
description = "A calculator"
source = "tool.dvl"

[tools.parameters.amount]
type = "number"
description = "Amount to process"

[tools.parameters.label]
description = "A label"
"#;
        let tools = parse_tools_from_toml(content, &dir);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "calc");
        assert_eq!(
            tools[0].inputs,
            vec![
                (
                    "amount".to_string(),
                    "number".to_string(),
                    "Amount to process".to_string()
                ),
                (
                    "label".to_string(),
                    "string".to_string(),
                    "A label".to_string()
                ),
            ]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn toml_inline_inputs_still_parse() {
        let dir = std::env::temp_dir().join("devlish_test_toml_inline");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("tool.dvl"), "x equals 1\n").unwrap();

        let content = r#"
[[tools]]
name = "inline_tool"
source = "tool.dvl"

[tools.inputs]
count = { type = "number", description = "How many" }
"#;
        let tools = parse_tools_from_toml(content, &dir);
        assert_eq!(tools.len(), 1);
        assert_eq!(
            tools[0].inputs,
            vec![(
                "count".to_string(),
                "number".to_string(),
                "How many".to_string()
            )]
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn toml_pending_parameter_is_flushed_at_tool_boundary() {
        let dir = std::env::temp_dir().join("devlish_test_toml_boundary");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.dvl"), "x equals 1\n").unwrap();
        std::fs::write(dir.join("b.dvl"), "x equals 2\n").unwrap();

        let content = r#"
[[tools]]
name = "first"
source = "a.dvl"

[tools.parameters.alpha]
type = "text"

[[tools]]
name = "second"
source = "b.dvl"
"#;
        let tools = parse_tools_from_toml(content, &dir);
        assert_eq!(tools.len(), 2);
        assert_eq!(
            tools[0].inputs,
            vec![("alpha".to_string(), "text".to_string(), String::new())]
        );
        assert!(tools[1].inputs.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    fn sample_audit_record(name: &str) -> Value {
        json!({
            "artifact_sha256": sha256_hex(b"artifact"),
            "input_sha256": sha256_hex(name.as_bytes()),
            "instruction_count": 3,
            "output_sha256": sha256_hex(b"out"),
            "rule_id": "pricing.tier",
            "rule_version": "1.0.0",
            "success": true,
        })
    }

    #[test]
    fn audit_log_chains_records_across_separate_writers() {
        let path = std::env::temp_dir().join(format!("devlish-audit-{}.jsonl", now_unix()));
        std::fs::remove_file(&path).ok();

        // Two writers on the same path simulate two CLI invocations: the
        // second must chain to the first's record.
        AuditLogWriter::new(path.clone())
            .append(&sample_audit_record("one"))
            .expect("first append");
        AuditLogWriter::new(path.clone())
            .append(&sample_audit_record("two"))
            .expect("second append");

        let content = std::fs::read_to_string(&path).expect("log readable");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);

        let first: Value = serde_json::from_str(lines[0]).unwrap();
        let second: Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(first["prev_sha256"], Value::Null);
        assert_eq!(
            second["prev_sha256"],
            json!(sha256_hex(lines[0].as_bytes()))
        );
        assert_eq!(first["runtime"]["kind"], json!("native"));
        assert_eq!(first["runtime"]["version"], json!(VERSION));
        assert!(first["timestamp"].is_u64());

        assert_eq!(verify_audit_log(&content), Ok(2));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn audit_verify_detects_modified_record() {
        let path = std::env::temp_dir().join(format!("devlish-audit-mod-{}.jsonl", now_unix()));
        std::fs::remove_file(&path).ok();
        let mut writer = AuditLogWriter::new(path.clone());
        for name in ["one", "two", "three"] {
            writer.append(&sample_audit_record(name)).expect("append");
        }
        let content = std::fs::read_to_string(&path).expect("log readable");
        std::fs::remove_file(&path).ok();

        let tampered = content.replace("\"success\":true", "\"success\":false");
        assert_ne!(tampered, content, "tamper must change the log");
        let error = verify_audit_log(&tampered).expect_err("tamper detected");
        assert!(
            error.contains("audit chain broken at line 2"),
            "got: {error}"
        );
    }

    #[test]
    fn audit_verify_detects_deleted_middle_record() {
        let path = std::env::temp_dir().join(format!("devlish-audit-del-{}.jsonl", now_unix()));
        std::fs::remove_file(&path).ok();
        let mut writer = AuditLogWriter::new(path.clone());
        for name in ["one", "two", "three"] {
            writer.append(&sample_audit_record(name)).expect("append");
        }
        let content = std::fs::read_to_string(&path).expect("log readable");
        std::fs::remove_file(&path).ok();

        let lines: Vec<&str> = content.lines().collect();
        let truncated = format!("{}\n{}\n", lines[0], lines[2]);
        let error = verify_audit_log(&truncated).expect_err("deletion detected");
        assert!(
            error.contains("audit chain broken at line 2"),
            "got: {error}"
        );
    }

    #[test]
    fn audit_verify_accepts_empty_log() {
        assert_eq!(verify_audit_log(""), Ok(0));
    }

    /// Canned host for journaling tests: deterministic responses, no I/O.
    struct StubHost;
    impl HostEffects for StubHost {
        fn emit_event(&mut self, _event: &Value) {}
        fn write_file(&mut self, _request: &Value) -> Result<(), String> {
            Ok(())
        }
        fn read_file(&mut self, _request: &Value) -> Result<Value, String> {
            Ok(json!("file-body"))
        }
        fn http_request(
            &mut self,
            _method: &str,
            _url: &str,
            _body: &Value,
            _headers: &Value,
        ) -> Result<Value, String> {
            Ok(json!({ "status": 200, "body": { "rate": 0.07 } }))
        }
    }

    #[test]
    fn journaling_host_records_exchanges_and_links_the_audit_record() {
        struct CapturingInner {
            record: Option<Value>,
        }
        impl HostEffects for CapturingInner {
            fn emit_event(&mut self, _event: &Value) {}
            fn write_file(&mut self, _request: &Value) -> Result<(), String> {
                Ok(())
            }
            fn read_file(&mut self, _request: &Value) -> Result<Value, String> {
                Ok(json!("file-body"))
            }
            fn http_request(
                &mut self,
                _method: &str,
                _url: &str,
                _body: &Value,
                _headers: &Value,
            ) -> Result<Value, String> {
                Ok(json!({ "status": 200 }))
            }
            fn audit_record(&mut self, record: &Value) -> Result<(), String> {
                self.record = Some(record.clone());
                Ok(())
            }
        }

        let dir = std::env::temp_dir().join(format!("devlish-journal-{}", now_unix()));
        std::fs::remove_dir_all(&dir).ok();
        let mut host = JournalingHost::new(
            CapturingInner { record: None },
            dir.clone(),
            json!({ "format": "devlish-bytecode" }),
            json!({ "amount": 100 }),
            false,
        );

        host.read_file(&json!({ "path": "config.json" })).unwrap();
        host.http_request(
            "GET",
            "https://api.example.com/rate",
            &Value::Null,
            &json!({}),
        )
        .unwrap();
        host.audit_record(&json!({ "rule_id": "pricing.tier", "success": true }))
            .unwrap();

        let record = host.inner.record.expect("record forwarded to inner host");
        let hash = record["journal_sha256"].as_str().expect("journal linked");
        let journal_path = dir.join(format!("{hash}.json"));
        let bytes = std::fs::read(&journal_path).expect("journal written");
        assert_eq!(sha256_hex(&bytes), hash, "attachment is content-addressed");

        let journal: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(journal["format"], json!("devlish-journal"));
        assert_eq!(journal["input"], json!({ "amount": 100 }));
        let effects = journal["effects"].as_array().unwrap();
        assert_eq!(effects.len(), 2);
        assert_eq!(effects[0]["kind"], json!("read_file"));
        assert_eq!(effects[0]["response"]["ok"], json!("file-body"));
        assert_eq!(effects[1]["kind"], json!("http_request"));
        assert_eq!(
            effects[1]["request"]["url"],
            json!("https://api.example.com/rate")
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn replay_host_feeds_journaled_responses_and_detects_divergence() {
        let effects = vec![
            json!({ "kind": "read_file", "request": { "path": "a.json" }, "response": { "ok": "body" } }),
            json!({ "kind": "http_request", "request": { "method": "GET", "url": "https://x", "body": null, "headers": {} }, "response": { "err": "boom" } }),
        ];

        let mut host = ReplayHost::new(effects.clone());
        assert_eq!(
            host.read_file(&json!({ "path": "a.json" })),
            Ok(json!("body"))
        );
        assert_eq!(
            host.http_request("GET", "https://x", &Value::Null, &json!({})),
            Err("boom".to_string())
        );
        assert!(host.fully_consumed().is_ok());

        // A request that differs from the journal is a divergence, not a guess.
        let mut host = ReplayHost::new(effects);
        let error = host
            .read_file(&json!({ "path": "OTHER.json" }))
            .expect_err("diverged");
        assert!(
            error.contains("replay diverged at effect #1"),
            "got: {error}"
        );
    }

    #[test]
    fn journaled_run_replays_offline_and_tampered_journal_is_detected() {
        let base = std::env::temp_dir().join(format!("devlish-replay-{}", now_unix()));
        std::fs::remove_dir_all(&base).ok();
        std::fs::create_dir_all(&base).unwrap();
        let rule = base.join("rule.dvl");
        let data = base.join("config.json");
        let log = base.join("audit.jsonl");
        let attachments = base.join("attachments");

        std::fs::write(&data, r#"{ "threshold": 250 }"#).unwrap();
        std::fs::write(
            &rule,
            format!(
                "Rule:\n  id: pricing.lookup\n  version: 1.0.0\n\nRead JSON from \"{}\" as config\nthreshold equals threshold of config\nIf threshold is greater than 100:\n  Respond with record with \"high\" as band and threshold as threshold\nRespond with record with \"low\" as band and threshold as threshold\n",
                data.display()
            ),
        )
        .unwrap();

        run_execute(vec![
            "run".to_string(),
            rule.to_string_lossy().to_string(),
            "--quiet".to_string(),
            "--audit-log".to_string(),
            log.to_string_lossy().to_string(),
            "--journal".to_string(),
            attachments.to_string_lossy().to_string(),
        ])
        .expect("journaled run succeeds");

        // Offline: the replay must not touch the filesystem effect, so delete
        // the data file before replaying.
        std::fs::remove_file(&data).unwrap();
        run_replay(vec![
            "replay".to_string(),
            log.to_string_lossy().to_string(),
            "--journal".to_string(),
            attachments.to_string_lossy().to_string(),
        ])
        .expect("replay reproduces the recorded output offline");

        // Change the archived effect response: the replay output must change
        // (and therefore mismatch the record), proving the journal drives it.
        let line = std::fs::read_to_string(&log).unwrap();
        let mut record: Value = serde_json::from_str(line.lines().next().unwrap()).unwrap();
        let old_hash = record["journal_sha256"].as_str().unwrap().to_string();
        let journal_bytes = std::fs::read(attachments.join(format!("{old_hash}.json"))).unwrap();
        let mut journal: Value = serde_json::from_slice(&journal_bytes).unwrap();
        journal["effects"][0]["response"]["ok"] = json!("{ \"threshold\": 5 }");
        let new_bytes = serde_json::to_vec(&journal).unwrap();
        let new_hash = sha256_hex(&new_bytes);
        std::fs::write(attachments.join(format!("{new_hash}.json")), &new_bytes).unwrap();
        record["journal_sha256"] = json!(new_hash);
        std::fs::write(
            &log,
            format!("{}\n", serde_json::to_string(&record).unwrap()),
        )
        .unwrap();

        let error = run_replay(vec![
            "replay".to_string(),
            log.to_string_lossy().to_string(),
            "--journal".to_string(),
            attachments.to_string_lossy().to_string(),
        ])
        .expect_err("tampered effect response cannot reproduce the recorded output");
        assert!(
            error.contains("REPLAY MISMATCH") && error.contains("replay diverged"),
            "tampered response must be reported as the divergence it is, got: {error}"
        );

        // A bit-flipped attachment fails the content-address check outright.
        let mut corrupted = new_bytes.clone();
        let last = corrupted.len() - 2;
        corrupted[last] = corrupted[last].wrapping_add(1);
        std::fs::write(attachments.join(format!("{new_hash}.json")), &corrupted).unwrap();
        let error = run_replay(vec![
            "replay".to_string(),
            log.to_string_lossy().to_string(),
            "--journal".to_string(),
            attachments.to_string_lossy().to_string(),
        ])
        .expect_err("corrupted attachment detected");
        assert!(error.contains("content address"), "got: {error}");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn replay_detects_forged_output_hash() {
        let base = std::env::temp_dir().join(format!("devlish-forge-{}", now_unix()));
        std::fs::create_dir_all(&base).unwrap();
        let rule = base.join("rule.dvl");
        let log = base.join("audit.jsonl");
        let attachments = base.join("attachments");
        std::fs::write(
            &rule,
            "Rule:\n  id: forge.check\n  version: 1.0.0\n\nanswer equals 41 plus 1\nPrint answer\n",
        )
        .unwrap();
        run_execute(vec![
            "run".to_string(),
            rule.to_string_lossy().to_string(),
            "--quiet".to_string(),
            "--audit-log".to_string(),
            log.to_string_lossy().to_string(),
            "--journal".to_string(),
            attachments.to_string_lossy().to_string(),
        ])
        .expect("run succeeds");

        // Forge the recorded output hash: the journal replays fine, but the
        // reproduced output can no longer match the record.
        let line = std::fs::read_to_string(&log).unwrap();
        let mut record: Value = serde_json::from_str(line.lines().next().unwrap()).unwrap();
        record["output_sha256"] = json!("0".repeat(64));
        std::fs::write(
            &log,
            format!("{}\n", serde_json::to_string(&record).unwrap()),
        )
        .unwrap();

        let error = run_replay(vec![
            "replay".to_string(),
            log.to_string_lossy().to_string(),
            "--journal".to_string(),
            attachments.to_string_lossy().to_string(),
        ])
        .expect_err("forged output hash detected");
        assert!(
            error.contains("REPLAY MISMATCH") && error.contains("replayed output sha256"),
            "got: {error}"
        );
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn non_quiet_run_replays_with_events_in_the_envelope() {
        let base = std::env::temp_dir().join(format!("devlish-events-{}", now_unix()));
        std::fs::create_dir_all(&base).unwrap();
        let rule = base.join("rule.dvl");
        let log = base.join("audit.jsonl");
        let attachments = base.join("attachments");
        std::fs::write(
            &rule,
            "Rule:\n  id: events.check\n  version: 1.0.0\n\nanswer equals 2 times 21\nPrint answer\n",
        )
        .unwrap();
        // No --quiet: events land in the result envelope and the journal
        // must archive emit_events=true so replay hashes identically.
        run_execute(vec![
            "run".to_string(),
            rule.to_string_lossy().to_string(),
            "--audit-log".to_string(),
            log.to_string_lossy().to_string(),
            "--journal".to_string(),
            attachments.to_string_lossy().to_string(),
        ])
        .expect("run succeeds");
        run_replay(vec![
            "replay".to_string(),
            log.to_string_lossy().to_string(),
            "--journal".to_string(),
            attachments.to_string_lossy().to_string(),
        ])
        .expect("replay reproduces the event-bearing envelope");
        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn journal_on_ungoverned_rule_is_rejected() {
        let base = std::env::temp_dir().join(format!("devlish-ungoverned-{}", now_unix()));
        std::fs::create_dir_all(&base).unwrap();
        let rule = base.join("rule.dvl");
        std::fs::write(&rule, "answer equals 1 plus 1\nPrint answer\n").unwrap();
        let error = run_execute(vec![
            "run".to_string(),
            rule.to_string_lossy().to_string(),
            "--quiet".to_string(),
            "--audit-log".to_string(),
            base.join("a.jsonl").to_string_lossy().to_string(),
            "--journal".to_string(),
            base.join("att").to_string_lossy().to_string(),
        ])
        .expect_err("ungoverned journal rejected");
        assert!(error.contains("governed rule"), "got: {error}");
        std::fs::remove_dir_all(&base).ok();
    }

    fn release_fixture(base: &Path, version: &str, window: &str) -> PathBuf {
        let rule = base.join(format!("pricing_{}.dvl", version.replace('.', "_")));
        std::fs::write(
            &rule,
            format!(
                "Rule:\n  id: pricing.tier\n  version: {version}\n{window}\nIf amount is greater than 100:\n  Respond with \"high\"\nRespond with \"low\"\n"
            ),
        )
        .unwrap();
        let stem = rule.file_stem().unwrap().to_str().unwrap().to_string();
        std::fs::write(
            base.join(format!("{stem}.cases.json")),
            r#"[{"name":"high","input":{"amount":150},"expected":"high"},{"name":"low","input":{"amount":50},"expected":"low"}]"#,
        )
        .unwrap();
        rule
    }

    fn release_cmd(registry: &Path, parts: &[&str]) -> Result<(), String> {
        let mut args = vec!["release".to_string()];
        args.extend(parts.iter().map(|s| s.to_string()));
        args.push("--registry".to_string());
        args.push(registry.to_string_lossy().to_string());
        run_release(args)
    }

    #[test]
    fn release_lifecycle_end_to_end_with_governed_run() {
        let base = std::env::temp_dir().join(format!("devlish-release-{}", now_unix()));
        std::fs::create_dir_all(&base).unwrap();
        let registry = base.join("registry.json");
        let rule = release_fixture(&base, "1.0.0", "");
        let rule_str = rule.to_string_lossy().to_string();

        release_cmd(&registry, &["propose", &rule_str, "--author", "andrew"])
            .expect("propose succeeds");

        // Separation of duties: the author cannot approve their own release.
        let error = release_cmd(
            &registry,
            &["approve", "pricing.tier@1.0.0", "--approver", "andrew"],
        )
        .expect_err("self-approval refused");
        assert!(error.contains("separation of duties"), "got: {error}");

        // A draft cannot be published.
        let error = release_cmd(&registry, &["publish", "pricing.tier@1.0.0"])
            .expect_err("draft publish refused");
        assert!(error.contains("only an approved"), "got: {error}");

        release_cmd(
            &registry,
            &["approve", "pricing.tier@1.0.0", "--approver", "dana"],
        )
        .expect("second-party approval succeeds");
        release_cmd(&registry, &["publish", "pricing.tier@1.0.0"]).expect("publish succeeds");

        // Published artifact runs under --governed.
        run_execute(vec![
            "run".to_string(),
            rule_str.clone(),
            "--quiet".to_string(),
            "--input".to_string(),
            r#"{"amount": 150}"#.to_string(),
            "--governed".to_string(),
            registry.to_string_lossy().to_string(),
        ])
        .expect("published artifact runs under --governed");

        // Tampering with the rule changes its hash: refused at run.
        let source = std::fs::read_to_string(&rule).unwrap();
        std::fs::write(&rule, source.replace("greater than 100", "greater than 1")).unwrap();
        let error = run_execute(vec![
            "run".to_string(),
            rule_str.clone(),
            "--quiet".to_string(),
            "--governed".to_string(),
            registry.to_string_lossy().to_string(),
        ])
        .expect_err("tampered artifact refused");
        assert!(error.contains("not a published release"), "got: {error}");
        std::fs::write(&rule, source).unwrap();

        // Retire: the artifact no longer runs; rollback republishes it.
        release_cmd(&registry, &["retire", "pricing.tier@1.0.0"]).expect("retire succeeds");
        let error = run_execute(vec![
            "run".to_string(),
            rule_str.clone(),
            "--quiet".to_string(),
            "--governed".to_string(),
            registry.to_string_lossy().to_string(),
        ])
        .expect_err("retired artifact refused");
        assert!(error.contains("not a published release"), "got: {error}");

        release_cmd(&registry, &["publish", "pricing.tier@1.0.0"])
            .expect("rollback: republishing a retired release succeeds");
        run_execute(vec![
            "run".to_string(),
            rule_str,
            "--quiet".to_string(),
            "--input".to_string(),
            r#"{"amount": 150}"#.to_string(),
            "--governed".to_string(),
            registry.to_string_lossy().to_string(),
        ])
        .expect("rolled-back artifact runs again");

        // Duplicate propose for the same version is refused.
        let error = release_cmd(
            &registry,
            &[
                "propose",
                &release_fixture(&base, "1.0.0", "").to_string_lossy(),
                "--author",
                "andrew",
            ],
        )
        .expect_err("duplicate version refused");
        assert!(error.contains("already in the registry"), "got: {error}");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn publish_refuses_overlapping_effective_windows() {
        let base = std::env::temp_dir().join(format!("devlish-overlap-{}", now_unix()));
        std::fs::create_dir_all(&base).unwrap();
        let registry = base.join("registry.json");

        // v1 in force through 2026; v2 open-ended from mid-2026: overlap.
        let v1 = release_fixture(&base, "1.0.0", "  effective until 2026-12-31\n");
        let v2 = release_fixture(&base, "2.0.0", "  effective from 2026-06-01\n");
        for rule in [&v1, &v2] {
            release_cmd(
                &registry,
                &["propose", &rule.to_string_lossy(), "--author", "andrew"],
            )
            .expect("propose succeeds");
        }
        release_cmd(
            &registry,
            &["approve", "pricing.tier@1.0.0", "--approver", "dana"],
        )
        .unwrap();
        release_cmd(
            &registry,
            &["approve", "pricing.tier@2.0.0", "--approver", "dana"],
        )
        .unwrap();
        release_cmd(&registry, &["publish", "pricing.tier@1.0.0"]).unwrap();
        let error = release_cmd(&registry, &["publish", "pricing.tier@2.0.0"])
            .expect_err("overlapping window refused");
        assert!(error.contains("overlaps"), "got: {error}");

        // A disjoint window publishes cleanly alongside.
        let v3 = release_fixture(&base, "3.0.0", "  effective from 2027-01-01\n");
        release_cmd(
            &registry,
            &["propose", &v3.to_string_lossy(), "--author", "andrew"],
        )
        .unwrap();
        release_cmd(
            &registry,
            &["approve", "pricing.tier@3.0.0", "--approver", "dana"],
        )
        .unwrap();
        release_cmd(&registry, &["publish", "pricing.tier@3.0.0"])
            .expect("disjoint window publishes");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn registry_chain_detects_edited_events() {
        let base = std::env::temp_dir().join(format!("devlish-regchain-{}", now_unix()));
        std::fs::create_dir_all(&base).unwrap();
        let registry = base.join("registry.json");
        let rule = release_fixture(&base, "1.0.0", "");
        release_cmd(
            &registry,
            &["propose", &rule.to_string_lossy(), "--author", "andrew"],
        )
        .unwrap();
        release_cmd(
            &registry,
            &["approve", "pricing.tier@1.0.0", "--approver", "dana"],
        )
        .unwrap();

        // Rewriting history -- swapping the recorded author -- breaks the chain.
        let content = std::fs::read_to_string(&registry).unwrap();
        std::fs::write(&registry, content.replace("\"andrew\"", "\"mallory\"")).unwrap();
        let error = release_cmd(&registry, &["verify"]).expect_err("edited registry detected");
        assert!(error.contains("registry chain broken"), "got: {error}");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn approve_normalizes_names_and_verify_requires_registry() {
        let base = std::env::temp_dir().join(format!("devlish-sod-{}", now_unix()));
        std::fs::create_dir_all(&base).unwrap();
        let registry = base.join("registry.json");
        let rule = release_fixture(&base, "1.0.0", "");
        release_cmd(
            &registry,
            &["propose", &rule.to_string_lossy(), "--author", "Andrew"],
        )
        .unwrap();

        // Case and whitespace do not dress the author up as someone else.
        let error = release_cmd(
            &registry,
            &["approve", "pricing.tier@1.0.0", "--approver", "  andrew "],
        )
        .expect_err("case/whitespace self-approval refused");
        assert!(error.contains("separation of duties"), "got: {error}");

        // Every verb except propose errors on a missing registry.
        let missing = base.join("nope.json");
        let error = run_release(vec![
            "release".to_string(),
            "verify".to_string(),
            "--registry".to_string(),
            missing.to_string_lossy().to_string(),
        ])
        .expect_err("verify on missing registry fails");
        assert!(error.contains("does not exist"), "got: {error}");

        std::fs::remove_dir_all(&base).ok();
    }

    #[test]
    fn windows_overlap_semantics() {
        // Open-ended on both sides overlaps everything.
        assert!(windows_overlap((None, None), (None, None)));
        assert!(windows_overlap(
            (None, Some("2026-12-31")),
            (Some("2026-06-01"), None)
        ));
        // Disjoint: one ends before the other begins.
        assert!(!windows_overlap(
            (None, Some("2026-12-31")),
            (Some("2027-01-01"), None)
        ));
        // Touching endpoints overlap (both in force that day).
        assert!(windows_overlap(
            (Some("2026-01-01"), Some("2026-06-30")),
            (Some("2026-06-30"), None)
        ));
    }

    #[test]
    fn journal_flag_requires_audit_log() {
        let error = run_execute(vec![
            "run".to_string(),
            "whatever.dvl".to_string(),
            "--journal".to_string(),
            "/tmp/j".to_string(),
        ])
        .expect_err("journal without audit log rejected");
        assert!(
            error.contains("--journal requires --audit-log"),
            "got: {error}"
        );
    }
}
