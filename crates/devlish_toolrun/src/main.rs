use std::env;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

const SCHEMA_VERSION: &str = "devlish-toolrun-report-v0";

#[derive(Debug, Clone)]
struct Config {
    raw_dir: PathBuf,
    max_lines: usize,
    command: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawOutput {
    stdout: String,
    stderr: String,
    stdout_bytes: usize,
    stderr_bytes: usize,
    exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Summary {
    adapter: &'static str,
    status: String,
    fields: Vec<(String, JsonValue)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum JsonValue {
    String(String),
    Number(i64),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
    Null,
}

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn run(args: Vec<String>) -> Result<ExitCode, String> {
    let config = parse_args(args)?;
    let started_ms = unix_ms();
    let output = run_command(&config.command)?;
    let raw_ref = write_raw_output(&config.raw_dir, started_ms, &config.command, &output)
        .map_err(|error| format!("failed to write raw output: {error}"))?;
    let summary = summarize(&config.command, &output, config.max_lines);
    let report = render_report(&config.command, &output, &raw_ref, &summary);
    println!("{report}");
    Ok(if output_status_success(&output) {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn parse_args(args: Vec<String>) -> Result<Config, String> {
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" {
        return Err(usage());
    }
    if args[0] != "exec" {
        return Err(format!("unknown command: {}\n\n{}", args[0], usage()));
    }

    let mut raw_dir = PathBuf::from(".devlish/toolruns");
    let mut max_lines = 24usize;
    let mut index = 1usize;
    while index < args.len() {
        match args[index].as_str() {
            "--raw-dir" => {
                index += 1;
                raw_dir = PathBuf::from(args.get(index).ok_or("--raw-dir requires a path")?);
            }
            "--max-lines" => {
                index += 1;
                let raw = args.get(index).ok_or("--max-lines requires a number")?;
                max_lines = raw
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --max-lines value: {raw}"))?;
            }
            "--" => {
                index += 1;
                break;
            }
            value if value.starts_with('-') => return Err(format!("unknown option: {value}")),
            _ => break,
        }
        index += 1;
    }

    let command = args[index..].to_vec();
    if command.is_empty() {
        return Err(format!("missing command to execute\n\n{}", usage()));
    }

    Ok(Config {
        raw_dir,
        max_lines,
        command,
    })
}

fn usage() -> String {
    "Usage: devlish-toolrun exec [--raw-dir PATH] [--max-lines N] -- <command> [args...]"
        .to_string()
}

fn run_command(command: &[String]) -> Result<RawOutput, String> {
    let program = command.first().ok_or("missing command")?;
    let output = Command::new(program)
        .args(command.iter().skip(1).map(OsStr::new))
        .output()
        .map_err(|error| format!("failed to execute {program}: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Ok(RawOutput {
        stdout_bytes: output.stdout.len(),
        stderr_bytes: output.stderr.len(),
        exit_code: output.status.code().unwrap_or(1),
        stdout,
        stderr,
    })
}

fn write_raw_output(
    raw_dir: &Path,
    started_ms: u128,
    command: &[String],
    output: &RawOutput,
) -> io::Result<PathBuf> {
    let run_id = format!("{started_ms}-{}", command_slug(command));
    let run_dir = raw_dir.join(run_id);
    fs::create_dir_all(&run_dir)?;
    fs::write(run_dir.join("stdout.txt"), &output.stdout)?;
    fs::write(run_dir.join("stderr.txt"), &output.stderr)?;
    fs::write(run_dir.join("command.txt"), command.join(" "))?;
    Ok(run_dir)
}

fn summarize(command: &[String], output: &RawOutput, max_lines: usize) -> Summary {
    let command_text = command.join(" ");
    if command_text.contains("rspec") {
        summarize_rspec(output)
    } else if command_text.contains("npm test") || command_text.contains("node --test") {
        summarize_node_test(output)
    } else if command_text.starts_with("git status") {
        summarize_git_status(output)
    } else if command_text.starts_with("rg ") || command_text.starts_with("grep ") {
        summarize_search(output, max_lines)
    } else {
        summarize_generic(output, max_lines)
    }
}

fn summarize_rspec(output: &RawOutput) -> Summary {
    let text = combined_output(output);
    let (examples, failures) = rspec_counts(&text);
    let duration = capture_after(&text, "Finished in ")
        .and_then(|tail| tail.split_whitespace().next().map(str::to_string));
    let status = if output.exit_code == 0 && failures.unwrap_or(1) == 0 {
        "pass"
    } else {
        "fail"
    };
    Summary {
        adapter: "rspec",
        status: status.to_string(),
        fields: vec![
            number_field("examples", examples),
            number_field("failures", failures),
            string_or_null_field("duration_s", duration),
        ],
    }
}

fn summarize_node_test(output: &RawOutput) -> Summary {
    let text = combined_output(output);
    let tests = tap_count(&text, "# tests ");
    let pass = tap_count(&text, "# pass ");
    let fail = tap_count(&text, "# fail ");
    let status = if fail == Some(0) || (fail.is_none() && output.exit_code == 0) {
        "pass"
    } else {
        "fail"
    };
    Summary {
        adapter: "node_test",
        status: status.to_string(),
        fields: vec![
            number_field("tests", tests),
            number_field("pass", pass),
            number_field("fail", fail),
        ],
    }
}

fn summarize_git_status(output: &RawOutput) -> Summary {
    let lines: Vec<&str> = output.stdout.lines().collect();
    let branch = lines
        .iter()
        .find(|line| line.starts_with("## "))
        .map(|line| line.trim_start_matches("## ").to_string());
    let changed_files = lines.iter().filter(|line| !line.starts_with("## ")).count();
    Summary {
        adapter: "git_status",
        status: if changed_files == 0 {
            "clean".to_string()
        } else {
            "dirty".to_string()
        },
        fields: vec![
            string_or_null_field("branch", branch),
            (
                "changed_files".to_string(),
                JsonValue::Number(changed_files as i64),
            ),
            (
                "entries".to_string(),
                JsonValue::Array(
                    lines
                        .iter()
                        .filter(|line| !line.starts_with("## "))
                        .take(20)
                        .map(|line| JsonValue::String((*line).to_string()))
                        .collect(),
                ),
            ),
        ],
    }
}

fn summarize_search(output: &RawOutput, max_lines: usize) -> Summary {
    let lines: Vec<&str> = output.stdout.lines().collect();
    let mut files = Vec::<String>::new();
    for line in &lines {
        if let Some(file) = line.split(':').next() {
            let file = file.to_string();
            if !files.contains(&file) {
                files.push(file);
            }
        }
    }
    Summary {
        adapter: "search",
        status: if lines.is_empty() {
            "no_matches".to_string()
        } else {
            "matches".to_string()
        },
        fields: vec![
            ("matches".to_string(), JsonValue::Number(lines.len() as i64)),
            ("files".to_string(), JsonValue::Number(files.len() as i64)),
            (
                "sample".to_string(),
                JsonValue::Array(
                    lines
                        .iter()
                        .take(max_lines)
                        .map(|line| JsonValue::String((*line).to_string()))
                        .collect(),
                ),
            ),
        ],
    }
}

fn summarize_generic(output: &RawOutput, max_lines: usize) -> Summary {
    let text = combined_output(output);
    let lines: Vec<&str> = text.lines().collect();
    Summary {
        adapter: "generic",
        status: if output.exit_code != 0 {
            "fail".to_string()
        } else if output.stderr_bytes == 0 {
            "ok".to_string()
        } else {
            "check_stderr".to_string()
        },
        fields: vec![
            ("lines".to_string(), JsonValue::Number(lines.len() as i64)),
            (
                "sample".to_string(),
                JsonValue::Array(
                    compact_sample(&lines, max_lines)
                        .into_iter()
                        .map(JsonValue::String)
                        .collect(),
                ),
            ),
        ],
    }
}

fn render_report(
    command: &[String],
    output: &RawOutput,
    raw_ref: &Path,
    summary: &Summary,
) -> String {
    let raw_bytes = output.stdout_bytes + output.stderr_bytes;
    let mut compact = Vec::new();
    compact.push((
        "schema_version".to_string(),
        JsonValue::String(SCHEMA_VERSION.to_string()),
    ));
    compact.push((
        "command".to_string(),
        JsonValue::Array(
            command
                .iter()
                .map(|part| JsonValue::String(part.clone()))
                .collect(),
        ),
    ));
    compact.push((
        "adapter".to_string(),
        JsonValue::String(summary.adapter.to_string()),
    ));
    compact.push((
        "status".to_string(),
        JsonValue::String(summary.status.clone()),
    ));
    compact.push((
        "exit_code".to_string(),
        JsonValue::Number(output.exit_code as i64),
    ));
    compact.push((
        "raw".to_string(),
        JsonValue::Object(vec![
            ("ref".to_string(), JsonValue::String(path_string(raw_ref))),
            (
                "stdout_bytes".to_string(),
                JsonValue::Number(output.stdout_bytes as i64),
            ),
            (
                "stderr_bytes".to_string(),
                JsonValue::Number(output.stderr_bytes as i64),
            ),
            (
                "total_bytes".to_string(),
                JsonValue::Number(raw_bytes as i64),
            ),
        ]),
    ));
    compact.push((
        "summary".to_string(),
        JsonValue::Object(summary.fields.clone()),
    ));
    let without_accounting = json_object(&compact);
    let compact_bytes = without_accounting.len();
    compact.push((
        "token_accounting".to_string(),
        JsonValue::Object(vec![
            ("raw_bytes".to_string(), JsonValue::Number(raw_bytes as i64)),
            (
                "model_visible_bytes".to_string(),
                JsonValue::Number(compact_bytes as i64),
            ),
            (
                "estimated_raw_tokens".to_string(),
                JsonValue::Number(estimated_tokens(raw_bytes) as i64),
            ),
            (
                "estimated_model_visible_tokens".to_string(),
                JsonValue::Number(estimated_tokens(compact_bytes) as i64),
            ),
            (
                "estimated_tokens_saved".to_string(),
                JsonValue::Number(estimated_tokens(raw_bytes.saturating_sub(compact_bytes)) as i64),
            ),
        ]),
    ));
    json_object(&compact)
}

fn output_status_success(output: &RawOutput) -> bool {
    output.exit_code == 0
}

fn command_slug(command: &[String]) -> String {
    let joined = command.join("-");
    let mut slug = String::new();
    for ch in joined.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').chars().take(64).collect()
}

fn combined_output(output: &RawOutput) -> String {
    if output.stderr.is_empty() {
        output.stdout.clone()
    } else if output.stdout.is_empty() {
        output.stderr.clone()
    } else {
        format!("{}\n{}", output.stdout, output.stderr)
    }
}

fn compact_sample(lines: &[&str], max_lines: usize) -> Vec<String> {
    if lines.len() <= max_lines {
        return lines.iter().map(|line| (*line).to_string()).collect();
    }
    let head_count = max_lines / 2;
    let tail_count = max_lines.saturating_sub(head_count + 1);
    let mut sample: Vec<String> = lines
        .iter()
        .take(head_count)
        .map(|line| (*line).to_string())
        .collect();
    sample.push(format!(
        "... {} lines omitted ...",
        lines.len() - head_count - tail_count
    ));
    sample.extend(
        lines
            .iter()
            .skip(lines.len().saturating_sub(tail_count))
            .map(|line| (*line).to_string()),
    );
    sample
}

fn rspec_counts(text: &str) -> (Option<i64>, Option<i64>) {
    for line in text.lines().rev() {
        let has_example_count = line.contains(" example") || line.contains(" examples");
        let has_failure_count = line.contains(" failure") || line.contains(" failures");
        if !has_example_count || !has_failure_count {
            continue;
        }

        let examples = number_before_marker(line, &[" examples", " example"]);
        let failures = number_before_marker(line, &[" failures", " failure"]);
        return (examples, failures);
    }

    (None, None)
}

fn number_before_marker(line: &str, markers: &[&str]) -> Option<i64> {
    markers.iter().find_map(|marker| {
        line.find(marker)
            .and_then(|index| parse_last_number(&line[..index]))
    })
}

fn capture_after<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    text.find(marker).map(|index| &text[index + marker.len()..])
}

fn parse_last_number(text: impl AsRef<str>) -> Option<i64> {
    text.as_ref().split_whitespace().rev().find_map(|part| {
        part.trim_matches(|ch: char| !ch.is_ascii_digit())
            .parse()
            .ok()
    })
}

fn tap_count(text: &str, prefix: &str) -> Option<i64> {
    text.lines()
        .find_map(|line| line.strip_prefix(prefix))
        .and_then(|value| value.trim().parse::<i64>().ok())
}

fn number_field(name: &str, value: Option<i64>) -> (String, JsonValue) {
    (
        name.to_string(),
        value.map(JsonValue::Number).unwrap_or(JsonValue::Null),
    )
}

fn string_or_null_field(name: &str, value: Option<String>) -> (String, JsonValue) {
    (
        name.to_string(),
        value.map(JsonValue::String).unwrap_or(JsonValue::Null),
    )
}

fn json_object(fields: &[(String, JsonValue)]) -> String {
    let body = fields
        .iter()
        .map(|(key, value)| format!("\"{}\":{}", escape_json(key), json_value(value)))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{body}}}")
}

fn json_value(value: &JsonValue) -> String {
    match value {
        JsonValue::String(value) => format!("\"{}\"", escape_json(value)),
        JsonValue::Number(value) => value.to_string(),
        JsonValue::Array(values) => {
            let body = values.iter().map(json_value).collect::<Vec<_>>().join(",");
            format!("[{body}]")
        }
        JsonValue::Object(fields) => json_object(fields),
        JsonValue::Null => "null".to_string(),
    }
}

fn escape_json(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => escaped.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped
}

fn estimated_tokens(bytes: usize) -> usize {
    bytes.div_ceil(4)
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn unix_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(stdout: &str, stderr: &str) -> RawOutput {
        RawOutput {
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            stdout_bytes: stdout.len(),
            stderr_bytes: stderr.len(),
            exit_code: 0,
        }
    }

    #[test]
    fn summarizes_rspec_pass_output() {
        let summary = summarize_rspec(&raw(
            "Finished in 9.5 seconds\n447 examples, 0 failures\n",
            "",
        ));

        assert_eq!(summary.adapter, "rspec");
        assert_eq!(summary.status, "pass");
        assert_eq!(
            summary.fields[0],
            ("examples".to_string(), JsonValue::Number(447))
        );
        assert_eq!(
            summary.fields[1],
            ("failures".to_string(), JsonValue::Number(0))
        );
    }

    #[test]
    fn summarizes_rspec_from_final_result_line_only() {
        let summary = summarize_rspec(&raw(
            "noise with seed 3751 and a word failures\nFinished in 8.98 seconds\n447 examples, 0 failures\nRandomized with seed 3751\n",
            "",
        ));

        assert_eq!(summary.status, "pass");
        assert_eq!(
            summary.fields[0],
            ("examples".to_string(), JsonValue::Number(447))
        );
        assert_eq!(
            summary.fields[1],
            ("failures".to_string(), JsonValue::Number(0))
        );
    }

    #[test]
    fn summarizes_node_test_tap_footer() {
        let summary = summarize_node_test(&raw("# tests 14\n# pass 14\n# fail 0\n", ""));

        assert_eq!(summary.adapter, "node_test");
        assert_eq!(summary.status, "pass");
        assert_eq!(
            summary.fields[0],
            ("tests".to_string(), JsonValue::Number(14))
        );
        assert_eq!(
            summary.fields[2],
            ("fail".to_string(), JsonValue::Number(0))
        );
    }

    #[test]
    fn summarizes_git_status_dirty_state() {
        let summary = summarize_git_status(&raw(
            "## main...origin/main\n M README.md\n?? crates/devlish_toolrun/\n",
            "",
        ));

        assert_eq!(summary.adapter, "git_status");
        assert_eq!(summary.status, "dirty");
        assert_eq!(
            summary.fields[1],
            ("changed_files".to_string(), JsonValue::Number(2))
        );
    }

    #[test]
    fn compact_sample_keeps_head_and_tail() {
        let lines = vec!["one", "two", "three", "four", "five"];

        assert_eq!(
            compact_sample(&lines, 3),
            vec![
                "one".to_string(),
                "... 3 lines omitted ...".to_string(),
                "five".to_string()
            ]
        );
    }

    #[test]
    fn renders_valid_json_shape_without_dependencies() {
        let output = raw("Finished in 1.0 seconds\n1 example, 0 failures\n", "");
        let summary = summarize_rspec(&output);
        let rendered = render_report(
            &[
                "bundle".to_string(),
                "exec".to_string(),
                "rspec".to_string(),
            ],
            &output,
            Path::new(".devlish/toolruns/demo"),
            &summary,
        );

        assert!(rendered.contains("\"schema_version\":\"devlish-toolrun-report-v0\""));
        assert!(rendered.contains("\"estimated_tokens_saved\""));
        assert!(rendered.contains("\"adapter\":\"rspec\""));
    }
}
