use serde::Serialize;
use serde_json::{json, Map, Number, Value};
use std::collections::{HashMap, HashSet};

const FORMAT: &str = "devlish-bytecode";
const FORMAT_VERSION: u8 = 0;
const COMPILER_VERSION: &str = "0.1.0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: usize,
    pub message: String,
    pub source_text: String,
}

impl Diagnostic {
    fn new(line: usize, message: impl Into<String>, source_text: impl Into<String>) -> Self {
        Self {
            line,
            message: message.into(),
            source_text: source_text.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    pub diagnostics: Vec<Diagnostic>,
}

impl CompileError {
    fn single(line: usize, message: impl Into<String>, source_text: impl Into<String>) -> Self {
        Self {
            diagnostics: vec![Diagnostic::new(line, message, source_text)],
        }
    }
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for diagnostic in &self.diagnostics {
            writeln!(
                formatter,
                "line {}: {}: {}",
                diagnostic.line, diagnostic.message, diagnostic.source_text
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for CompileError {}

#[derive(Debug, Clone, PartialEq)]
pub struct CompileOptions {
    pub source_path: Option<String>,
    /// Search paths for resolving Import statements, checked in order after
    /// the current file's directory. Populated from DEVLISH_PATH env var
    /// and ~/.devlish/lib/ by the CLI.
    pub search_paths: Vec<String>,
}

struct Program {
    statements: Vec<Statement>,
    manifest: Option<ProgramManifest>,
}

#[derive(Debug, Clone, PartialEq)]
struct ClassProgram {
    module_name: String,
    class_name: String,
    parent_class: Option<(String, String)>,
    methods: Vec<MethodDef>,
}

#[derive(Debug, Clone, PartialEq)]
struct MethodDef {
    name: String,
    ruby_name: String,
    params: Vec<String>,
    is_private: bool,
    body: Vec<Statement>,
    return_value: Option<Expression>,
    line: usize,
    source_text: String,
}

enum ParsedSource {
    Flat(Program),
    Class(ClassProgram),
}

#[derive(Debug, Clone, PartialEq)]
struct Statement {
    line: usize,
    source_text: String,
    kind: StatementKind,
}

#[derive(Debug, Clone, PartialEq)]
enum StatementKind {
    Input {
        target: String,
        prompt: Expression,
        source: InputSource,
    },
    Assignment {
        target: String,
        value: Expression,
    },
    Output {
        value: Expression,
    },
    FileWrite {
        value: Expression,
        path: Expression,
        mode: FileWriteMode,
    },
    FileRead {
        path: Expression,
        target: String,
        format: FileReadFormat,
    },
    Branch {
        condition: Expression,
        then_statements: Vec<Statement>,
        else_statements: Vec<Statement>,
    },
    ReadXlsxCell {
        sheet: String,
        cell: String,
        target: String,
    },
    ReadPdfText {
        path: String,
        target: String,
    },
    ReadDocxText {
        path: String,
        target: String,
    },
    Assertion {
        assertion_id: String,
        target: String,
        operator: AssertionOperator,
        expected: Option<Expression>,
    },
    ExportAssertions {
        path: Expression,
    },
    WhileLoop {
        condition: Expression,
        body: Vec<Statement>,
    },
    UntilLoop {
        condition: Expression,
        body: Vec<Statement>,
    },
    ForEach {
        item: String,
        collection: Expression,
        body: Vec<Statement>,
    },
    TryRecover {
        body: Vec<Statement>,
        recovery: Vec<Statement>,
    },
    Break,
    Continue,
    Fail {
        message: Expression,
    },
    Require {
        condition: Expression,
        message: Option<Expression>,
    },
    SetField {
        target: Expression,
        value: Expression,
        condition: Option<Expression>,
    },
    Append {
        value: Expression,
        target: String,
    },
    Pop {
        source: String,
        store_as: String,
    },
    ConditionalAssignment {
        target: String,
        value: Expression,
        condition: Expression,
    },
    Bind {
        source_name: String,
        target_name: String,
        kind: String,
    },
    Definition {
        name: String,
        definition: String,
    },
    Load {
        path: Option<String>,
        alias: Option<String>,
    },
    Extract {
        target: String,
        store_as: String,
    },
    Validate {
        target: Expression,
        rule: ValidateRule,
        value: Option<Expression>,
    },
    DocumentRequirement {
        verb: String,
        target: String,
    },
    Route {
        source: Expression,
        destination: Expression,
    },
    ServiceCall {
        service: String,
        action: String,
        arguments: Vec<(String, Expression)>,
    },
    Import {
        path: String,
    },
    /// `Use the <module> module.` / `Use <a> and <b> from the <module> module.`
    /// Resolved at compile time like Import, but the module is found by
    /// well-known name (bundled stdlib first, then search paths) and its
    /// symbols are namespaced instead of inlined flat. An empty `symbols`
    /// vec means whole-module import (qualified access only).
    UseModule {
        module: String,
        symbols: Vec<String>,
    },
    ReadStdin {
        target: String,
    },
    Trigger {
        trigger_type: String,
        params: Vec<(String, String)>,
    },
    RespondWith {
        value: Expression,
    },
    HttpRequest {
        method: String,
        url: Expression,
        body: Option<Expression>,
        dest: String,
    },
    HttpDownload {
        url: Expression,
        path: Expression,
    },
    XlsxReadRows {
        path: Expression,
        sheet: Option<String>,
        dest: String,
    },
    Checkpoint {
        prompt: Expression,
        context_key: Option<String>,
    },
    FileCopy {
        source: Expression,
        destination: Expression,
    },
    FileMove {
        source: Expression,
        destination: Expression,
    },
    FileMkdir {
        path: Expression,
    },
    FileDelete {
        path: Expression,
    },
    FileExists {
        path: Expression,
        dest: String,
    },
    FileStat {
        path: Expression,
        dest: String,
    },
    FileList {
        path: Expression,
        dest: String,
    },
    FileGlob {
        pattern: Expression,
        directory: Expression,
        dest: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputSource {
    MultilinePrompt,
    MultilineStdin,
    Prompt,
    Stdin,
}

impl InputSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::MultilinePrompt => "multiline_prompt",
            Self::MultilineStdin => "multiline_stdin",
            Self::Prompt => "prompt",
            Self::Stdin => "stdin",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileWriteMode {
    Append,
    Csv,
    Export,
    Overwrite,
    Write,
}

impl FileWriteMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Append => "append",
            Self::Csv => "csv",
            Self::Export => "export",
            Self::Overwrite => "overwrite",
            Self::Write => "write",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileReadFormat {
    Csv,
    Json,
    Text,
}

impl FileReadFormat {
    fn as_str(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
            Self::Text => "text",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AssertionOperator {
    Equals,
    Contains,
    Present,
    NotSpreadsheetError,
}

impl AssertionOperator {
    fn as_str(self) -> &'static str {
        match self {
            Self::Equals => "equals",
            Self::Contains => "contains",
            Self::Present => "present",
            Self::NotSpreadsheetError => "not_spreadsheet_error",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValidateRule {
    Contains,
    Equals,
    Matches,
    Maximum,
    Minimum,
    Missing,
    OneOf,
    Present,
}

impl ValidateRule {
    fn as_str(self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::Equals => "equals",
            Self::Matches => "matches",
            Self::Maximum => "maximum",
            Self::Minimum => "minimum",
            Self::Missing => "missing",
            Self::OneOf => "one_of",
            Self::Present => "present",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Expression {
    Literal(Value),
    Variable(String),
    Binary {
        operator: BinaryOperator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    Comparison {
        operator: ComparisonOperator,
        left: Box<Expression>,
        right: Box<Expression>,
    },
    LogicalAnd {
        left: Box<Expression>,
        right: Box<Expression>,
    },
    LogicalOr {
        left: Box<Expression>,
        right: Box<Expression>,
    },
    LogicalNot {
        operand: Box<Expression>,
    },
    ListLiteral(Vec<Expression>),
    RecordLiteral(Vec<(String, Expression)>),
    FieldAccess {
        record: Box<Expression>,
        field: String,
    },
    BuiltinCall {
        name: String,
        arguments: Vec<Expression>,
    },
    Contains {
        left: Box<Expression>,
        right: Box<Expression>,
    },
    StartsWith {
        left: Box<Expression>,
        right: Box<Expression>,
    },
    EndsWith {
        left: Box<Expression>,
        right: Box<Expression>,
    },
    IsMissing(Box<Expression>),
    IsIn {
        value: Box<Expression>,
        collection: Box<Expression>,
    },
    MethodCall {
        name: String,
        arguments: Vec<Expression>,
    },
    /// A possessive module-qualified reference (`math's pi`). Resolved at
    /// compile time by `resolve_qualified_refs` into the module's mangled
    /// symbol; never reaches bytecode emission.
    QualifiedRef {
        module: String,
        name: String,
    },
    /// A collection operation whose per-element behavior is an arbitrary
    /// expression (`map xs to item times 2`, `filter xs where item > 3`,
    /// `reduce xs starting at 0 with total and item to total plus item`,
    /// `sort xs by item's ... key expr`). Compiled by inlining an index loop
    /// (the ForEach skeleton) — there are no function values or call frames
    /// at runtime (DEVL-132).
    Comprehension {
        kind: ComprehensionKind,
        list: Box<Expression>,
        /// Symbol the current element is bound to inside `body` (`item` unless
        /// the reduce phrasing names it).
        binding: String,
        /// Reduce only: accumulator symbol name and its initial value.
        accumulator: Option<(String, Box<Expression>)>,
        body: Box<Expression>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComprehensionKind {
    Map,
    Filter,
    Reject,
    Find,
    Any,
    All,
    Reduce,
    SortBy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BinaryOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    IntDivide,
    Power,
    And,
    Or,
}

impl BinaryOperator {
    fn opcode(self) -> &'static str {
        match self {
            Self::Add => "ADD",
            Self::Subtract => "SUB",
            Self::Multiply => "MUL",
            Self::Divide => "DIV",
            Self::Modulo => "MOD",
            Self::IntDivide => "IDIV",
            Self::Power => "POW",
            Self::And => "AND",
            Self::Or => "OR",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComparisonOperator {
    Equals,
    NotEquals,
    GreaterThan,
    GreaterOrEqual,
    LessThan,
    LessOrEqual,
}

impl ComparisonOperator {
    fn opcode(self) -> &'static str {
        match self {
            Self::Equals => "EQ",
            Self::NotEquals => "NEQ",
            Self::GreaterThan => "GT",
            Self::GreaterOrEqual => "GTE",
            Self::LessThan => "LT",
            Self::LessOrEqual => "LTE",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BytecodePackage {
    format: &'static str,
    format_version: u8,
    compiler_version: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_path: Option<String>,
    source_hash: String,
    /// Per-file manifest of every source file that contributed to this
    /// artifact, including inlined imports. Present only when the program
    /// has imports; a single-file program omits it and `source_hash` is
    /// simply the sha256 of that one file (backward compatible).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    source_files: Vec<Value>,
    constant_pool: Vec<Value>,
    symbol_table: Vec<String>,
    instructions: Vec<Value>,
    source_map: Vec<Value>,
    effect_table: Vec<Value>,
    imports: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    class_info: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    methods: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest: Option<Value>,
    /// Bundled standard-library modules this artifact used, with the stdlib
    /// version they shipped in. Present only when a `Use`d module resolved to
    /// the bundled stdlib (DEVL-131).
    #[serde(skip_serializing_if = "Option::is_none")]
    stdlib: Option<Value>,
}

#[derive(Debug, Clone, Default)]
struct ProgramManifest {
    permissions: Vec<ManifestPermission>,
    boundaries: Vec<String>,
    callers: Vec<String>,
    rule: Option<RuleMetadata>,
    /// Runtime variables the program expects to receive via `--input` JSON.
    /// Declared so the unbound-identifier lint treats them as bound rather than
    /// flagging every `--input`-driven program (DEVL-127).
    inputs: Vec<String>,
}

#[derive(Debug, Clone)]
struct ManifestPermission {
    kind: String,
    scope: Option<String>,
}

/// Governance identity for a rule, declared in a `Rule:` manifest section.
/// Present only for governed rules; a program without a `Rule:` section
/// compiles exactly as before (ungoverned mode).
#[derive(Debug, Clone, PartialEq, Eq)]
struct RuleMetadata {
    id: String,
    version: String,
    author: Option<String>,
    effective_from: Option<String>,
    effective_until: Option<String>,
}

impl RuleMetadata {
    fn to_value(&self) -> Value {
        let mut m = serde_json::Map::new();
        m.insert("id".to_string(), Value::String(self.id.clone()));
        m.insert("version".to_string(), Value::String(self.version.clone()));
        if let Some(ref author) = self.author {
            m.insert("author".to_string(), Value::String(author.clone()));
        }
        if let Some(ref from) = self.effective_from {
            m.insert("effective_from".to_string(), Value::String(from.clone()));
        }
        if let Some(ref until) = self.effective_until {
            m.insert("effective_until".to_string(), Value::String(until.clone()));
        }
        Value::Object(m)
    }
}

impl ProgramManifest {
    fn is_empty(&self) -> bool {
        self.permissions.is_empty()
            && self.boundaries.is_empty()
            && self.callers.is_empty()
            && self.rule.is_none()
            && self.inputs.is_empty()
    }

    fn to_value(&self) -> Value {
        let permissions: Vec<Value> = self
            .permissions
            .iter()
            .map(|p| {
                let mut m = serde_json::Map::new();
                m.insert("kind".to_string(), Value::String(p.kind.clone()));
                if let Some(ref scope) = p.scope {
                    m.insert("scope".to_string(), Value::String(scope.clone()));
                }
                Value::Object(m)
            })
            .collect();
        let boundaries: Vec<Value> = self.boundaries.iter().map(|b| json!(b)).collect();
        let callers: Vec<Value> = self.callers.iter().map(|c| json!(c)).collect();
        let inputs: Vec<Value> = self.inputs.iter().map(|i| json!(i)).collect();
        let mut obj = serde_json::Map::new();
        obj.insert("permissions".to_string(), Value::Array(permissions));
        obj.insert("boundaries".to_string(), Value::Array(boundaries));
        obj.insert("callers".to_string(), Value::Array(callers));
        obj.insert("inputs".to_string(), Value::Array(inputs));
        if let Some(ref rule) = self.rule {
            obj.insert("rule".to_string(), rule.to_value());
        }
        Value::Object(obj)
    }
}

pub fn compile_source(
    source: &str,
    options: CompileOptions,
) -> Result<BytecodePackage, CompileError> {
    let mut collected: Vec<(std::path::PathBuf, String)> = Vec::new();
    match detect_and_parse(source)? {
        ParsedSource::Flat(mut program) => {
            let mut imported = HashSet::new();
            let mut active = HashSet::new();
            if let Some(ref sp) = options.source_path {
                active.insert(normalize_path(sp));
            }
            let mut module_exports = HashMap::new();
            let direct_uses = resolve_imports(
                &mut program.statements,
                &options,
                &mut imported,
                &mut active,
                &mut collected,
                &HashSet::new(),
                &mut module_exports,
            )?;
            reject_nested_use(&program.statements)?;
            validate_literal_arguments(&program.statements)?;
            resolve_qualified_refs(
                &mut program.statements,
                &filtered_exports(&module_exports, &direct_uses),
            )?;
            let closure = build_source_closure(source, &options.source_path, &collected);
            Ok(BytecodeCompiler::new(options).compile(program, closure))
        }
        ParsedSource::Class(mut class_program) => {
            for method in &mut class_program.methods {
                let mut imported = HashSet::new();
                let mut active = HashSet::new();
                if let Some(ref sp) = options.source_path {
                    active.insert(normalize_path(sp));
                }
                // An imported fragment must not shadow this method's params.
                let reserved: HashSet<String> = method.params.iter().cloned().collect();
                let mut module_exports = HashMap::new();
                let direct_uses = resolve_imports(
                    &mut method.body,
                    &options,
                    &mut imported,
                    &mut active,
                    &mut collected,
                    &reserved,
                    &mut module_exports,
                )?;
                reject_nested_use(&method.body)?;
                validate_literal_arguments(&method.body)?;
                if let Some(return_value) = &method.return_value {
                    if let Some(message) = find_invalid_literal_argument(return_value) {
                        return Err(CompileError::single(
                            method.line,
                            message,
                            &method.source_text,
                        ));
                    }
                }
                let visible = filtered_exports(&module_exports, &direct_uses);
                resolve_qualified_refs(&mut method.body, &visible)?;
                if let Some(return_value) = &mut method.return_value {
                    resolve_qualified_refs_in_expression(
                        return_value,
                        &visible,
                        method.line,
                        &method.source_text,
                    )?;
                }
            }
            let closure = build_source_closure(source, &options.source_path, &collected);
            compile_class_program(options, &class_program, closure)
        }
    }
}

fn normalize_path(p: &str) -> String {
    std::path::Path::new(p)
        .canonicalize()
        .unwrap_or_else(|_| std::path::PathBuf::from(p))
        .to_string_lossy()
        .to_string()
}

/// The set of source files that produced an artifact and a single hash over
/// all of them. `hash` becomes the bytecode's `source_hash`; `files` becomes
/// its `source_files` manifest (empty for a single-file program).
struct SourceClosure {
    hash: String,
    files: Vec<Value>,
    /// Names of bundled stdlib modules that contributed source (DEVL-131).
    stdlib_modules: Vec<String>,
}

impl SourceClosure {
    fn stdlib_value(&self) -> Option<Value> {
        if self.stdlib_modules.is_empty() {
            return None;
        }
        Some(json!({
            "version": STDLIB_VERSION,
            "modules": self.stdlib_modules,
        }))
    }
}

/// Canonicalizes `p`, or if it does not exist on disk, canonicalizes its
/// parent and re-attaches the file name. This keeps an entry file that has no
/// on-disk backing (e.g. an MCP source string) consistent with its sibling
/// imports, which matters on platforms where the temp root is itself a symlink
/// (macOS `/var` -> `/private/var`).
fn canonicalize_best(p: &std::path::Path) -> std::path::PathBuf {
    if let Ok(canonical) = p.canonicalize() {
        return canonical;
    }
    if let (Some(parent), Some(name)) = (p.parent(), p.file_name()) {
        if let Ok(canonical_parent) = parent.canonicalize() {
            return canonical_parent.join(name);
        }
    }
    p.to_path_buf()
}

/// Expresses `target` relative to `base`, canonicalizing both first so the
/// result is independent of the current working directory.
fn relative_path_string(base: &std::path::Path, target: &std::path::Path) -> String {
    let base = canonicalize_best(base);
    let target = canonicalize_best(target);
    let base_comps: Vec<_> = base.components().collect();
    let target_comps: Vec<_> = target.components().collect();
    let mut shared = 0;
    while shared < base_comps.len()
        && shared < target_comps.len()
        && base_comps[shared] == target_comps[shared]
    {
        shared += 1;
    }
    let mut rel = std::path::PathBuf::new();
    for _ in shared..base_comps.len() {
        rel.push("..");
    }
    for comp in &target_comps[shared..] {
        rel.push(comp.as_os_str());
    }
    let joined = rel.to_string_lossy().replace('\\', "/");
    if joined.is_empty() {
        ".".to_string()
    } else {
        joined
    }
}

/// Builds the source closure for an artifact. A single-file program (no
/// imports) hashes exactly its own bytes and carries no manifest, so its
/// `source_hash` matches earlier compiler output. A multi-file program's
/// `source_hash` is a sha256 over the sorted set of per-file content hashes,
/// so editing any inlined import changes it.
///
/// `source_hash` intentionally covers file *contents* only, not paths: an
/// import resolved through a search path (`DEVLISH_PATH`, `~/.devlish/lib/`)
/// lives at a machine-specific absolute location, so mixing its path into the
/// hash would make two honest builds of byte-identical sources disagree. The
/// human-facing `source_files` manifest still records a best-effort relative
/// path per file for auditing; those paths are display-only.
fn build_source_closure(
    entry_source: &str,
    entry_path: &Option<String>,
    imports: &[(std::path::PathBuf, String)],
) -> SourceClosure {
    let stdlib_modules: Vec<String> = {
        let mut names: Vec<String> = imports
            .iter()
            .filter_map(|(path, _)| {
                path.to_str()
                    .and_then(|text| text.strip_prefix(STDLIB_PATH_PREFIX))
                    .map(|file| file.trim_end_matches(".dvl").to_string())
            })
            .collect();
        names.sort();
        names.dedup();
        names
    };

    if imports.is_empty() {
        return SourceClosure {
            hash: sha256_hex(entry_source.as_bytes()),
            files: Vec::new(),
            stdlib_modules,
        };
    }

    let base = entry_path
        .as_ref()
        .and_then(|p| std::path::Path::new(p).parent())
        // A bare filename ("main.dvl") has an empty parent; treat that as the
        // current directory so display paths stay relative basenames.
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(|parent| parent.to_path_buf())
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let entry_rel = entry_path
        .as_ref()
        .map(|p| relative_path_string(&base, std::path::Path::new(p)))
        .unwrap_or_else(|| "<entry>".to_string());

    let mut entries: Vec<(String, String)> = vec![(entry_rel, sha256_hex(entry_source.as_bytes()))];
    for (path, content) in imports {
        // Bundled stdlib modules have no on-disk location; their virtual
        // `stdlib:<name>.dvl` path is kept verbatim (DEVL-131).
        let display = match path.to_str() {
            Some(text) if text.starts_with(STDLIB_PATH_PREFIX) => text.to_string(),
            _ => relative_path_string(&base, path),
        };
        entries.push((display, sha256_hex(content.as_bytes())));
    }
    // Deterministic ordering for the display manifest, independent of read
    // order and of duplicate reads across class methods.
    entries.sort();
    entries.dedup();

    // Path-independent content hash: sort the content hashes on their own so
    // the result is reproducible regardless of where the files live on disk.
    let mut content_hashes: Vec<&str> = entries.iter().map(|(_, hash)| hash.as_str()).collect();
    content_hashes.sort_unstable();
    content_hashes.dedup();
    let hash = sha256_hex(content_hashes.join("\n").as_bytes());

    let files = entries
        .into_iter()
        .map(|(path, hash)| json!({ "path": path, "sha256": hash }))
        .collect();

    SourceClosure {
        hash,
        files,
        stdlib_modules,
    }
}

fn resolve_import_path(
    import_path: &str,
    source_path: &Option<String>,
    search_paths: &[String],
) -> Option<std::path::PathBuf> {
    let path = std::path::Path::new(import_path);

    // 0. Absolute path
    if path.is_absolute() && path.is_file() {
        return Some(path.to_path_buf());
    }
    // 1. Relative to current file
    if let Some(ref sp) = source_path {
        if let Some(parent) = std::path::Path::new(sp).parent() {
            let candidate = parent.join(import_path);
            if candidate.is_file() {
                return Some(candidate);
            }
            for dir in devlish_project_import_dirs(parent) {
                let candidate = dir.join(import_path);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    // 2. Search paths (DEVLISH_PATH, ~/.devlish/lib/)
    for dir in search_paths {
        let candidate = std::path::Path::new(dir).join(import_path);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn devlish_project_import_dirs(start: &std::path::Path) -> Vec<std::path::PathBuf> {
    for ancestor in start.ancestors() {
        if ancestor.join("devlish.toml").is_file() {
            return vec![
                ancestor.to_path_buf(),
                ancestor.join("devlish"),
                ancestor.join("lib"),
            ]
            .into_iter()
            .filter(|path| path.is_dir())
            .collect();
        }
    }
    Vec::new()
}

fn resolve_imports(
    statements: &mut Vec<Statement>,
    options: &CompileOptions,
    imported: &mut HashSet<String>,
    active: &mut HashSet<String>,
    collected: &mut Vec<(std::path::PathBuf, String)>,
    // Names the enclosing scope already owns that an imported symbol must not
    // shadow. For class methods this is the method's parameter names, so an
    // imported fragment defining a name matching a caller-supplied param is a
    // hard error rather than a silent overwrite (DEVL-127). Empty for
    // top-level programs.
    reserved: &HashSet<String>,
    // Module name -> exported (unmangled) symbol names, for every module
    // brought in with `Use` anywhere in this compilation unit (a cache; see
    // the returned visibility set). Consumed by resolve_qualified_refs
    // (DEVL-131).
    module_exports: &mut HashMap<String, HashSet<String>>,
) -> Result<HashSet<String>, CompileError> {
    // Modules THIS scope brought in with a direct `Use` (plus those of flat
    // `Import`ed files, which are textual includes of this scope). Qualified
    // references may only reach these: a module a dependency happened to Use
    // transitively is not visible here (DEVL-131 review).
    let mut direct_uses: HashSet<String> = HashSet::new();
    let mut local_imports = HashSet::new();
    let mut i = 0;
    while i < statements.len() {
        if let StatementKind::UseModule {
            ref module,
            ref symbols,
        } = statements[i].kind
        {
            let module = module.clone();
            let symbols = symbols.clone();
            let line = statements[i].line;
            let source_text = statements[i].source_text.clone();

            // Repeated Use of a module is legal (like Python's `import math`
            // plus `from math import tau`); the module body inlines once and
            // clashing selective aliases surface as name collisions.
            let canonical = format!("use-module:{module}");
            if active.contains(&canonical) {
                // A whole-module circular Use edge is dropped once seen, like
                // Import cycles. A SELECTIVE Use cannot be dropped silently:
                // its aliases would never be created and every bound symbol
                // would evaluate to null (DEVL-131). Fail loudly instead.
                if !symbols.is_empty() {
                    return Err(CompileError::single(
                        line,
                        format!(
                            "Circular Use: cannot selectively bind symbols from the \
                             {module} module while it is still being resolved"
                        ),
                        &source_text,
                    ));
                }
                statements.remove(i);
                continue;
            }

            direct_uses.insert(module.clone());
            let mut fragment = Vec::new();
            if imported.insert(canonical.clone()) {
                let (module_source, closure_path, child_source_path) =
                    locate_module(&module, options, line, &source_text)?;
                collected.push((closure_path, module_source.clone()));

                let module_program = parse_source(&module_source)?;
                let mut module_stmts = module_program.statements;
                let child_options = CompileOptions {
                    source_path: child_source_path,
                    search_paths: options.search_paths.clone(),
                };
                active.insert(canonical.clone());
                let child_direct = resolve_imports(
                    &mut module_stmts,
                    &child_options,
                    imported,
                    active,
                    collected,
                    &HashSet::new(),
                    module_exports,
                )?;
                active.remove(&canonical);

                // The module's own qualified refs resolve against the modules
                // IT directly used, before its symbols are mangled. Resolved
                // refs become mangled names of other modules, which the
                // rename pass below never touches.
                resolve_qualified_refs(
                    &mut module_stmts,
                    &filtered_exports(module_exports, &child_direct),
                )?;

                // The module's own (unmangled) definitions are its exports.
                // Symbols of modules it transitively used are already mangled
                // and stay internal.
                let mut exports = defined_symbols(&module_stmts);
                exports.retain(|symbol| !symbol.starts_with(MODULE_MANGLE_PREFIX));

                let rename: HashMap<String, String> = exports
                    .iter()
                    .map(|symbol| (symbol.clone(), mangle_module_symbol(&module, symbol)))
                    .collect();
                for stmt in &mut module_stmts {
                    rename_symbols_in_statement(stmt, &rename);
                }
                module_exports.insert(module.clone(), exports);
                fragment = module_stmts;
            }

            // Selective form: bind the chosen symbols to their unqualified
            // names at the Use site, collision-checked like flat imports.
            let mut aliases = Vec::new();
            if !symbols.is_empty() {
                let exports = module_exports.get(&module).cloned().unwrap_or_default();
                for symbol in &symbols {
                    if !exports.contains(symbol) {
                        return Err(CompileError::single(
                            line,
                            format!("The {module} module does not define '{symbol}'"),
                            &source_text,
                        ));
                    }
                }
                let mut local_symbols = HashSet::new();
                for (index, stmt) in statements.iter().enumerate() {
                    if index != i {
                        collect_defined_symbols(stmt, &mut local_symbols);
                    }
                }
                if let Some(symbol) = symbols
                    .iter()
                    .find(|s| local_symbols.contains(*s) || reserved.contains(*s))
                {
                    return Err(CompileError::single(
                        line,
                        format!(
                            "Use name collision: {symbol} is defined by the {module} module \
                             and this file"
                        ),
                        &source_text,
                    ));
                }
                for symbol in &symbols {
                    aliases.push(Statement {
                        line,
                        source_text: source_text.clone(),
                        kind: StatementKind::Assignment {
                            target: symbol.clone(),
                            value: Expression::Variable(mangle_module_symbol(&module, symbol)),
                        },
                    });
                }
            }

            statements.remove(i);
            let mut insert_at = i;
            for stmt in fragment.into_iter().chain(aliases) {
                statements.insert(insert_at, stmt);
                insert_at += 1;
            }
            // The fragment is already fully resolved; skip past it.
            i = insert_at;
        } else if let StatementKind::Import { ref path } = statements[i].kind {
            let import_path = path.clone();
            let line = statements[i].line;
            let source_text = statements[i].source_text.clone();

            let resolved =
                resolve_import_path(&import_path, &options.source_path, &options.search_paths)
                    .ok_or_else(|| {
                        CompileError::single(
                            line,
                            format!("Import not found: {import_path}"),
                            &source_text,
                        )
                    })?;

            // The `stdlib:` prefix marks bundled stdlib provenance in the
            // source closure and package metadata. A disk file that smuggles
            // it into its path (a file literally named `stdlib:math.dvl`)
            // would masquerade as audited bundled code, so refuse it
            // (DEVL-131 review).
            if resolved.components().any(|component| {
                component
                    .as_os_str()
                    .to_string_lossy()
                    .starts_with(STDLIB_PATH_PREFIX)
            }) {
                return Err(CompileError::single(
                    line,
                    format!(
                        "Import path {} uses the reserved '{STDLIB_PATH_PREFIX}' prefix, \
                         which marks bundled standard-library provenance",
                        resolved.display()
                    ),
                    &source_text,
                ));
            }

            let canonical = normalize_path(&resolved.to_string_lossy());
            if !local_imports.insert(canonical.clone()) {
                return Err(CompileError::single(
                    line,
                    format!(
                        "Duplicate import: {import_path} already resolves to {}",
                        resolved.display()
                    ),
                    &source_text,
                ));
            }
            if active.contains(&canonical) {
                // Circular imports are ignored after the active edge is seen.
                statements.remove(i);
                continue;
            }
            if !imported.insert(canonical.clone()) {
                // A transitive import already brought this file in for the
                // current compilation unit, so do not inline it twice.
                statements.remove(i);
                continue;
            }

            let imported_source = std::fs::read_to_string(&resolved).map_err(|err| {
                CompileError::single(
                    line,
                    format!("Failed to read import {}: {err}", resolved.display()),
                    &source_text,
                )
            })?;
            collected.push((resolved.clone(), imported_source.clone()));

            let imported_program = parse_source(&imported_source)?;
            let mut imported_stmts = imported_program.statements;

            // Recursively resolve imports in the imported file
            let child_options = CompileOptions {
                source_path: Some(resolved.to_string_lossy().to_string()),
                search_paths: options.search_paths.clone(),
            };
            active.insert(canonical.clone());
            let child_direct = resolve_imports(
                &mut imported_stmts,
                &child_options,
                imported,
                active,
                collected,
                reserved,
                module_exports,
            )?;
            active.remove(&canonical);
            // A flat Import is a textual include: modules the imported file
            // Used are visible to this scope, exactly as if its statements
            // had been written here.
            direct_uses.extend(child_direct);

            if let Some(symbol) = first_symbol_collision(&imported_stmts, statements, i, reserved) {
                return Err(CompileError::single(
                    line,
                    format!(
                        "Import name collision: {symbol} is defined by {import_path} and this file"
                    ),
                    &source_text,
                ));
            }

            // Replace the Import statement with the imported statements
            statements.remove(i);
            for (j, stmt) in imported_stmts.into_iter().enumerate() {
                statements.insert(i + j, stmt);
            }
            // Don't increment i; re-check the newly inserted statements
        } else {
            i += 1;
        }
    }
    Ok(direct_uses)
}

/// Restricts the module export cache to the modules a scope directly Used, so
/// qualified references cannot reach transitive dependencies (DEVL-131).
fn filtered_exports(
    module_exports: &HashMap<String, HashSet<String>>,
    visible: &HashSet<String>,
) -> HashMap<String, HashSet<String>> {
    module_exports
        .iter()
        .filter(|(name, _)| visible.contains(*name))
        .map(|(name, exports)| (name.clone(), exports.clone()))
        .collect()
}

/// Prefix for compiler-internal namespaced symbols. Underscore-leading names
/// cannot be written in Devlish source (sanitize_name trims underscores), so
/// mangled names can never collide with user symbols.
const MODULE_MANGLE_PREFIX: &str = "__module_";

fn mangle_module_symbol(module: &str, symbol: &str) -> String {
    // Length-prefix the module name so the mangling is injective even when
    // identifiers contain literal double underscores: ("a__b", "c") and
    // ("a", "b__c") must not both mangle to "__module_a__b__c".
    format!("{MODULE_MANGLE_PREFIX}{}_{module}__{symbol}", module.len())
}

/// Virtual path prefix for bundled stdlib sources in the source closure
/// (`stdlib:math.dvl`). Bundled modules have no on-disk location (DEVL-131).
const STDLIB_PATH_PREFIX: &str = "stdlib:";

/// Standard library modules embedded in the toolchain binary (DEVL-131).
/// Resolved by well-known name ahead of DEVLISH_PATH and ~/.devlish/lib.
const BUNDLED_STDLIB: &[(&str, &str)] = &[("math", include_str!("../../../stdlib/math.dvl"))];

/// The stdlib version shipped with this compiler, recorded in packages that
/// use a bundled module.
pub const STDLIB_VERSION: &str = env!("CARGO_PKG_VERSION");

fn bundled_stdlib_source(name: &str) -> Option<&'static str> {
    BUNDLED_STDLIB
        .iter()
        .find(|(module, _)| *module == name)
        .map(|(_, source)| *source)
}

/// Finds the source for a `Use`d module: the bundled stdlib first, then the
/// import search machinery looking for `<name>.dvl`. Returns the source text,
/// the path recorded in the source closure, and the source_path child modules
/// resolve their own imports against (None for bundled modules, which have no
/// on-disk location).
fn locate_module(
    module: &str,
    options: &CompileOptions,
    line: usize,
    source_text: &str,
) -> Result<(String, std::path::PathBuf, Option<String>), CompileError> {
    if let Some(bundled) = bundled_stdlib_source(module) {
        return Ok((
            bundled.to_string(),
            std::path::PathBuf::from(format!("{STDLIB_PATH_PREFIX}{module}.dvl")),
            None,
        ));
    }
    let file = format!("{module}.dvl");
    let resolved = resolve_import_path(&file, &options.source_path, &options.search_paths)
        .ok_or_else(|| {
            CompileError::single(
                line,
                format!(
                    "Unknown module: {module}. It is not in the bundled standard library \
                     and {file} was not found on the module search path"
                ),
                source_text,
            )
        })?;
    let content = std::fs::read_to_string(&resolved).map_err(|err| {
        CompileError::single(
            line,
            format!("Failed to read module {}: {err}", resolved.display()),
            source_text,
        )
    })?;
    let child_source_path = Some(resolved.to_string_lossy().to_string());
    Ok((content, resolved, child_source_path))
}

/// Renames every definition and reference of the mapped symbols inside one
/// statement (recursing into child blocks). Used to move a `Use`d module's
/// symbols into their mangled namespace. Mirrors the coverage of
/// collect_defined_symbols (targets) and each_expression_in_statement
/// (references).
fn rename_symbols_in_statement(statement: &mut Statement, rename: &HashMap<String, String>) {
    let rename_string = |name: &mut String| {
        if let Some(new_name) = rename.get(name.as_str()) {
            *name = new_name.clone();
        }
    };
    match &mut statement.kind {
        StatementKind::Input { target, .. }
        | StatementKind::Assignment { target, .. }
        | StatementKind::FileRead { target, .. }
        | StatementKind::ReadXlsxCell { target, .. }
        | StatementKind::ReadPdfText { target, .. }
        | StatementKind::ReadDocxText { target, .. }
        | StatementKind::ReadStdin { target }
        | StatementKind::ConditionalAssignment { target, .. }
        | StatementKind::Append { target, .. } => rename_string(target),
        StatementKind::ForEach { item, .. } => rename_string(item),
        StatementKind::Pop { store_as, .. } | StatementKind::Extract { store_as, .. } => {
            rename_string(store_as)
        }
        StatementKind::Bind { target_name, .. } => rename_string(target_name),
        StatementKind::HttpRequest { dest, .. }
        | StatementKind::XlsxReadRows { dest, .. }
        | StatementKind::FileExists { dest, .. }
        | StatementKind::FileStat { dest, .. }
        | StatementKind::FileList { dest, .. }
        | StatementKind::FileGlob { dest, .. } => rename_string(dest),
        StatementKind::Load { path, alias } => {
            let current = alias
                .as_deref()
                .unwrap_or_else(|| path.as_deref().unwrap_or("document"));
            let symbol = sanitize_name(current);
            if let Some(new_name) = rename.get(&symbol) {
                *alias = Some(new_name.clone());
            }
        }
        // SetField targets are expressions; their roots are renamed by the
        // expression pass below. The remaining kinds define no symbols
        // (mirrors collect_defined_symbols). Listed explicitly so a new
        // StatementKind with a target forces a compile error here (DEVL-127).
        StatementKind::Branch { .. }
        | StatementKind::WhileLoop { .. }
        | StatementKind::UntilLoop { .. }
        | StatementKind::TryRecover { .. }
        | StatementKind::SetField { .. }
        | StatementKind::Output { .. }
        | StatementKind::FileWrite { .. }
        | StatementKind::Assertion { .. }
        | StatementKind::ExportAssertions { .. }
        | StatementKind::Break
        | StatementKind::Continue
        | StatementKind::Fail { .. }
        | StatementKind::Require { .. }
        | StatementKind::Definition { .. }
        | StatementKind::Validate { .. }
        | StatementKind::DocumentRequirement { .. }
        | StatementKind::Route { .. }
        | StatementKind::ServiceCall { .. }
        | StatementKind::Import { .. }
        | StatementKind::UseModule { .. }
        | StatementKind::Trigger { .. }
        | StatementKind::RespondWith { .. }
        | StatementKind::HttpDownload { .. }
        | StatementKind::Checkpoint { .. }
        | StatementKind::FileCopy { .. }
        | StatementKind::FileMove { .. }
        | StatementKind::FileMkdir { .. }
        | StatementKind::FileDelete { .. } => {}
    }
    each_expression_in_statement_mut(statement, &mut |expr| rename_expression(expr, rename));
    for block in child_statement_blocks_mut(statement) {
        for child in block {
            rename_symbols_in_statement(child, rename);
        }
    }
}

fn rename_expression(expr: &mut Expression, rename: &HashMap<String, String>) {
    walk_expression_mut(expr, &mut |node| {
        match node {
            Expression::Variable(name) | Expression::MethodCall { name, .. } => {
                if let Some(new_name) = rename.get(name.as_str()) {
                    *name = new_name.clone();
                }
            }
            // Qualified refs inside a module fragment point at modules that
            // fragment used; they resolve through the shared export table,
            // never through renaming.
            _ => {}
        }
    });
}

/// Applies `visit` to `expr` and every nested expression it owns.
fn walk_expression_mut(expr: &mut Expression, visit: &mut impl FnMut(&mut Expression)) {
    visit(expr);
    match expr {
        Expression::Binary { left, right, .. }
        | Expression::Comparison { left, right, .. }
        | Expression::LogicalAnd { left, right }
        | Expression::LogicalOr { left, right }
        | Expression::Contains { left, right }
        | Expression::StartsWith { left, right }
        | Expression::EndsWith { left, right } => {
            walk_expression_mut(left, visit);
            walk_expression_mut(right, visit);
        }
        Expression::LogicalNot { operand } | Expression::IsMissing(operand) => {
            walk_expression_mut(operand, visit)
        }
        Expression::IsIn { value, collection } => {
            walk_expression_mut(value, visit);
            walk_expression_mut(collection, visit);
        }
        Expression::ListLiteral(items) => {
            for item in items {
                walk_expression_mut(item, visit);
            }
        }
        Expression::RecordLiteral(fields) => {
            for (_, value) in fields {
                walk_expression_mut(value, visit);
            }
        }
        Expression::FieldAccess { record, .. } => walk_expression_mut(record, visit),
        Expression::BuiltinCall { arguments, .. } | Expression::MethodCall { arguments, .. } => {
            for argument in arguments {
                walk_expression_mut(argument, visit);
            }
        }
        Expression::Comprehension {
            list,
            accumulator,
            body,
            ..
        } => {
            walk_expression_mut(list, visit);
            if let Some((_, init)) = accumulator {
                walk_expression_mut(init, visit);
            }
            walk_expression_mut(body, visit);
        }
        Expression::Literal(_) | Expression::Variable(_) | Expression::QualifiedRef { .. } => {}
    }
}

/// Validates literal arguments to builtins at compile time with the exact
/// code the VM runs, so a bad regex pattern (DEVL-133) or a bad decimal /
/// zero-denominator fraction literal (DEVL-134) is a compile error, not a
/// runtime surprise. Dynamic arguments still fail loudly at runtime.
fn validate_literal_arguments(statements: &[Statement]) -> Result<(), CompileError> {
    for statement in statements {
        let mut error: Option<String> = None;
        each_expression_in_statement(statement, &mut |expr| {
            if error.is_none() {
                error = find_invalid_literal_argument(expr);
            }
        });
        if let Some(message) = error {
            return Err(CompileError::single(
                statement.line,
                message,
                &statement.source_text,
            ));
        }
        for block in child_statement_blocks(statement) {
            validate_literal_arguments(block)?;
        }
    }
    Ok(())
}

fn find_invalid_literal_argument(expr: &Expression) -> Option<String> {
    let mut found = None;
    // The only expression walker is the mutable one; clone to reuse it.
    let mut clone = expr.clone();
    walk_expression_mut(&mut clone, &mut |node| {
        if found.is_some() {
            return;
        }
        if let Expression::BuiltinCall { name, arguments } = node {
            if name.starts_with("regex_") {
                let flags_index = if name == "regex_replace" { 3 } else { 2 };
                let Some(Expression::Literal(Value::String(pattern))) = arguments.get(1) else {
                    return;
                };
                let flags = match arguments.get(flags_index) {
                    Some(Expression::Literal(Value::String(flags))) => flags.clone(),
                    _ => String::new(),
                };
                if let Err(message) = devlish_vm::compile_regex(pattern, &flags) {
                    found = Some(message);
                }
            } else if name == "to_decimal" {
                if let Some(Expression::Literal(Value::String(text))) = arguments.first() {
                    if let Err(message) = devlish_vm::parse_decimal(text) {
                        found = Some(message);
                    }
                }
            } else if name == "to_fraction" {
                if let (
                    Some(Expression::Literal(Value::Number(n))),
                    Some(Expression::Literal(Value::Number(d))),
                ) = (arguments.first(), arguments.get(1))
                {
                    if let (Some(n), Some(d)) = (n.as_i64(), d.as_i64()) {
                        if let Err(message) = devlish_vm::fraction_json(n, d) {
                            found = Some(message);
                        }
                    }
                }
            }
        }
    });
    found
}

/// Errors on any `Use` statement that survived import resolution. Resolution
/// only walks top-level statements, so a survivor is nested inside a Branch,
/// loop, or Try block, where it would otherwise compile to a silent no-op and
/// every symbol it promises would evaluate to null (DEVL-131). Fail loudly
/// instead.
fn reject_nested_use(statements: &[Statement]) -> Result<(), CompileError> {
    for statement in statements {
        if let StatementKind::UseModule { module, .. } = &statement.kind {
            return Err(CompileError::single(
                statement.line,
                format!(
                    "Use statements must be at the top level of the file, not inside a \
                     block: Use the {module} module"
                ),
                &statement.source_text,
            ));
        }
        for block in child_statement_blocks(statement) {
            reject_nested_use(block)?;
        }
    }
    Ok(())
}

/// Rewrites every `QualifiedRef` (`math's pi`) into its module's mangled
/// symbol, erroring on unknown modules or symbols. Runs after resolve_imports,
/// before lint and bytecode emission (DEVL-131).
fn resolve_qualified_refs(
    statements: &mut [Statement],
    module_exports: &HashMap<String, HashSet<String>>,
) -> Result<(), CompileError> {
    for statement in statements.iter_mut() {
        let line = statement.line;
        let source_text = statement.source_text.clone();
        let mut error: Option<String> = None;
        each_expression_in_statement_mut(statement, &mut |expr| {
            rewrite_qualified_refs(expr, module_exports, &mut error);
        });
        if let Some(message) = error {
            return Err(CompileError::single(line, message, &source_text));
        }
        for block in child_statement_blocks_mut(statement) {
            resolve_qualified_refs(block, module_exports)?;
        }
    }
    Ok(())
}

fn rewrite_qualified_refs(
    expr: &mut Expression,
    module_exports: &HashMap<String, HashSet<String>>,
    error: &mut Option<String>,
) {
    walk_expression_mut(expr, &mut |node| {
        if let Expression::QualifiedRef { module, name } = node {
            match module_exports.get(module.as_str()) {
                None => {
                    if error.is_none() {
                        *error = Some(format!(
                            "Unknown module '{module}' in qualified reference \
                             {module}'s {name}. Add 'Use the {module} module.' first"
                        ));
                    }
                }
                Some(exports) if !exports.contains(name.as_str()) => {
                    if error.is_none() {
                        *error =
                            Some(format!("The {module} module does not define '{name}'"));
                    }
                }
                Some(_) => {
                    *node = Expression::Variable(mangle_module_symbol(module, name));
                }
            }
        }
    });
}

/// Resolves a qualified ref in a standalone expression (a class method's
/// return value), reporting errors against the method's declaration site.
fn resolve_qualified_refs_in_expression(
    expr: &mut Expression,
    module_exports: &HashMap<String, HashSet<String>>,
    line: usize,
    source_text: &str,
) -> Result<(), CompileError> {
    let mut error: Option<String> = None;
    rewrite_qualified_refs(expr, module_exports, &mut error);
    match error {
        Some(message) => Err(CompileError::single(line, message, source_text)),
        None => Ok(()),
    }
}

fn first_symbol_collision(
    imported_statements: &[Statement],
    local_statements: &[Statement],
    import_index: usize,
    reserved: &HashSet<String>,
) -> Option<String> {
    let mut imported_symbols: Vec<String> =
        defined_symbols(imported_statements).into_iter().collect();
    imported_symbols.sort();
    if imported_symbols.is_empty() {
        return None;
    }

    let mut local_symbols = HashSet::new();
    for (index, statement) in local_statements.iter().enumerate() {
        if index != import_index {
            collect_defined_symbols(statement, &mut local_symbols);
        }
    }

    imported_symbols
        .into_iter()
        .find(|symbol| local_symbols.contains(symbol) || reserved.contains(symbol))
}

fn defined_symbols(statements: &[Statement]) -> HashSet<String> {
    let mut symbols = HashSet::new();
    for statement in statements {
        collect_defined_symbols(statement, &mut symbols);
    }
    symbols
}

fn collect_defined_symbols(statement: &Statement, symbols: &mut HashSet<String>) {
    match &statement.kind {
        // A selective Use binds its chosen symbols at the Use site once
        // resolved, so collision detection must see them (DEVL-131).
        StatementKind::UseModule {
            symbols: use_symbols,
            ..
        } => {
            for symbol in use_symbols {
                symbols.insert(symbol.clone());
            }
        }
        StatementKind::Input { target, .. }
        | StatementKind::Assignment { target, .. }
        | StatementKind::FileRead { target, .. }
        | StatementKind::ReadXlsxCell { target, .. }
        | StatementKind::ReadPdfText { target, .. }
        | StatementKind::ReadDocxText { target, .. }
        | StatementKind::ReadStdin { target }
        | StatementKind::ConditionalAssignment { target, .. } => {
            symbols.insert(target.clone());
        }
        StatementKind::Branch {
            then_statements,
            else_statements,
            ..
        } => {
            for child in then_statements.iter().chain(else_statements.iter()) {
                collect_defined_symbols(child, symbols);
            }
        }
        StatementKind::WhileLoop { body, .. } | StatementKind::UntilLoop { body, .. } => {
            for child in body {
                collect_defined_symbols(child, symbols);
            }
        }
        StatementKind::TryRecover { body, recovery } => {
            for child in body.iter().chain(recovery.iter()) {
                collect_defined_symbols(child, symbols);
            }
        }
        StatementKind::ForEach { item, body, .. } => {
            symbols.insert(item.clone());
            for child in body {
                collect_defined_symbols(child, symbols);
            }
        }
        StatementKind::SetField { target, .. } => {
            if let Some((root, _fields)) = field_path(target) {
                symbols.insert(root);
            }
        }
        StatementKind::Append { target, .. } => {
            symbols.insert(target.clone());
        }
        StatementKind::Pop { store_as, .. } => {
            symbols.insert(store_as.clone());
        }
        StatementKind::Bind { target_name, .. } => {
            symbols.insert(target_name.clone());
        }
        StatementKind::Load { path, alias } => {
            let alias = alias
                .as_deref()
                .unwrap_or_else(|| path.as_deref().unwrap_or("document"));
            symbols.insert(sanitize_name(alias));
        }
        StatementKind::Extract { store_as, .. } => {
            symbols.insert(store_as.clone());
        }
        StatementKind::HttpRequest { dest, .. } => {
            symbols.insert(dest.clone());
        }
        StatementKind::XlsxReadRows { dest, .. } => {
            symbols.insert(dest.clone());
        }
        StatementKind::FileExists { dest, .. }
        | StatementKind::FileStat { dest, .. }
        | StatementKind::FileList { dest, .. } => {
            symbols.insert(dest.clone());
        }
        StatementKind::FileGlob { dest, .. } => {
            symbols.insert(dest.clone());
        }
        StatementKind::FileCopy { .. }
        | StatementKind::FileMove { .. }
        | StatementKind::FileMkdir { .. }
        | StatementKind::FileDelete { .. } => {}
        StatementKind::HttpDownload { .. }
        | StatementKind::RespondWith { .. }
        | StatementKind::Output { .. }
        | StatementKind::FileWrite { .. }
        | StatementKind::Assertion { .. }
        | StatementKind::ExportAssertions { .. }
        | StatementKind::Break
        | StatementKind::Continue
        | StatementKind::Fail { .. }
        | StatementKind::Require { .. }
        | StatementKind::Definition { .. }
        | StatementKind::Validate { .. }
        | StatementKind::DocumentRequirement { .. }
        | StatementKind::Route { .. }
        | StatementKind::ServiceCall { .. }
        | StatementKind::Import { .. }
        | StatementKind::Trigger { .. }
        | StatementKind::Checkpoint { .. } => {}
    }
}

fn detect_and_parse(source: &str) -> Result<ParsedSource, CompileError> {
    // Find the first non-comment, non-blank, non-import line
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.starts_with("import ") {
            continue;
        }
        // Check if it matches a class declaration: `<Module>'s <Class>`. The
        // possessive owner must be a single identifier so a statement that
        // merely contains a qualified reference (`Set r to math's pi`,
        // DEVL-131) is not misread as a class header.
        if trimmed
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase())
        {
            if let Some((owner, rest)) = trimmed.split_once("'s ") {
                let rest = rest.trim().trim_end_matches(':');
                if is_identifier_text(owner)
                    && !rest.is_empty()
                    && rest
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == ' ' || ch == '_')
                {
                    return Ok(ParsedSource::Class(parse_class_source(source)?));
                }
            }
        }
        break;
    }
    Ok(ParsedSource::Flat(parse_source(source)?))
}

pub fn compile_source_to_json(
    source: &str,
    options: CompileOptions,
) -> Result<String, CompileError> {
    let package = compile_source(source, options)?;
    serde_json::to_string_pretty(&package).map_err(|error| CompileError {
        diagnostics: vec![Diagnostic::new(0, error.to_string(), "")],
    })
}

/// A non-fatal lint finding. Distinct from `Diagnostic`, which carries hard
/// compile errors.
#[derive(Debug, Clone)]
pub struct LintWarning {
    pub line: usize,
    pub message: String,
    pub source_text: String,
}

/// Analyzes `source` for non-fatal issues after a successful parse. Currently
/// reports identifiers referenced before they are ever bound by an assignment,
/// `Ask`, import, loop, or other binding construct (DEVL-127) — the class of
/// typo that used to evaluate silently to null. Method parameters, class
/// fields, loop variables, imported symbols, defined terms, and builtins are
/// treated as bound and never warned about, to keep false positives low.
///
/// Returns `Err` only when the source cannot be parsed at all; a clean parse
/// with no findings yields an empty vector.
pub fn lint_source(source: &str, options: CompileOptions) -> Result<Vec<LintWarning>, CompileError> {
    let mut collected: Vec<(std::path::PathBuf, String)> = Vec::new();
    let mut warnings = Vec::new();
    let mut reported = HashSet::new();
    match detect_and_parse(source)? {
        ParsedSource::Flat(mut program) => {
            let mut imported = HashSet::new();
            let mut active = HashSet::new();
            if let Some(ref sp) = options.source_path {
                active.insert(normalize_path(sp));
            }
            let mut module_exports = HashMap::new();
            let direct_uses = resolve_imports(
                &mut program.statements,
                &options,
                &mut imported,
                &mut active,
                &mut collected,
                &HashSet::new(),
                &mut module_exports,
            )?;
            reject_nested_use(&program.statements)?;
            resolve_qualified_refs(
                &mut program.statements,
                &filtered_exports(&module_exports, &direct_uses),
            )?;
            let mut bound = defined_symbols(&program.statements);
            collect_definition_names(&program.statements, &mut bound);
            // Declared `--input` variables are bound at runtime, so a program
            // that declares them in its `inputs:` manifest must not warn on them.
            if let Some(manifest) = &program.manifest {
                for input in &manifest.inputs {
                    bound.insert(sanitize_name(input));
                }
            }
            lint_unbound_refs(&program.statements, &bound, &mut warnings, &mut reported);
        }
        ParsedSource::Class(mut class_program) => {
            // Sibling method NAMES that any method may legitimately call. Params
            // are deliberately NOT unioned here: binding one method's params for
            // every method would mask a typo in method A whenever method B has a
            // matching param (DEVL-127). Each method binds only its own params.
            let mut method_names = HashSet::new();
            for method in &class_program.methods {
                method_names.insert(sanitize_name(&method.name));
                method_names.insert(method.ruby_name.clone());
            }
            for method in &mut class_program.methods {
                let mut imported = HashSet::new();
                let mut active = HashSet::new();
                if let Some(ref sp) = options.source_path {
                    active.insert(normalize_path(sp));
                }
                let params: HashSet<String> = method.params.iter().cloned().collect();
                let mut module_exports = HashMap::new();
                let direct_uses = resolve_imports(
                    &mut method.body,
                    &options,
                    &mut imported,
                    &mut active,
                    &mut collected,
                    &params,
                    &mut module_exports,
                )?;
                reject_nested_use(&method.body)?;
                let visible = filtered_exports(&module_exports, &direct_uses);
                resolve_qualified_refs(&mut method.body, &visible)?;
                if let Some(return_value) = &mut method.return_value {
                    resolve_qualified_refs_in_expression(
                        return_value,
                        &visible,
                        method.line,
                        &method.source_text,
                    )?;
                }
                let mut bound = method_names.clone();
                bound.extend(params);
                for symbol in defined_symbols(&method.body) {
                    bound.insert(symbol);
                }
                collect_definition_names(&method.body, &mut bound);
                lint_unbound_refs(&method.body, &bound, &mut warnings, &mut reported);
                // Return values are stripped out of `body` into `return_value`
                // during class parsing, so lint them separately or a misspelled
                // `Respond with X` would never warn (DEVL-127).
                if let Some(return_value) = &method.return_value {
                    let mut names = Vec::new();
                    collect_referenced_roots(return_value, &mut names);
                    report_unbound_names(
                        names,
                        &bound,
                        &mut reported,
                        method.line,
                        &method.source_text,
                        &mut warnings,
                    );
                }
            }
        }
    }
    Ok(warnings)
}

/// Adds the names of `X is Y` defined terms to the bound set so referencing a
/// defined term never trips the unbound-identifier lint.
fn collect_definition_names(statements: &[Statement], bound: &mut HashSet<String>) {
    for statement in statements {
        if let StatementKind::Definition { name, .. } = &statement.kind {
            bound.insert(sanitize_name(name));
        }
        for block in child_statement_blocks(statement) {
            collect_definition_names(block, bound);
        }
    }
}

/// Walks statements, warning once per name for any variable reference whose
/// root name is not in `bound`.
fn lint_unbound_refs(
    statements: &[Statement],
    bound: &HashSet<String>,
    warnings: &mut Vec<LintWarning>,
    reported: &mut HashSet<String>,
) {
    for statement in statements {
        let mut names = Vec::new();
        each_expression_in_statement(statement, &mut |expr| {
            collect_referenced_roots(expr, &mut names);
        });
        report_unbound_names(
            names,
            bound,
            reported,
            statement.line,
            &statement.source_text,
            warnings,
        );
        // Recurse into nested blocks so warnings carry the inner line number.
        for block in child_statement_blocks(statement) {
            lint_unbound_refs(block, bound, warnings, reported);
        }
    }
}

/// Emits one unbound-identifier warning per not-yet-reported name that is not in
/// `bound`. Shared by the statement walker and the class return-value check so
/// the wording stays identical.
fn report_unbound_names(
    names: Vec<String>,
    bound: &HashSet<String>,
    reported: &mut HashSet<String>,
    line: usize,
    source_text: &str,
    warnings: &mut Vec<LintWarning>,
) {
    for name in names {
        if name.is_empty() || bound.contains(&name) || !reported.insert(name.clone()) {
            continue;
        }
        warnings.push(LintWarning {
            line,
            message: format!(
                "'{name}' is used but never assigned, asked, or imported \
                 (possible typo). It will evaluate to null."
            ),
            source_text: source_text.to_string(),
        });
    }
}

/// Invokes `visit` on every top-level expression directly owned by a single
/// statement (not descending into nested statement blocks — the caller handles
/// those so line numbers stay attached to the right statement).
fn each_expression_in_statement(statement: &Statement, visit: &mut impl FnMut(&Expression)) {
    match &statement.kind {
        StatementKind::Assignment { value, .. }
        | StatementKind::Output { value }
        | StatementKind::RespondWith { value }
        | StatementKind::Fail { message: value } => visit(value),
        StatementKind::ConditionalAssignment {
            value, condition, ..
        } => {
            visit(value);
            visit(condition);
        }
        StatementKind::Input { prompt, .. } => visit(prompt),
        StatementKind::FileWrite { value, path, .. } => {
            visit(value);
            visit(path);
        }
        StatementKind::FileRead { path, .. } => visit(path),
        StatementKind::Branch { condition, .. } => visit(condition),
        StatementKind::WhileLoop { condition, .. } | StatementKind::UntilLoop { condition, .. } => {
            visit(condition)
        }
        StatementKind::ForEach { collection, .. } => visit(collection),
        StatementKind::Require { condition, message } => {
            visit(condition);
            if let Some(message) = message {
                visit(message);
            }
        }
        StatementKind::SetField {
            target,
            value,
            condition,
        } => {
            visit(target);
            visit(value);
            if let Some(condition) = condition {
                visit(condition);
            }
        }
        StatementKind::Append { value, .. } => visit(value),
        StatementKind::Assertion { expected, .. } => {
            if let Some(expected) = expected {
                visit(expected);
            }
        }
        StatementKind::Validate { target, value, .. } => {
            visit(target);
            if let Some(value) = value {
                visit(value);
            }
        }
        StatementKind::Route {
            source,
            destination,
        } => {
            visit(source);
            visit(destination);
        }
        StatementKind::ServiceCall { arguments, .. } => {
            for (_, expr) in arguments {
                visit(expr);
            }
        }
        StatementKind::HttpRequest { url, body, .. } => {
            visit(url);
            if let Some(body) = body {
                visit(body);
            }
        }
        StatementKind::HttpDownload { url, path } => {
            visit(url);
            visit(path);
        }
        StatementKind::XlsxReadRows { path, .. } => visit(path),
        StatementKind::Checkpoint { prompt, .. } => visit(prompt),
        StatementKind::FileCopy {
            source,
            destination,
        }
        | StatementKind::FileMove {
            source,
            destination,
        } => {
            visit(source);
            visit(destination);
        }
        StatementKind::FileMkdir { path }
        | StatementKind::FileDelete { path }
        | StatementKind::FileExists { path, .. }
        | StatementKind::FileStat { path, .. }
        | StatementKind::FileList { path, .. } => visit(path),
        StatementKind::FileGlob {
            pattern, directory, ..
        } => {
            visit(pattern);
            visit(directory);
        }
        StatementKind::ExportAssertions { path } => visit(path),
        // Statements with no directly-owned Expression fields (String-only or
        // nullary). Listed explicitly rather than via a catch-all so that adding
        // a new StatementKind with an Expression field forces a compile error
        // here instead of silently skipping it (DEVL-127).
        StatementKind::ReadXlsxCell { .. }
        | StatementKind::ReadPdfText { .. }
        | StatementKind::ReadDocxText { .. }
        | StatementKind::ReadStdin { .. }
        | StatementKind::Pop { .. }
        | StatementKind::Bind { .. }
        | StatementKind::Definition { .. }
        | StatementKind::Load { .. }
        | StatementKind::Extract { .. }
        | StatementKind::DocumentRequirement { .. }
        | StatementKind::Import { .. }
        | StatementKind::UseModule { .. }
        | StatementKind::Trigger { .. }
        | StatementKind::Break
        | StatementKind::Continue
        | StatementKind::TryRecover { .. } => {}
    }
}

/// Mutable mirror of `each_expression_in_statement`, for compile-time
/// rewrite passes (module symbol renaming, qualified-ref resolution). Kept
/// arm-for-arm identical with the immutable version (DEVL-131).
fn each_expression_in_statement_mut(
    statement: &mut Statement,
    visit: &mut impl FnMut(&mut Expression),
) {
    match &mut statement.kind {
        StatementKind::Assignment { value, .. }
        | StatementKind::Output { value }
        | StatementKind::RespondWith { value }
        | StatementKind::Fail { message: value } => visit(value),
        StatementKind::ConditionalAssignment {
            value, condition, ..
        } => {
            visit(value);
            visit(condition);
        }
        StatementKind::Input { prompt, .. } => visit(prompt),
        StatementKind::FileWrite { value, path, .. } => {
            visit(value);
            visit(path);
        }
        StatementKind::FileRead { path, .. } => visit(path),
        StatementKind::Branch { condition, .. } => visit(condition),
        StatementKind::WhileLoop { condition, .. } | StatementKind::UntilLoop { condition, .. } => {
            visit(condition)
        }
        StatementKind::ForEach { collection, .. } => visit(collection),
        StatementKind::Require { condition, message } => {
            visit(condition);
            if let Some(message) = message {
                visit(message);
            }
        }
        StatementKind::SetField {
            target,
            value,
            condition,
        } => {
            visit(target);
            visit(value);
            if let Some(condition) = condition {
                visit(condition);
            }
        }
        StatementKind::Append { value, .. } => visit(value),
        StatementKind::Assertion { expected, .. } => {
            if let Some(expected) = expected {
                visit(expected);
            }
        }
        StatementKind::Validate { target, value, .. } => {
            visit(target);
            if let Some(value) = value {
                visit(value);
            }
        }
        StatementKind::Route {
            source,
            destination,
        } => {
            visit(source);
            visit(destination);
        }
        StatementKind::ServiceCall { arguments, .. } => {
            for (_, expr) in arguments {
                visit(expr);
            }
        }
        StatementKind::HttpRequest { url, body, .. } => {
            visit(url);
            if let Some(body) = body {
                visit(body);
            }
        }
        StatementKind::HttpDownload { url, path } => {
            visit(url);
            visit(path);
        }
        StatementKind::XlsxReadRows { path, .. } => visit(path),
        StatementKind::Checkpoint { prompt, .. } => visit(prompt),
        StatementKind::FileCopy {
            source,
            destination,
        }
        | StatementKind::FileMove {
            source,
            destination,
        } => {
            visit(source);
            visit(destination);
        }
        StatementKind::FileMkdir { path }
        | StatementKind::FileDelete { path }
        | StatementKind::FileExists { path, .. }
        | StatementKind::FileStat { path, .. }
        | StatementKind::FileList { path, .. } => visit(path),
        StatementKind::FileGlob {
            pattern, directory, ..
        } => {
            visit(pattern);
            visit(directory);
        }
        StatementKind::ExportAssertions { path } => visit(path),
        StatementKind::ReadXlsxCell { .. }
        | StatementKind::ReadPdfText { .. }
        | StatementKind::ReadDocxText { .. }
        | StatementKind::ReadStdin { .. }
        | StatementKind::Pop { .. }
        | StatementKind::Bind { .. }
        | StatementKind::Definition { .. }
        | StatementKind::Load { .. }
        | StatementKind::Extract { .. }
        | StatementKind::DocumentRequirement { .. }
        | StatementKind::Import { .. }
        | StatementKind::UseModule { .. }
        | StatementKind::Trigger { .. }
        | StatementKind::Break
        | StatementKind::Continue
        | StatementKind::TryRecover { .. } => {}
    }
}

/// Returns the nested statement blocks a statement owns, so the statement-tree
/// walkers recurse uniformly. The match is exhaustive: adding a StatementKind
/// with child statements forces a compile error here (DEVL-127).
fn child_statement_blocks(statement: &Statement) -> Vec<&[Statement]> {
    match &statement.kind {
        StatementKind::Branch {
            then_statements,
            else_statements,
            ..
        } => vec![then_statements.as_slice(), else_statements.as_slice()],
        StatementKind::WhileLoop { body, .. }
        | StatementKind::UntilLoop { body, .. }
        | StatementKind::ForEach { body, .. } => vec![body.as_slice()],
        StatementKind::TryRecover { body, recovery } => {
            vec![body.as_slice(), recovery.as_slice()]
        }
        // Leaf statements: no nested statement blocks.
        StatementKind::Input { .. }
        | StatementKind::Assignment { .. }
        | StatementKind::Output { .. }
        | StatementKind::FileWrite { .. }
        | StatementKind::FileRead { .. }
        | StatementKind::ReadXlsxCell { .. }
        | StatementKind::ReadPdfText { .. }
        | StatementKind::ReadDocxText { .. }
        | StatementKind::Assertion { .. }
        | StatementKind::ExportAssertions { .. }
        | StatementKind::Break
        | StatementKind::Continue
        | StatementKind::Fail { .. }
        | StatementKind::Require { .. }
        | StatementKind::SetField { .. }
        | StatementKind::Append { .. }
        | StatementKind::Pop { .. }
        | StatementKind::ConditionalAssignment { .. }
        | StatementKind::Bind { .. }
        | StatementKind::Definition { .. }
        | StatementKind::Load { .. }
        | StatementKind::Extract { .. }
        | StatementKind::Validate { .. }
        | StatementKind::DocumentRequirement { .. }
        | StatementKind::Route { .. }
        | StatementKind::ServiceCall { .. }
        | StatementKind::Import { .. }
        | StatementKind::UseModule { .. }
        | StatementKind::ReadStdin { .. }
        | StatementKind::Trigger { .. }
        | StatementKind::RespondWith { .. }
        | StatementKind::HttpRequest { .. }
        | StatementKind::HttpDownload { .. }
        | StatementKind::XlsxReadRows { .. }
        | StatementKind::Checkpoint { .. }
        | StatementKind::FileCopy { .. }
        | StatementKind::FileMove { .. }
        | StatementKind::FileMkdir { .. }
        | StatementKind::FileDelete { .. }
        | StatementKind::FileExists { .. }
        | StatementKind::FileStat { .. }
        | StatementKind::FileList { .. }
        | StatementKind::FileGlob { .. } => Vec::new(),
    }
}

/// Mutable mirror of `child_statement_blocks` (DEVL-131). Exhaustive like its
/// immutable twin so a new StatementKind with child blocks forces a compile
/// error here instead of being silently skipped by the rewrite passes
/// (DEVL-127).
fn child_statement_blocks_mut(statement: &mut Statement) -> Vec<&mut Vec<Statement>> {
    match &mut statement.kind {
        StatementKind::Branch {
            then_statements,
            else_statements,
            ..
        } => vec![then_statements, else_statements],
        StatementKind::WhileLoop { body, .. }
        | StatementKind::UntilLoop { body, .. }
        | StatementKind::ForEach { body, .. } => vec![body],
        StatementKind::TryRecover { body, recovery } => vec![body, recovery],
        StatementKind::Input { .. }
        | StatementKind::Assignment { .. }
        | StatementKind::Output { .. }
        | StatementKind::FileWrite { .. }
        | StatementKind::FileRead { .. }
        | StatementKind::ReadXlsxCell { .. }
        | StatementKind::ReadPdfText { .. }
        | StatementKind::ReadDocxText { .. }
        | StatementKind::Assertion { .. }
        | StatementKind::ExportAssertions { .. }
        | StatementKind::Break
        | StatementKind::Continue
        | StatementKind::Fail { .. }
        | StatementKind::Require { .. }
        | StatementKind::SetField { .. }
        | StatementKind::Append { .. }
        | StatementKind::Pop { .. }
        | StatementKind::ConditionalAssignment { .. }
        | StatementKind::Bind { .. }
        | StatementKind::Definition { .. }
        | StatementKind::Load { .. }
        | StatementKind::Extract { .. }
        | StatementKind::Validate { .. }
        | StatementKind::DocumentRequirement { .. }
        | StatementKind::Route { .. }
        | StatementKind::ServiceCall { .. }
        | StatementKind::Import { .. }
        | StatementKind::UseModule { .. }
        | StatementKind::ReadStdin { .. }
        | StatementKind::Trigger { .. }
        | StatementKind::RespondWith { .. }
        | StatementKind::HttpRequest { .. }
        | StatementKind::HttpDownload { .. }
        | StatementKind::XlsxReadRows { .. }
        | StatementKind::Checkpoint { .. }
        | StatementKind::FileCopy { .. }
        | StatementKind::FileMove { .. }
        | StatementKind::FileMkdir { .. }
        | StatementKind::FileDelete { .. }
        | StatementKind::FileExists { .. }
        | StatementKind::FileStat { .. }
        | StatementKind::FileList { .. }
        | StatementKind::FileGlob { .. } => Vec::new(),
    }
}

/// Collects the root variable name of every variable reference inside `expr`.
/// Field access chains contribute their base record's name; builtin and method
/// names are not treated as variable references.
fn collect_referenced_roots(expr: &Expression, out: &mut Vec<String>) {
    match expr {
        Expression::Variable(name) => out.push(name.clone()),
        Expression::Binary { left, right, .. }
        | Expression::Comparison { left, right, .. }
        | Expression::LogicalAnd { left, right }
        | Expression::LogicalOr { left, right }
        | Expression::Contains { left, right }
        | Expression::StartsWith { left, right }
        | Expression::EndsWith { left, right } => {
            collect_referenced_roots(left, out);
            collect_referenced_roots(right, out);
        }
        Expression::LogicalNot { operand } | Expression::IsMissing(operand) => {
            collect_referenced_roots(operand, out)
        }
        Expression::IsIn { value, collection } => {
            collect_referenced_roots(value, out);
            collect_referenced_roots(collection, out);
        }
        Expression::ListLiteral(items) => {
            for item in items {
                collect_referenced_roots(item, out);
            }
        }
        Expression::RecordLiteral(fields) => {
            for (_, value) in fields {
                collect_referenced_roots(value, out);
            }
        }
        Expression::FieldAccess { record, .. } => collect_referenced_roots(record, out),
        Expression::BuiltinCall { arguments, .. } | Expression::MethodCall { arguments, .. } => {
            for argument in arguments {
                collect_referenced_roots(argument, out);
            }
        }
        Expression::Comprehension {
            kind: _,
            list,
            binding,
            accumulator,
            body,
        } => {
            collect_referenced_roots(list, out);
            if let Some((_, init)) = accumulator {
                collect_referenced_roots(init, out);
            }
            // The element binding (and reduce accumulator) are defined by the
            // loop itself, not free references.
            let mut body_roots = Vec::new();
            collect_referenced_roots(body, &mut body_roots);
            body_roots.retain(|name| {
                name != binding
                    && accumulator
                        .as_ref()
                        .map(|(acc, _)| name != acc)
                        .unwrap_or(true)
            });
            out.extend(body_roots);
        }
        // Qualified refs are rewritten into mangled Variables by
        // resolve_qualified_refs before any lint walk sees them.
        Expression::Literal(_) | Expression::QualifiedRef { .. } => {}
    }
}

fn parse_source(source: &str) -> Result<Program, CompileError> {
    let (manifest, remaining_source) = parse_manifest(source)?;

    let mut lines = Vec::new();
    for (index, original_line) in remaining_source.lines().enumerate() {
        let line_number = index + 1;
        let text = original_line.trim();
        if text.is_empty() || text.starts_with('#') {
            continue;
        }
        lines.push(SourceLine {
            line_number,
            indent: leading_spaces(original_line),
            text: text.to_string(),
        });
    }

    let mut index = 0usize;
    let statements = parse_block(&lines, &mut index, 0)?;
    if let Some(line) = lines.get(index) {
        return Err(CompileError::single(
            line.line_number,
            "Unexpected indentation",
            &line.text,
        ));
    }

    Ok(Program {
        statements,
        manifest: if manifest.is_empty() {
            None
        } else {
            Some(manifest)
        },
    })
}

fn parse_manifest(source: &str) -> Result<(ProgramManifest, &str), CompileError> {
    let mut manifest = ProgramManifest::default();
    let mut current_section: Option<&str> = None;
    let mut consumed_bytes = 0;
    let mut rule = RawRule::default();

    for (index, line) in source.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            consumed_bytes += line.len() + 1; // +1 for newline
            continue;
        }

        let lower = trimmed.to_ascii_lowercase();
        match lower.as_str() {
            "permissions:" => {
                current_section = Some("permissions");
                consumed_bytes += line.len() + 1;
                continue;
            }
            "boundaries:" => {
                current_section = Some("boundaries");
                consumed_bytes += line.len() + 1;
                continue;
            }
            "callers:" => {
                current_section = Some("callers");
                consumed_bytes += line.len() + 1;
                continue;
            }
            "inputs:" => {
                current_section = Some("inputs");
                consumed_bytes += line.len() + 1;
                continue;
            }
            "rule:" => {
                current_section = Some("rule");
                rule.seen = true;
                rule.header_line = line_number;
                consumed_bytes += line.len() + 1;
                continue;
            }
            _ => {}
        }

        // If we're in a manifest section and the line is indented, parse it
        if current_section.is_some() && leading_spaces(line) > 0 {
            match current_section.unwrap() {
                "permissions" => {
                    if let Some(perm) = parse_manifest_permission(trimmed) {
                        manifest.permissions.push(perm);
                    }
                }
                "boundaries" => {
                    manifest.boundaries.push(trimmed.to_string());
                }
                "callers" => {
                    manifest.callers.push(trimmed.to_string());
                }
                "inputs" => {
                    manifest.inputs.push(trimmed.to_string());
                }
                "rule" => {
                    parse_rule_field(trimmed, line_number, &mut rule)?;
                }
                _ => {}
            }
            consumed_bytes += line.len() + 1;
            continue;
        }

        // Not a manifest line; stop consuming
        break;
    }

    if rule.seen {
        manifest.rule = Some(rule.finish()?);
    }

    if manifest.is_empty() {
        return Ok((manifest, source));
    }

    let remaining = if consumed_bytes < source.len() {
        &source[consumed_bytes..]
    } else {
        ""
    };
    Ok((manifest, remaining))
}

/// Raw fields collected from a `Rule:` section before validation. Each value
/// carries the source line it appeared on so validation errors point at it.
#[derive(Default)]
struct RawRule {
    seen: bool,
    header_line: usize,
    id: Option<(String, usize)>,
    version: Option<(String, usize)>,
    author: Option<String>,
    effective_from: Option<(String, usize)>,
    effective_until: Option<(String, usize)>,
}

impl RawRule {
    /// Validates the collected fields and builds the final RuleMetadata.
    fn finish(self) -> Result<RuleMetadata, CompileError> {
        let header_line = self.header_line;
        let (id, id_line) = self.id.ok_or_else(|| {
            CompileError::single(header_line, "Rule section is missing 'id'", "Rule:")
        })?;
        if !is_dotted_identifier(&id) {
            return Err(CompileError::single(
                id_line,
                format!("Rule id '{id}' must be a dotted identifier (e.g. credit_verification.dti_check)"),
                &id,
            ));
        }

        let (version, version_line) = self.version.ok_or_else(|| {
            CompileError::single(id_line, "Rule section is missing 'version'", &id)
        })?;
        if !is_semver(&version) {
            return Err(CompileError::single(
                version_line,
                format!("Rule version '{version}' must be semantic MAJOR.MINOR.PATCH (e.g. 2.1.0)"),
                &version,
            ));
        }

        let effective_from = match self.effective_from {
            Some((value, line)) => {
                if parse_iso_date(&value).is_none() {
                    return Err(CompileError::single(
                        line,
                        format!("Rule 'effective from' date '{value}' must be YYYY-MM-DD"),
                        &value,
                    ));
                }
                Some(value)
            }
            None => None,
        };

        let effective_until = match self.effective_until {
            Some((value, line)) => {
                if parse_iso_date(&value).is_none() {
                    return Err(CompileError::single(
                        line,
                        format!("Rule 'effective until' date '{value}' must be YYYY-MM-DD"),
                        &value,
                    ));
                }
                // ISO dates are fixed-width, so lexical order equals chronological order.
                if let Some(ref from) = effective_from {
                    if &value < from {
                        return Err(CompileError::single(
                            line,
                            format!("Rule 'effective until' ({value}) is before 'effective from' ({from})"),
                            &value,
                        ));
                    }
                }
                Some(value)
            }
            None => None,
        };

        Ok(RuleMetadata {
            id,
            version,
            author: self.author,
            effective_from,
            effective_until,
        })
    }
}

/// Parses one indented line inside a `Rule:` section into `rule`.
fn parse_rule_field(
    line: &str,
    line_number: usize,
    rule: &mut RawRule,
) -> Result<(), CompileError> {
    let lower = line.to_ascii_lowercase();
    if lower.starts_with("effective from ") {
        let value = line["effective from ".len()..].trim().to_string();
        rule.effective_from = Some((value, line_number));
        return Ok(());
    }
    if lower.starts_with("effective until ") {
        let value = line["effective until ".len()..].trim().to_string();
        rule.effective_until = Some((value, line_number));
        return Ok(());
    }
    if let Some((key, value)) = line.split_once(':') {
        let value = value.trim().trim_matches('"').to_string();
        match key.trim().to_ascii_lowercase().as_str() {
            "id" => rule.id = Some((value, line_number)),
            "version" => rule.version = Some((value, line_number)),
            "author" => rule.author = Some(value),
            other => {
                return Err(CompileError::single(
                    line_number,
                    format!("Unknown Rule field '{other}' (expected id, version, author, effective from, effective until)"),
                    line,
                ));
            }
        }
        return Ok(());
    }
    Err(CompileError::single(
        line_number,
        "Unrecognized line in Rule section",
        line,
    ))
}

/// A dotted identifier: dot-separated segments, each starting with a letter and
/// continuing with letters, digits, or underscores.
fn is_dotted_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.split('.').all(|segment| {
            let mut chars = segment.chars();
            matches!(chars.next(), Some(first) if first.is_ascii_alphabetic())
                && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        })
}

/// Semantic version: three dot-separated non-negative integers, each without a
/// leading zero (so `1.2.3` is valid but `01.02.03` is not, matching semver).
fn is_semver(value: &str) -> bool {
    let parts: Vec<&str> = value.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|part| {
            !part.is_empty()
                && part.chars().all(|ch| ch.is_ascii_digit())
                && (part.len() == 1 || !part.starts_with('0'))
        })
}

/// Parses a strict `YYYY-MM-DD` calendar date, returning its components when
/// valid. Rejects impossible dates (e.g. `2026-02-31`, `2026-04-31`), honoring
/// month lengths and leap years, so an effective date always names a real day.
/// Exported so the CLI's `--as-of` uses the same validity rule as the compiler
/// and the devlish-runtime `isValidIsoDate`.
pub fn parse_iso_date(value: &str) -> Option<(u16, u8, u8)> {
    let parts: Vec<&str> = value.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return None;
    }
    let year: u16 = parts[0].parse().ok()?;
    let month: u8 = parts[1].parse().ok()?;
    let day: u8 = parts[2].parse().ok()?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    if !(1..=max_day).contains(&day) {
        return None;
    }
    Some((year, month, day))
}

fn parse_manifest_permission(line: &str) -> Option<ManifestPermission> {
    let lower = line.to_ascii_lowercase();

    // "Read files from <path>"
    if let Some(rest) = lower.strip_prefix("read files from ") {
        return Some(ManifestPermission {
            kind: "read_file".to_string(),
            scope: Some(rest.trim().trim_matches('"').to_string()),
        });
    }
    // "Read files"
    if lower == "read files" {
        return Some(ManifestPermission {
            kind: "read_file".to_string(),
            scope: None,
        });
    }
    // "Write files to <path>"
    if let Some(rest) = lower.strip_prefix("write files to ") {
        return Some(ManifestPermission {
            kind: "write_file".to_string(),
            scope: Some(rest.trim().trim_matches('"').to_string()),
        });
    }
    // "Write files"
    if lower == "write files" {
        return Some(ManifestPermission {
            kind: "write_file".to_string(),
            scope: None,
        });
    }
    // "Call <service> service"
    if let Some(rest) = lower.strip_prefix("call ") {
        let service = rest.trim_end_matches(" service").trim();
        return Some(ManifestPermission {
            kind: "service_call".to_string(),
            scope: Some(service.to_string()),
        });
    }
    // "HTTP requests" / "HTTP requests to <domain>"
    if let Some(rest) = lower.strip_prefix("http requests to ") {
        return Some(ManifestPermission {
            kind: "http_request".to_string(),
            scope: Some(rest.trim().trim_matches('"').to_string()),
        });
    }
    if lower == "http requests" {
        return Some(ManifestPermission {
            kind: "http_request".to_string(),
            scope: None,
        });
    }
    // "Filesystem operations" / "Filesystem operations on <path>"
    if let Some(rest) = lower.strip_prefix("filesystem operations on ") {
        return Some(ManifestPermission {
            kind: "filesystem".to_string(),
            scope: Some(rest.trim().trim_matches('"').to_string()),
        });
    }
    if lower == "filesystem operations" {
        return Some(ManifestPermission {
            kind: "filesystem".to_string(),
            scope: None,
        });
    }

    None
}

fn parse_class_source(source: &str) -> Result<ClassProgram, CompileError> {
    let mut module_name = String::new();
    let mut class_name = String::new();
    let mut parent_class: Option<(String, String)> = None;
    let mut class_body_lines: Vec<SourceLine> = Vec::new();
    let mut found_class_decl = false;
    let mut seen_method_header = false;

    for (index, original_line) in source.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = original_line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        // Only skip genuinely top-level imports (indent 0). Those sit outside any
        // method body and have no resolution path in a class program, so dropping
        // them preserves the original behavior. Imports indented inside a method
        // body must instead fall through to be collected as body lines, so they
        // become StatementKind::Import statements and get inlined by the
        // per-method resolve_imports pass in compile_source. See DEVL-123.
        if lower.starts_with("import ") && leading_spaces(original_line) == 0 {
            continue;
        }

        if !found_class_decl {
            // Parse class declaration: Module's ClassName [based on Parent's Class]:
            let decl = trimmed.trim_end_matches(':');
            if let Some(apostrophe_pos) = decl.find("'s ") {
                module_name = decl[..apostrophe_pos].to_string();
                let rest = &decl[apostrophe_pos + 3..];
                if let Some((cls, parent_part)) = split_once_ci(rest, " based on ") {
                    class_name = cls.trim().to_string();
                    if let Some(parent_apos) = parent_part.find("'s ") {
                        parent_class = Some((
                            parent_part[..parent_apos].trim().to_string(),
                            parent_part[parent_apos + 3..].trim().to_string(),
                        ));
                    } else {
                        parent_class = Some((parent_part.trim().to_string(), String::new()));
                    }
                } else {
                    class_name = rest.trim().to_string();
                }
                found_class_decl = true;
            } else {
                return Err(CompileError::single(
                    line_number,
                    "Expected class declaration (Module's ClassName:)",
                    trimmed,
                ));
            }
            continue;
        }

        // Skip class-level imports that appear before the first method header,
        // at ANY indent. Such an import sits outside every method body and has
        // no resolution path in a class program. If kept it would become the
        // first collected body line, so `parse_class_methods` would derive its
        // method_indent from the import and fail to match any method header,
        // silently compiling the class with zero methods (DEVL-123). Once a
        // method header has been seen, imports are genuinely inside a method
        // body and must be kept so the per-method resolve_imports pass inlines
        // them.
        let is_import = lower.starts_with("import ");
        let is_method_header = trimmed.ends_with(':') && !is_control_flow_header(trimmed);
        if is_import && !seen_method_header {
            continue;
        }
        if is_method_header {
            seen_method_header = true;
        }

        // Collect body lines
        class_body_lines.push(SourceLine {
            line_number,
            indent: leading_spaces(original_line),
            text: trimmed.to_string(),
        });
    }

    if !found_class_decl {
        return Err(CompileError::single(0, "No class declaration found", ""));
    }

    // Group lines into methods
    let methods = parse_class_methods(&class_body_lines)?;

    Ok(ClassProgram {
        module_name,
        class_name,
        parent_class,
        methods,
    })
}

fn is_control_flow_header(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let lower_no_colon = lower.trim_end_matches(':');
    lower_no_colon.starts_with("if ")
        || lower_no_colon.starts_with("while ")
        || lower_no_colon.starts_with("for each ")
        || lower_no_colon.starts_with("until ")
        || lower_no_colon == "try"
        || lower_no_colon.starts_with("try ")
        || lower_no_colon == "otherwise"
        || lower_no_colon.starts_with("otherwise")
        || lower_no_colon.starts_with("when ")
        || lower_no_colon.starts_with("every ")
}

fn parse_class_methods(lines: &[SourceLine]) -> Result<Vec<MethodDef>, CompileError> {
    if lines.is_empty() {
        return Ok(Vec::new());
    }

    // Find the method-level indent (the indent of the first line)
    let method_indent = lines[0].indent;

    // Find method header indices: lines at method_indent that end with ':'
    // and are not control flow headers
    let mut method_starts: Vec<usize> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.indent == method_indent
            && line.text.ends_with(':')
            && !is_control_flow_header(&line.text)
        {
            method_starts.push(i);
        }
    }

    let mut methods = Vec::new();
    for (mi, &start_idx) in method_starts.iter().enumerate() {
        let header = &lines[start_idx];
        let end_idx = if mi + 1 < method_starts.len() {
            method_starts[mi + 1]
        } else {
            lines.len()
        };

        // Parse method header: [privately] <name> [using <params>]:
        let header_text = header.text.trim_end_matches(':').trim();
        let (is_private, name_and_params) =
            if let Some(rest) = strip_prefix_ci(header_text, "privately ") {
                (true, rest.trim())
            } else {
                (false, header_text)
            };

        let (method_name, params) =
            if let Some((name_part, params_part)) = split_once_ci(name_and_params, " using ") {
                let param_list: Vec<String> = params_part
                    .split(" and ")
                    .map(|p| sanitize_name(p.trim()))
                    .collect();
                (name_part.trim().to_string(), param_list)
            } else {
                (name_and_params.to_string(), Vec::new())
            };

        let ruby_name = sanitize_name(&method_name);
        if methods
            .iter()
            .any(|method: &MethodDef| method.ruby_name == ruby_name)
        {
            return Err(CompileError::single(
                header.line_number,
                format!("Duplicate method name: {method_name}"),
                &header.text,
            ));
        }

        // Collect body lines (lines between this header and next method)
        let body_lines: Vec<SourceLine> = lines[start_idx + 1..end_idx]
            .iter()
            .filter(|l| l.indent > method_indent)
            .cloned()
            .collect();

        // Parse body using parse_block
        let mut body_index = 0usize;
        let body_indent = body_lines
            .first()
            .map(|l| l.indent)
            .unwrap_or(method_indent + 2);
        let mut body_statements = parse_block(&body_lines, &mut body_index, body_indent)?;

        // Extract the last "respond with" statement as return_value
        let mut return_value = None;
        let mut i = body_statements.len();
        while i > 0 {
            i -= 1;
            if let StatementKind::RespondWith { value } = &body_statements[i].kind {
                return_value = Some(value.clone());
                body_statements.remove(i);
                // Keep only the last one as return value, remove all respond with statements
            }
        }

        methods.push(MethodDef {
            name: method_name,
            ruby_name,
            params,
            is_private,
            body: body_statements,
            return_value,
            line: header.line_number,
            source_text: header.text.clone(),
        });
    }

    Ok(methods)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceLine {
    line_number: usize,
    indent: usize,
    text: String,
}

fn parse_block(
    lines: &[SourceLine],
    index: &mut usize,
    indent: usize,
) -> Result<Vec<Statement>, CompileError> {
    let mut statements = Vec::new();
    while let Some(line) = lines.get(*index) {
        if line.indent < indent {
            break;
        }
        if line.indent > indent {
            return Err(CompileError::single(
                line.line_number,
                "Unexpected indentation",
                &line.text,
            ));
        }
        statements.push(parse_statement(lines, index)?);
    }
    Ok(statements)
}

fn parse_statement(lines: &[SourceLine], index: &mut usize) -> Result<Statement, CompileError> {
    let line = lines
        .get(*index)
        .ok_or_else(|| CompileError::single(0, "Unexpected end of source", ""))?;
    let line_number = line.line_number;
    let source_text = line.text.as_str();
    let current_indent = line.indent;

    // If block
    if let Some(rest) = strip_prefix_ci(source_text, "If ") {
        let condition_text = rest.trim().trim_end_matches(':').trim();
        *index += 1;
        let Some(child) = lines.get(*index) else {
            return Err(CompileError::single(
                line_number,
                "If requires an indented body",
                source_text,
            ));
        };
        if child.indent <= current_indent {
            return Err(CompileError::single(
                line_number,
                "If requires an indented body",
                source_text,
            ));
        }
        let then_statements = parse_block(lines, index, child.indent)?;

        // Check for Otherwise: at the same indent level
        let else_statements = if let Some(else_line) = lines.get(*index) {
            if else_line.indent == current_indent {
                let else_text = else_line.text.trim();
                let is_otherwise = else_text.eq_ignore_ascii_case("otherwise")
                    || else_text.eq_ignore_ascii_case("otherwise:");
                if is_otherwise {
                    *index += 1;
                    let Some(else_child) = lines.get(*index) else {
                        return Err(CompileError::single(
                            else_line.line_number,
                            "Otherwise requires an indented body",
                            else_text,
                        ));
                    };
                    if else_child.indent <= current_indent {
                        return Err(CompileError::single(
                            else_line.line_number,
                            "Otherwise requires an indented body",
                            else_text,
                        ));
                    }
                    parse_block(lines, index, else_child.indent)?
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        return Ok(statement(
            line_number,
            source_text,
            StatementKind::Branch {
                condition: parse_condition_expression(condition_text),
                then_statements,
                else_statements,
            },
        ));
    }

    // While block
    if let Some(rest) = strip_prefix_ci(source_text, "While ") {
        let condition_text = rest.trim().trim_end_matches(':').trim();
        *index += 1;
        let Some(child) = lines.get(*index) else {
            return Err(CompileError::single(
                line_number,
                "While requires an indented body",
                source_text,
            ));
        };
        if child.indent <= current_indent {
            return Err(CompileError::single(
                line_number,
                "While requires an indented body",
                source_text,
            ));
        }
        let body = parse_block(lines, index, child.indent)?;
        return Ok(statement(
            line_number,
            source_text,
            StatementKind::WhileLoop {
                condition: parse_condition_expression(condition_text),
                body,
            },
        ));
    }

    // Until block
    if let Some(rest) = strip_prefix_ci(source_text, "Until ") {
        let condition_text = rest.trim().trim_end_matches(':').trim();
        *index += 1;
        let Some(child) = lines.get(*index) else {
            return Err(CompileError::single(
                line_number,
                "Until requires an indented body",
                source_text,
            ));
        };
        if child.indent <= current_indent {
            return Err(CompileError::single(
                line_number,
                "Until requires an indented body",
                source_text,
            ));
        }
        let body = parse_block(lines, index, child.indent)?;
        return Ok(statement(
            line_number,
            source_text,
            StatementKind::UntilLoop {
                condition: parse_condition_expression(condition_text),
                body,
            },
        ));
    }

    // For each block
    if let Some(rest) = strip_prefix_ci(source_text, "For each ") {
        let rest = rest.trim().trim_end_matches(':').trim();
        if let Some((item, collection)) = split_once_ci(rest, " in ") {
            *index += 1;
            let Some(child) = lines.get(*index) else {
                return Err(CompileError::single(
                    line_number,
                    "For each requires an indented body",
                    source_text,
                ));
            };
            if child.indent <= current_indent {
                return Err(CompileError::single(
                    line_number,
                    "For each requires an indented body",
                    source_text,
                ));
            }
            let body = parse_block(lines, index, child.indent)?;
            return Ok(statement(
                line_number,
                source_text,
                StatementKind::ForEach {
                    item: sanitize_name(item),
                    collection: parse_expression(collection.trim()),
                    body,
                },
            ));
        }
    }

    // Try block with Otherwise recovery.
    let trimmed_lower = source_text.to_ascii_lowercase();
    if trimmed_lower == "try:" || trimmed_lower == "try" {
        *index += 1;
        let Some(child) = lines.get(*index) else {
            return Err(CompileError::single(
                line_number,
                "Try requires an indented body",
                source_text,
            ));
        };
        if child.indent <= current_indent {
            return Err(CompileError::single(
                line_number,
                "Try requires an indented body",
                source_text,
            ));
        }
        let body = parse_block(lines, index, child.indent)?;
        let recovery = if let Some(else_line) = lines.get(*index) {
            if else_line.indent == current_indent {
                let else_text = else_line.text.trim();
                let is_otherwise = else_text.eq_ignore_ascii_case("otherwise")
                    || else_text.eq_ignore_ascii_case("otherwise:");
                if is_otherwise {
                    *index += 1;
                    let Some(else_child) = lines.get(*index) else {
                        return Err(CompileError::single(
                            else_line.line_number,
                            "Otherwise requires an indented body",
                            else_text,
                        ));
                    };
                    if else_child.indent <= current_indent {
                        return Err(CompileError::single(
                            else_line.line_number,
                            "Otherwise requires an indented body",
                            else_text,
                        ));
                    }
                    parse_block(lines, index, else_child.indent)?
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        return Ok(statement(
            line_number,
            source_text,
            StatementKind::TryRecover { body, recovery },
        ));
    }

    // Trigger patterns (block statements with colon ending)
    if let Some(trigger) = parse_trigger_header(source_text) {
        *index += 1;
        // Consume the body (but store it as trigger metadata, not compiled)
        if let Some(child) = lines.get(*index) {
            if child.indent > current_indent {
                let _body = parse_block(lines, index, child.indent)?;
            }
        }
        return Ok(statement(line_number, source_text, trigger));
    }

    let parsed = parse_flat_statement(line_number, source_text)?;
    *index += 1;
    Ok(parsed)
}

fn parse_trigger_header(line: &str) -> Option<StatementKind> {
    let trimmed = line.trim().trim_end_matches(':');
    let lower = trimmed.to_ascii_lowercase();

    // "every day at <time>"
    if let Some(rest) = strip_prefix_ci(trimmed, "Every day at ") {
        return Some(StatementKind::Trigger {
            trigger_type: "schedule".to_string(),
            params: vec![
                ("interval".to_string(), "daily".to_string()),
                ("time".to_string(), rest.trim().to_string()),
            ],
        });
    }

    // "every <day> at <time>"
    if lower.starts_with("every ") {
        if let Some((day_part, time_part)) = split_once_ci(trimmed, " at ") {
            let day = strip_prefix_ci(day_part, "Every ")
                .unwrap_or(day_part)
                .trim();
            return Some(StatementKind::Trigger {
                trigger_type: "schedule".to_string(),
                params: vec![
                    ("interval".to_string(), "weekly".to_string()),
                    ("day".to_string(), day.to_string()),
                    ("time".to_string(), time_part.trim().to_string()),
                ],
            });
        }
        // "every <N> <unit>"
        let rest = strip_prefix_ci(trimmed, "Every ").unwrap_or(trimmed);
        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.len() == 2 {
            return Some(StatementKind::Trigger {
                trigger_type: "schedule".to_string(),
                params: vec![
                    ("count".to_string(), parts[0].to_string()),
                    ("unit".to_string(), parts[1].to_string()),
                ],
            });
        }
    }

    // "when <event>"
    if let Some(rest) = strip_prefix_ci(trimmed, "When ") {
        return Some(StatementKind::Trigger {
            trigger_type: "event".to_string(),
            params: vec![("event".to_string(), rest.trim().to_string())],
        });
    }

    None
}

fn parse_flat_statement(line_number: usize, line: &str) -> Result<Statement, CompileError> {
    let lower = line.trim().to_ascii_lowercase();

    // break / continue (exact match)
    if lower == "break" {
        return Ok(statement(line_number, line, StatementKind::Break));
    }
    if lower == "continue" {
        return Ok(statement(line_number, line, StatementKind::Continue));
    }

    // Bracket characters are not devlish grammar. Left unchecked they used to
    // slip through as unknown identifiers and evaluate to null, silently
    // producing wrong data. Fail loudly instead (DEVL-127). Brackets inside a
    // quoted string are legitimate text and are left alone. Import statements
    // may carry brackets in their (unquoted) file paths, so skip the guard for
    // them (DEVL-127).
    if !lower.starts_with("import ") {
        if let Some(bracket) = first_bracket_outside_quotes(line) {
            return Err(CompileError::single(
                line_number,
                format!(
                    "Unexpected '{bracket}' in expression. Devlish has no bracket syntax; \
                     write lists as `list of 1, 2, 3`."
                ),
                line,
            ));
        }
    }

    // checkpoint "prompt" [saving context as key]
    if let Some(rest) = strip_prefix_ci(line, "Checkpoint ") {
        let (prompt_text, context_key) = if let Some((prompt_part, key_part)) =
            split_once_ci(rest.trim(), " saving context as ")
        {
            (prompt_part.trim(), Some(sanitize_name(key_part.trim())))
        } else {
            (rest.trim(), None)
        };
        return Ok(statement(
            line_number,
            line,
            StatementKind::Checkpoint {
                prompt: parse_expression(prompt_text),
                context_key,
            },
        ));
    }

    // fail with <message>
    if let Some(rest) = strip_prefix_ci(line, "Fail with ") {
        return Ok(statement(
            line_number,
            line,
            StatementKind::Fail {
                message: parse_expression(rest.trim()),
            },
        ));
    }

    // require <condition> otherwise fail with <message>
    if let Some(rest) = strip_prefix_ci(line, "Require ") {
        if let Some((cond_text, msg_text)) = split_once_ci(rest.trim(), " otherwise fail with ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::Require {
                    condition: parse_condition_expression(cond_text.trim()),
                    message: Some(parse_expression(msg_text.trim())),
                },
            ));
        }
        // require <X> in document
        if let Some((target, _)) = split_once_ci(rest.trim(), " in document") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::DocumentRequirement {
                    verb: "require".to_string(),
                    target: target.trim().to_string(),
                },
            ));
        }
        // plain require <condition>
        return Ok(statement(
            line_number,
            line,
            StatementKind::Require {
                condition: parse_condition_expression(rest.trim()),
                message: None,
            },
        ));
    }

    // Ask multiline "prompt" as <name>
    if let Some(rest) = strip_prefix_ci(line, "Ask multiline ") {
        let (prompt, rest) = quoted_prefix(rest)
            .ok_or_else(|| CompileError::single(line_number, "Expected quoted prompt", line))?;
        let target = strip_prefix_ci(rest.trim(), "as ").ok_or_else(|| {
            CompileError::single(line_number, "Expected `as <name>` after prompt", line)
        })?;
        return Ok(statement(
            line_number,
            line,
            StatementKind::Input {
                target: sanitize_name(target),
                prompt: Expression::Literal(Value::String(prompt)),
                source: InputSource::MultilinePrompt,
            },
        ));
    }

    // Ask (existing)
    if let Some(rest) = strip_prefix_ci(line, "Ask ") {
        let (prompt, rest) = quoted_prefix(rest)
            .ok_or_else(|| CompileError::single(line_number, "Expected quoted prompt", line))?;
        let target = strip_prefix_ci(rest.trim(), "as ").ok_or_else(|| {
            CompileError::single(line_number, "Expected `as <name>` after prompt", line)
        })?;
        return Ok(statement(
            line_number,
            line,
            StatementKind::Input {
                target: sanitize_name(target),
                prompt: Expression::Literal(Value::String(prompt)),
                source: InputSource::Prompt,
            },
        ));
    }

    // Print / Show (existing)
    if let Some(rest) = strip_prefix_ci(line, "Print ") {
        return Ok(statement(
            line_number,
            line,
            StatementKind::Output {
                value: parse_expression(rest.trim()),
            },
        ));
    }
    if let Some(rest) = strip_prefix_ci(line, "Show ") {
        return Ok(statement(
            line_number,
            line,
            StatementKind::Output {
                value: parse_expression(rest.trim()),
            },
        ));
    }

    // set <target> to <value> [if <condition>]
    if let Some(rest) = strip_prefix_ci(line, "Set ") {
        if let Some((target_text, value_text)) = split_set_target_value(rest.trim()) {
            let (value_str, condition) = split_trailing_if(value_text.trim());
            return Ok(statement(
                line_number,
                line,
                StatementKind::SetField {
                    target: parse_set_target(target_text.trim()),
                    value: parse_expression(value_str),
                    condition: condition.map(|c| parse_condition_expression(c)),
                },
            ));
        }
    }

    // append <value> to <target>
    if let Some(rest) = strip_prefix_ci(line, "Append ") {
        if let Some((value_text, path_text)) = split_once_ci(rest.trim(), " to file ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::FileWrite {
                    value: parse_expression(value_text.trim()),
                    path: parse_expression(path_text.trim()),
                    mode: FileWriteMode::Append,
                },
            ));
        }
        if let Some((value_text, target_text)) = split_once_ci(rest.trim(), " to ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::Append {
                    value: parse_expression(value_text.trim()),
                    target: sanitize_name(target_text.trim()),
                },
            ));
        }
    }

    // pop from <source> and save as <target>
    if let Some(rest) = strip_prefix_ci(line, "Pop from ") {
        if let Some((source_text, target_text)) = split_once_ci(rest.trim(), " and save as ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::Pop {
                    source: sanitize_name(source_text.trim()),
                    store_as: sanitize_name(target_text.trim()),
                },
            ));
        }
    }

    // alias/nickname/symbol/handle <source> as <target>
    for keyword in &["Alias ", "Nickname ", "Symbol ", "Handle "] {
        if let Some(rest) = strip_prefix_ci(line, keyword) {
            if let Some((source_text, target_text)) = split_once_ci(rest.trim(), " as ") {
                return Ok(statement(
                    line_number,
                    line,
                    StatementKind::Bind {
                        source_name: sanitize_name(source_text.trim()),
                        target_name: sanitize_name(target_text.trim()),
                        kind: keyword.trim().to_ascii_lowercase(),
                    },
                ));
            }
        }
    }

    // read multiline input/stdin as <name>
    if let Some(rest) = strip_prefix_ci(line, "Read the multiline input as ") {
        return Ok(statement(
            line_number,
            line,
            StatementKind::Input {
                target: sanitize_name(rest.trim()),
                prompt: Expression::Literal(Value::String(String::new())),
                source: InputSource::MultilineStdin,
            },
        ));
    }
    if let Some(rest) = strip_prefix_ci(line, "Read the multiline stdin as ") {
        return Ok(statement(
            line_number,
            line,
            StatementKind::Input {
                target: sanitize_name(rest.trim()),
                prompt: Expression::Literal(Value::String(String::new())),
                source: InputSource::MultilineStdin,
            },
        ));
    }
    if let Some(rest) = strip_prefix_ci(line, "Read multiline input as ") {
        return Ok(statement(
            line_number,
            line,
            StatementKind::Input {
                target: sanitize_name(rest.trim()),
                prompt: Expression::Literal(Value::String(String::new())),
                source: InputSource::MultilineStdin,
            },
        ));
    }
    if let Some(rest) = strip_prefix_ci(line, "Read multiline stdin as ") {
        return Ok(statement(
            line_number,
            line,
            StatementKind::Input {
                target: sanitize_name(rest.trim()),
                prompt: Expression::Literal(Value::String(String::new())),
                source: InputSource::MultilineStdin,
            },
        ));
    }

    // read the input/stdin as <name> / read input as <name>
    if let Some(rest) = strip_prefix_ci(line, "Read the input as ") {
        return Ok(statement(
            line_number,
            line,
            StatementKind::ReadStdin {
                target: sanitize_name(rest.trim()),
            },
        ));
    }
    if let Some(rest) = strip_prefix_ci(line, "Read the stdin as ") {
        return Ok(statement(
            line_number,
            line,
            StatementKind::ReadStdin {
                target: sanitize_name(rest.trim()),
            },
        ));
    }
    if let Some(rest) = strip_prefix_ci(line, "Read input as ") {
        return Ok(statement(
            line_number,
            line,
            StatementKind::ReadStdin {
                target: sanitize_name(rest.trim()),
            },
        ));
    }
    if let Some(rest) = strip_prefix_ci(line, "Read stdin as ") {
        return Ok(statement(
            line_number,
            line,
            StatementKind::ReadStdin {
                target: sanitize_name(rest.trim()),
            },
        ));
    }

    // Export (existing - must come before Load since both are common)
    if let Some(rest) = strip_prefix_ci(line, "Export ") {
        let lower_rest = rest.trim_start().to_ascii_lowercase();
        if lower_rest.starts_with("assertions to ") {
            // Handled by the assertion-report export syntax below.
        } else if let Some((value, path)) = split_once_ci(rest.trim(), " to ") {
            let (path_text, mode) =
                if let Some(path_without_format) = strip_suffix_ci(path.trim(), " as CSV") {
                    (path_without_format.trim(), FileWriteMode::Csv)
                } else {
                    (path.trim(), FileWriteMode::Export)
                };
            return Ok(statement(
                line_number,
                line,
                StatementKind::FileWrite {
                    value: parse_expression(value.trim()),
                    path: parse_expression(path_text),
                    mode,
                },
            ));
        }
    }

    if let Some(rest) = strip_prefix_ci(line, "Write ") {
        if let Some((value, path)) = split_once_ci(rest.trim(), " to ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::FileWrite {
                    value: parse_expression(value.trim()),
                    path: parse_expression(path.trim()),
                    mode: FileWriteMode::Write,
                },
            ));
        }
    }

    if let Some(rest) = strip_prefix_ci(line, "Overwrite ") {
        if let Some((value, path)) = split_once_ci(rest.trim(), " to file ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::FileWrite {
                    value: parse_expression(value.trim()),
                    path: parse_expression(path.trim()),
                    mode: FileWriteMode::Overwrite,
                },
            ));
        }
        if let Some((value, path)) = split_once_ci(rest.trim(), " to ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::FileWrite {
                    value: parse_expression(value.trim()),
                    path: parse_expression(path.trim()),
                    mode: FileWriteMode::Overwrite,
                },
            ));
        }
    }

    // Load patterns
    if lower.starts_with("load ") {
        return parse_load_statement(line_number, line);
    }

    // read structured files
    if let Some(rest) = strip_prefix_ci(line, "Read JSON from ") {
        if let Some((path_text, target_text)) = split_once_ci(rest.trim(), " as ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::FileRead {
                    path: parse_expression(path_text.trim()),
                    target: sanitize_name(target_text.trim()),
                    format: FileReadFormat::Json,
                },
            ));
        }
    }
    if let Some(rest) = strip_prefix_ci(line, "Read CSV from ") {
        if let Some((path_text, target_text)) = split_once_ci(rest.trim(), " as ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::FileRead {
                    path: parse_expression(path_text.trim()),
                    target: sanitize_name(target_text.trim()),
                    format: FileReadFormat::Csv,
                },
            ));
        }
    }

    if let Some(rest) = strip_prefix_ci(line, "Read text from ") {
        if let Some((path_text, target_text)) = split_once_ci(rest.trim(), " as ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::FileRead {
                    path: parse_expression(path_text.trim()),
                    target: sanitize_name(target_text.trim()),
                    format: FileReadFormat::Text,
                },
            ));
        }
    }

    // Find files matching <pattern> in <directory> as <dest>
    // (must come before generic "Find" to avoid being caught by Extract)
    if let Some(rest) = strip_prefix_ci(line, "Find files matching ") {
        if let Some((pattern_and_dir, dest_text)) = split_once_ci(rest.trim(), " as ") {
            if let Some((pattern_text, dir_text)) = split_once_ci(pattern_and_dir.trim(), " in ") {
                return Ok(statement(
                    line_number,
                    line,
                    StatementKind::FileGlob {
                        pattern: parse_expression(pattern_text.trim()),
                        directory: parse_expression(dir_text.trim()),
                        dest: sanitize_name(dest_text.trim()),
                    },
                ));
            }
        }
    }

    // find/extract <target> [and save as <name>]
    if let Some(rest) = strip_prefix_ci(line, "Find ") {
        let rest = rest.trim();
        if let Some((target_text, store_text)) = split_once_ci(rest, " and save as ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::Extract {
                    target: target_text.trim().to_string(),
                    store_as: sanitize_name(store_text.trim()),
                },
            ));
        }
        return Ok(statement(
            line_number,
            line,
            StatementKind::Extract {
                target: rest.to_string(),
                store_as: sanitize_name(rest),
            },
        ));
    }
    if let Some(rest) = strip_prefix_ci(line, "Extract ") {
        let rest = rest.trim();
        if let Some((target_text, store_text)) = split_once_ci(rest, " and save as ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::Extract {
                    target: target_text.trim().to_string(),
                    store_as: sanitize_name(store_text.trim()),
                },
            ));
        }
        return Ok(statement(
            line_number,
            line,
            StatementKind::Extract {
                target: rest.to_string(),
                store_as: sanitize_name(rest),
            },
        ));
    }

    // Read XLSX cell (existing)
    if let Some(rest) = strip_prefix_ci(line, "Read XLSX cell ") {
        let (reference, rest) = quoted_prefix(rest).ok_or_else(|| {
            CompileError::single(line_number, "Expected quoted XLSX cell reference", line)
        })?;
        let target = strip_prefix_ci(rest.trim(), "as ").ok_or_else(|| {
            CompileError::single(
                line_number,
                "Expected `as <name>` after XLSX cell reference",
                line,
            )
        })?;
        let (sheet, cell) = reference.split_once('!').ok_or_else(|| {
            CompileError::single(line_number, "XLSX cell reference must use Sheet!Cell", line)
        })?;
        return Ok(statement(
            line_number,
            line,
            StatementKind::ReadXlsxCell {
                sheet: sheet.trim().to_string(),
                cell: cell.trim().to_string(),
                target: sanitize_name(target),
            },
        ));
    }

    // Read XLSX rows from "file.xlsx" [sheet "Sheet1"] as data
    if let Some(rest) = strip_prefix_ci(line, "Read XLSX rows from ") {
        let (path_text, after_path) = quoted_prefix(rest).ok_or_else(|| {
            CompileError::single(line_number, "Expected quoted XLSX file path", line)
        })?;
        let (sheet, dest_text) =
            if let Some(after_sheet) = strip_prefix_ci(after_path.trim(), "sheet ") {
                let (sheet_name, rest2) = quoted_prefix(after_sheet).ok_or_else(|| {
                    CompileError::single(line_number, "Expected quoted sheet name", line)
                })?;
                (Some(sheet_name.to_string()), rest2.to_string())
            } else {
                (None, after_path.to_string())
            };
        let target = strip_prefix_ci(dest_text.trim(), "as ").ok_or_else(|| {
            CompileError::single(
                line_number,
                "Expected `as <name>` after XLSX rows path",
                line,
            )
        })?;
        return Ok(statement(
            line_number,
            line,
            StatementKind::XlsxReadRows {
                // `path_text` is the already-unquoted string content from
                // `quoted_prefix`, so it is a string literal, not an expression.
                // Re-parsing it as an expression turned a path like
                // `/tmp/foo.xlsx` into a phantom variable that evaluated to null
                // at runtime (DEVL-127).
                path: Expression::Literal(Value::String(path_text.to_string())),
                sheet,
                dest: sanitize_name(target),
            },
        ));
    }

    // Download the url at "https://..." to "path"
    if let Some(rest) = strip_prefix_ci(line, "Download the url at ") {
        if let Some((url_text, path_text)) = split_once_ci(rest.trim(), " to ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::HttpDownload {
                    url: parse_expression(url_text.trim()),
                    path: parse_expression(path_text.trim()),
                },
            ));
        }
    }

    // Read PDF text (existing)
    if let Some(rest) = strip_prefix_ci(line, "Read PDF text ") {
        let (path, rest) = quoted_prefix(rest)
            .ok_or_else(|| CompileError::single(line_number, "Expected quoted PDF path", line))?;
        let target = strip_prefix_ci(rest.trim(), "as ").ok_or_else(|| {
            CompileError::single(line_number, "Expected `as <name>` after PDF path", line)
        })?;
        return Ok(statement(
            line_number,
            line,
            StatementKind::ReadPdfText {
                path,
                target: sanitize_name(target),
            },
        ));
    }

    // Read DOCX text (existing)
    if let Some(rest) = strip_prefix_ci(line, "Read DOCX text ") {
        let (path, rest) = quoted_prefix(rest)
            .ok_or_else(|| CompileError::single(line_number, "Expected quoted DOCX path", line))?;
        let target = strip_prefix_ci(rest.trim(), "as ").ok_or_else(|| {
            CompileError::single(line_number, "Expected `as <name>` after DOCX path", line)
        })?;
        return Ok(statement(
            line_number,
            line,
            StatementKind::ReadDocxText {
                path,
                target: sanitize_name(target),
            },
        ));
    }

    // <var> must/should be at least/at most/equal/contain/match/... <value>
    // Also: verify <var> is at least <value>
    if let Some(rest) = strip_prefix_ci(line, "Verify ") {
        if let Some((target_text, value_text)) = split_once_ci(rest.trim(), " is at least ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::Validate {
                    target: parse_expression(target_text.trim()),
                    rule: ValidateRule::Minimum,
                    value: Some(parse_expression(value_text.trim())),
                },
            ));
        }
        if let Some((target_text, value_text)) = split_once_ci(rest.trim(), " is at most ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::Validate {
                    target: parse_expression(target_text.trim()),
                    rule: ValidateRule::Maximum,
                    value: Some(parse_expression(value_text.trim())),
                },
            ));
        }
    }
    if let Some(validation) = parse_validation_statement(line) {
        return Ok(statement(line_number, line, validation));
    }

    // document must/should contain/have/include <X>
    if let Some(target) = parse_document_requirement(line) {
        return Ok(statement(line_number, line, target));
    }

    // check for <X>
    if let Some(rest) = strip_prefix_ci(line, "Check for ") {
        return Ok(statement(
            line_number,
            line,
            StatementKind::DocumentRequirement {
                verb: "check".to_string(),
                target: rest.trim().to_string(),
            },
        ));
    }

    // route <source> to <destination>
    if let Some(rest) = strip_prefix_ci(line, "Route ") {
        if let Some((source_text, dest_text)) = split_once_ci(rest.trim(), " to ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::Route {
                    source: parse_expression(source_text.trim()),
                    destination: parse_expression(dest_text.trim()),
                },
            ));
        }
    }

    // Filesystem keywords
    // Copy file from <source> to <destination>
    if let Some(rest) = strip_prefix_ci(line, "Copy file from ") {
        if let Some((source_text, dest_text)) = split_once_ci(rest.trim(), " to ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::FileCopy {
                    source: parse_expression(source_text.trim()),
                    destination: parse_expression(dest_text.trim()),
                },
            ));
        }
    }
    // Move file from <source> to <destination>
    if let Some(rest) = strip_prefix_ci(line, "Move file from ") {
        if let Some((source_text, dest_text)) = split_once_ci(rest.trim(), " to ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::FileMove {
                    source: parse_expression(source_text.trim()),
                    destination: parse_expression(dest_text.trim()),
                },
            ));
        }
    }
    // Create directory <path>
    if let Some(rest) = strip_prefix_ci(line, "Create directory ") {
        return Ok(statement(
            line_number,
            line,
            StatementKind::FileMkdir {
                path: parse_expression(rest.trim()),
            },
        ));
    }
    // Delete file <path>
    if let Some(rest) = strip_prefix_ci(line, "Delete file ") {
        return Ok(statement(
            line_number,
            line,
            StatementKind::FileDelete {
                path: parse_expression(rest.trim()),
            },
        ));
    }
    // Check if <path> exists as <dest>
    if let Some(rest) = strip_prefix_ci(line, "Check if ") {
        if let Some((path_text, dest_text)) = split_once_ci(rest.trim(), " exists as ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::FileExists {
                    path: parse_expression(path_text.trim()),
                    dest: sanitize_name(dest_text.trim()),
                },
            ));
        }
    }
    // Get file info for <path> as <dest>
    if let Some(rest) = strip_prefix_ci(line, "Get file info for ") {
        if let Some((path_text, dest_text)) = split_once_ci(rest.trim(), " as ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::FileStat {
                    path: parse_expression(path_text.trim()),
                    dest: sanitize_name(dest_text.trim()),
                },
            ));
        }
    }
    // List files in <path> as <dest>
    if let Some(rest) = strip_prefix_ci(line, "List files in ") {
        if let Some((path_text, dest_text)) = split_once_ci(rest.trim(), " as ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::FileList {
                    path: parse_expression(path_text.trim()),
                    dest: sanitize_name(dest_text.trim()),
                },
            ));
        }
    }
    // HTTP verb keywords
    if let Some(rest) = strip_prefix_ci(line, "Get the url at ") {
        if let Some((url_text, dest_text)) = split_once_ci(rest.trim(), " as ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::HttpRequest {
                    method: "GET".to_string(),
                    url: parse_expression(url_text.trim()),
                    body: None,
                    dest: sanitize_name(dest_text.trim()),
                },
            ));
        }
    }
    if let Some(rest) = strip_prefix_ci(line, "Post to ") {
        if let Some((url_and_body, dest_text)) = split_once_ci(rest.trim(), " as ") {
            if let Some((url_text, body_text)) = split_once_ci(url_and_body.trim(), " with ") {
                return Ok(statement(
                    line_number,
                    line,
                    StatementKind::HttpRequest {
                        method: "POST".to_string(),
                        url: parse_expression(url_text.trim()),
                        body: Some(parse_expression(body_text.trim())),
                        dest: sanitize_name(dest_text.trim()),
                    },
                ));
            }
        }
    }
    if let Some(rest) = strip_prefix_ci(line, "Put to ") {
        if let Some((url_and_body, dest_text)) = split_once_ci(rest.trim(), " as ") {
            if let Some((url_text, body_text)) = split_once_ci(url_and_body.trim(), " with ") {
                return Ok(statement(
                    line_number,
                    line,
                    StatementKind::HttpRequest {
                        method: "PUT".to_string(),
                        url: parse_expression(url_text.trim()),
                        body: Some(parse_expression(body_text.trim())),
                        dest: sanitize_name(dest_text.trim()),
                    },
                ));
            }
        }
    }
    if let Some(rest) = strip_prefix_ci(line, "Patch to ") {
        if let Some((url_and_body, dest_text)) = split_once_ci(rest.trim(), " as ") {
            if let Some((url_text, body_text)) = split_once_ci(url_and_body.trim(), " with ") {
                return Ok(statement(
                    line_number,
                    line,
                    StatementKind::HttpRequest {
                        method: "PATCH".to_string(),
                        url: parse_expression(url_text.trim()),
                        body: Some(parse_expression(body_text.trim())),
                        dest: sanitize_name(dest_text.trim()),
                    },
                ));
            }
        }
    }
    if let Some(rest) = strip_prefix_ci(line, "Delete the url at ") {
        if let Some((url_text, dest_text)) = split_once_ci(rest.trim(), " as ") {
            return Ok(statement(
                line_number,
                line,
                StatementKind::HttpRequest {
                    method: "DELETE".to_string(),
                    url: parse_expression(url_text.trim()),
                    body: None,
                    dest: sanitize_name(dest_text.trim()),
                },
            ));
        }
    }

    // Service call patterns
    if let Some(sc) = parse_service_call(line) {
        return Ok(statement(line_number, line, sc));
    }

    // Use the <name> module / Use <a> and <b> from the <name> module
    if let Some(rest) = strip_prefix_ci(line, "Use ") {
        if let Some(parsed) = parse_use_module(rest.trim().trim_end_matches('.'), line_number, line)?
        {
            return Ok(parsed);
        }
    }

    // import <path>
    if let Some(rest) = strip_prefix_ci(line, "Import ") {
        let path = rest.trim().trim_matches('"');
        return Ok(statement(
            line_number,
            line,
            StatementKind::Import {
                path: path.to_string(),
            },
        ));
    }

    // <name> equals <expr> [if <condition>]
    if let Some((target, value_part)) = split_once_ci(line, " equals ") {
        let target = target.trim();
        if starts_with_lowercase_name(target) {
            let (value_str, condition) = split_trailing_if(value_part.trim());
            let value = parse_expression(value_str);
            if let Some(name) = find_reserved_word_variable(&value) {
                return Err(CompileError::single(
                    line_number,
                    &format!("Invalid expression '{value_str}': '{name}' uses the reserved word 'equals'"),
                    line,
                ));
            }
            if let Some(cond_text) = condition {
                return Ok(statement(
                    line_number,
                    line,
                    StatementKind::ConditionalAssignment {
                        target: sanitize_name(target),
                        value,
                        condition: parse_condition_expression(cond_text),
                    },
                ));
            }
            return Ok(statement(
                line_number,
                line,
                StatementKind::Assignment {
                    target: sanitize_name(target),
                    value,
                },
            ));
        }
    }

    // Expect (existing)
    if let Some(rest) = strip_prefix_ci(line, "Expect ") {
        return parse_assertion(line_number, line, rest);
    }

    // Export assertions to (existing)
    if let Some(rest) = strip_prefix_ci(line, "Export assertions to ") {
        return Ok(statement(
            line_number,
            line,
            StatementKind::ExportAssertions {
                path: parse_expression(rest.trim()),
            },
        ));
    }

    // email/notify <recipient>
    if let Some(rest) = strip_prefix_ci(line, "Email ") {
        return Ok(statement(
            line_number,
            line,
            StatementKind::ServiceCall {
                service: "email".to_string(),
                action: "send".to_string(),
                arguments: vec![("recipient".to_string(), parse_expression(rest.trim()))],
            },
        ));
    }
    if let Some(rest) = strip_prefix_ci(line, "Notify ") {
        return Ok(statement(
            line_number,
            line,
            StatementKind::ServiceCall {
                service: "notification".to_string(),
                action: "send".to_string(),
                arguments: vec![("recipient".to_string(), parse_expression(rest.trim()))],
            },
        ));
    }

    // respond with <value>
    if let Some(rest) = strip_prefix_ci(line, "Respond with ") {
        return Ok(statement(
            line_number,
            line,
            StatementKind::RespondWith {
                value: parse_expression(rest.trim()),
            },
        ));
    }

    // <term> is <definition> (catch-all for "X is Y" definitions)
    if let Some((name, definition)) = split_once_ci(line, " is ") {
        let name = name.trim();
        let definition = definition.trim();
        if !name.is_empty() && !definition.is_empty() {
            return Ok(statement(
                line_number,
                line,
                StatementKind::Definition {
                    name: name.to_string(),
                    definition: definition.to_string(),
                },
            ));
        }
    }

    Err(CompileError::single(
        line_number,
        "Unsupported native compiler statement",
        line,
    ))
}

fn parse_validation_statement(line: &str) -> Option<StatementKind> {
    if let Some((target_text, rest)) = split_once_ci(line, " must be at least ") {
        return Some(validation(target_text, ValidateRule::Minimum, Some(rest)));
    }
    if let Some((target_text, rest)) = split_once_ci(line, " must be at most ") {
        return Some(validation(target_text, ValidateRule::Maximum, Some(rest)));
    }
    if let Some((target_text, rest)) = split_once_ci(line, " should be at least ") {
        return Some(validation(target_text, ValidateRule::Minimum, Some(rest)));
    }
    if let Some((target_text, rest)) = split_once_ci(line, " should be at most ") {
        return Some(validation(target_text, ValidateRule::Maximum, Some(rest)));
    }
    for modal in [" must ", " should "] {
        if let Some((target_text, rest)) = split_once_ci(line, modal) {
            let target_trimmed = target_text.trim();
            if target_trimmed.eq_ignore_ascii_case("document")
                || target_trimmed.eq_ignore_ascii_case("documents")
            {
                return None;
            }
            let rest = rest.trim();
            if let Some(value) = strip_prefix_ci(rest, "equal ") {
                return Some(validation(target_text, ValidateRule::Equals, Some(value)));
            }
            if let Some(value) = strip_prefix_ci(rest, "equals ") {
                return Some(validation(target_text, ValidateRule::Equals, Some(value)));
            }
            if let Some(value) = strip_prefix_ci(rest, "contain ") {
                return Some(validation(target_text, ValidateRule::Contains, Some(value)));
            }
            if let Some(value) = strip_prefix_ci(rest, "contains ") {
                return Some(validation(target_text, ValidateRule::Contains, Some(value)));
            }
            if let Some(value) = strip_prefix_ci(rest, "match ") {
                return Some(validation(target_text, ValidateRule::Matches, Some(value)));
            }
            if let Some(value) = strip_prefix_ci(rest, "matches ") {
                return Some(validation(target_text, ValidateRule::Matches, Some(value)));
            }
            if rest.eq_ignore_ascii_case("be present")
                || rest.eq_ignore_ascii_case("be provided")
                || rest.eq_ignore_ascii_case("exist")
            {
                return Some(validation(target_text, ValidateRule::Present, None));
            }
            if rest.eq_ignore_ascii_case("be missing")
                || rest.eq_ignore_ascii_case("be blank")
                || rest.eq_ignore_ascii_case("not be present")
            {
                return Some(validation(target_text, ValidateRule::Missing, None));
            }
            if let Some(value) = strip_prefix_ci(rest, "be one of ") {
                return Some(validation(target_text, ValidateRule::OneOf, Some(value)));
            }
            if let Some(value) = strip_prefix_ci(rest, "be in ") {
                return Some(validation(target_text, ValidateRule::OneOf, Some(value)));
            }
        }
    }
    None
}

fn validation(target: &str, rule: ValidateRule, value: Option<&str>) -> StatementKind {
    StatementKind::Validate {
        target: parse_expression(target.trim()),
        rule,
        value: value.map(|text| parse_expression(text.trim())),
    }
}

fn parse_load_statement(line_number: usize, line: &str) -> Result<Statement, CompileError> {
    let rest = strip_prefix_ci(line, "Load ").unwrap_or("");
    let rest = rest.trim();
    let lower = rest.to_ascii_lowercase();

    // "load document" (no args)
    if lower == "document" || lower == "document:" {
        return Ok(statement(
            line_number,
            line,
            StatementKind::Load {
                path: None,
                alias: None,
            },
        ));
    }

    // "load <file> as <name>"
    if let Some((path_text, alias_text)) = split_once_ci(rest, " as ") {
        let path = unquote(path_text.trim());
        return Ok(statement(
            line_number,
            line,
            StatementKind::Load {
                path: Some(path),
                alias: Some(alias_text.trim().to_string()),
            },
        ));
    }

    // "load <name> from <file>"
    if let Some((name_text, path_text)) = split_once_ci(rest, " from ") {
        let path = unquote(path_text.trim());
        return Ok(statement(
            line_number,
            line,
            StatementKind::Load {
                path: Some(path),
                alias: Some(name_text.trim().to_string()),
            },
        ));
    }

    // "load <file>"
    let path = unquote(rest);
    Ok(statement(
        line_number,
        line,
        StatementKind::Load {
            path: Some(path),
            alias: None,
        },
    ))
}

fn parse_document_requirement(line: &str) -> Option<StatementKind> {
    for verb_word in &["must", "should"] {
        for action_word in &["contain", "have", "include"] {
            let pattern = format!("Document {} {} ", verb_word, action_word);
            if let Some(rest) = strip_prefix_ci(line, &pattern) {
                return Some(StatementKind::DocumentRequirement {
                    verb: format!("{} {}", verb_word, action_word),
                    target: rest.trim().to_string(),
                });
            }
        }
    }
    None
}

fn parse_service_call(line: &str) -> Option<StatementKind> {
    // search the <Service> for <X> [at <Y>]
    if let Some(rest) = strip_prefix_ci(line, "Search the ") {
        if let Some((service, query)) = split_once_ci(rest.trim(), " for ") {
            let mut arguments = Vec::new();
            let (query_text, at_text) = if let Some((q, a)) = split_once_ci(query.trim(), " at ") {
                arguments.push(("location".to_string(), parse_expression(a.trim())));
                (q, Some(a))
            } else {
                (query.trim(), None)
            };
            let _ = at_text;
            arguments.insert(
                0,
                ("query".to_string(), parse_expression(query_text.trim())),
            );
            return Some(StatementKind::ServiceCall {
                service: sanitize_name(service.trim()),
                action: "search".to_string(),
                arguments,
            });
        }
    }

    // create <Service> entry with <fields>
    if let Some(rest) = strip_prefix_ci(line, "Create ") {
        if let Some((service, fields_text)) = split_once_ci(rest.trim(), " entry with ") {
            let arguments = parse_field_list(fields_text.trim());
            return Some(StatementKind::ServiceCall {
                service: sanitize_name(service.trim()),
                action: "create".to_string(),
                arguments,
            });
        }
    }

    // send email via <Service> to <recipient> [with template <X>]
    if let Some(rest) = strip_prefix_ci(line, "Send email via ") {
        if let Some((service, recipient_text)) = split_once_ci(rest.trim(), " to ") {
            let mut arguments = Vec::new();
            let (recipient, template) =
                if let Some((r, t)) = split_once_ci(recipient_text.trim(), " with template ") {
                    arguments.push(("template".to_string(), parse_expression(t.trim())));
                    (r, Some(t))
                } else {
                    (recipient_text.trim(), None)
                };
            let _ = template;
            arguments.insert(
                0,
                ("recipient".to_string(), parse_expression(recipient.trim())),
            );
            return Some(StatementKind::ServiceCall {
                service: sanitize_name(service.trim()),
                action: "send_email".to_string(),
                arguments,
            });
        }
    }

    // send message via <Service> to <recipient>
    if let Some(rest) = strip_prefix_ci(line, "Send message via ") {
        if let Some((service, recipient)) = split_once_ci(rest.trim(), " to ") {
            return Some(StatementKind::ServiceCall {
                service: sanitize_name(service.trim()),
                action: "send_message".to_string(),
                arguments: vec![("recipient".to_string(), parse_expression(recipient.trim()))],
            });
        }
    }

    // send message to <recipient>
    if let Some(rest) = strip_prefix_ci(line, "Send message to ") {
        return Some(StatementKind::ServiceCall {
            service: "messaging".to_string(),
            action: "send".to_string(),
            arguments: vec![("recipient".to_string(), parse_expression(rest.trim()))],
        });
    }

    // send email to <recipient> (no "via")
    if let Some(rest) = strip_prefix_ci(line, "Send email to ") {
        return Some(StatementKind::ServiceCall {
            service: "email".to_string(),
            action: "send".to_string(),
            arguments: vec![("recipient".to_string(), parse_expression(rest.trim()))],
        });
    }

    None
}

fn parse_field_list(text: &str) -> Vec<(String, Expression)> {
    let mut fields = Vec::new();
    // Parse "key as value, key as value" or "key as value and key as value"
    let parts = split_list_items(text);
    for part in parts {
        if let Some((value_text, key_text)) = split_once_ci(part.trim(), " as ") {
            fields.push((
                sanitize_name(key_text.trim()),
                parse_expression(value_text.trim()),
            ));
        }
    }
    fields
}

/// Parses `Use ...` module statements. `rest` is the text after the `Use `
/// keyword with any trailing period removed. Returns Ok(None) when the text
/// does not end in ` module`, so other statement forms can still claim it.
fn parse_use_module(
    rest: &str,
    line_number: usize,
    line: &str,
) -> Result<Option<Statement>, CompileError> {
    let lower = rest.to_ascii_lowercase();
    let Some(body) = lower
        .ends_with(" module")
        .then(|| rest[..rest.len() - " module".len()].trim())
    else {
        return Ok(None);
    };

    // Whole-module form: `Use the math module`
    if let Some(name) = strip_prefix_ci(body, "the ") {
        let name = name.trim();
        if is_identifier_text(name) {
            return Ok(Some(statement(
                line_number,
                line,
                StatementKind::UseModule {
                    module: sanitize_name(name),
                    symbols: Vec::new(),
                },
            )));
        }
    }

    // Selective form: `Use pi and tau from the math module`. Split at the
    // LAST ` from the ` so a symbol name containing the phrase (`distance
    // from the sun`) does not swallow the module name (DEVL-131 review).
    const FROM_THE: &str = " from the ";
    let from_split = body
        .to_ascii_lowercase()
        .rfind(FROM_THE)
        .map(|idx| (&body[..idx], &body[idx + FROM_THE.len()..]));
    if let Some((symbols_part, module_part)) = from_split {
        let name = module_part.trim();
        if !is_identifier_text(name) {
            return Err(CompileError::single(
                line_number,
                format!("Invalid module name in Use statement: {name}"),
                line,
            ));
        }
        let symbols: Vec<String> = split_list_items(symbols_part.trim())
            .iter()
            .map(|item| sanitize_name(item))
            .collect();
        if symbols.is_empty() || symbols.iter().any(String::is_empty) {
            return Err(CompileError::single(
                line_number,
                "Use statement lists no symbols before 'from the'",
                line,
            ));
        }
        return Ok(Some(statement(
            line_number,
            line,
            StatementKind::UseModule {
                module: sanitize_name(name),
                symbols,
            },
        )));
    }

    Err(CompileError::single(
        line_number,
        format!(
            "Malformed Use statement. Expected 'Use the <name> module' or \
             'Use <symbol> and <symbol> from the <name> module', got: Use {rest}"
        ),
        line,
    ))
}

fn split_list_items(text: &str) -> Vec<&str> {
    // Empty segments are skipped so an Oxford comma ("a, b, and c") reads as
    // a single separator instead of producing a phantom null item.
    let mut items = Vec::new();
    let mut start = 0;
    let lower = text.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i] == b',' {
            let segment = text[start..i].trim();
            if !segment.is_empty() {
                items.push(segment);
            }
            start = i + 1;
            i += 1;
        } else if i + 5 <= len && &lower[i..i + 5] == " and " {
            let segment = text[start..i].trim();
            if !segment.is_empty() {
                items.push(segment);
            }
            start = i + 5;
            i += 5;
        } else {
            i += 1;
        }
    }
    if start < len {
        let segment = text[start..].trim();
        if !segment.is_empty() {
            items.push(segment);
        }
    }
    items
}

fn field_names_expression(text: &str) -> Expression {
    let fields = split_list_items(text)
        .into_iter()
        .filter_map(|item| {
            let field = sanitize_name(&unquote(item.trim()));
            (!field.is_empty()).then(|| Expression::Literal(Value::String(field)))
        })
        .collect();
    Expression::ListLiteral(fields)
}

fn split_trailing_if<'a>(text: &'a str) -> (&'a str, Option<&'a str>) {
    // Find the last " if " in the text to split off a trailing condition.
    // We must avoid splitting inside quoted strings.
    let lower = text.to_ascii_lowercase();
    let mut last_if = None;
    let mut in_quote = false;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        if in_quote {
            if ch == '\\' {
                i += 1; // skip next
            } else if ch == '"' {
                in_quote = false;
            }
        } else if ch == '"' {
            // Only double quotes delimit strings; a `'` is English text
            // (`math's pi if flag` must still find its trailing ` if `).
            in_quote = true;
        } else if i + 4 <= lower.len() && &lower[i..i + 4] == " if " {
            last_if = Some(i);
        }
        i += 1;
    }

    if let Some(idx) = last_if {
        let value_part = text[..idx].trim();
        let cond_part = text[idx + 4..].trim();
        if !cond_part.is_empty() {
            return (value_part, Some(cond_part));
        }
    }
    (text, None)
}

fn unquote(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2 {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

fn parse_assertion(line_number: usize, line: &str, rest: &str) -> Result<Statement, CompileError> {
    let (body, assertion_id) = split_assertion_id(rest).ok_or_else(|| {
        CompileError::single(line_number, "Expected assertion id with `as \"id\"`", line)
    })?;

    if let Some(target) = strip_suffix_ci(body.trim(), " is not spreadsheet error") {
        return Ok(statement(
            line_number,
            line,
            StatementKind::Assertion {
                assertion_id,
                target: sanitize_name(target),
                operator: AssertionOperator::NotSpreadsheetError,
                expected: None,
            },
        ));
    }

    if let Some(target) = strip_suffix_ci(body.trim(), " is present") {
        return Ok(statement(
            line_number,
            line,
            StatementKind::Assertion {
                assertion_id,
                target: sanitize_name(target),
                operator: AssertionOperator::Present,
                expected: None,
            },
        ));
    }

    if let Some((target, expected)) = split_once_ci(body.trim(), " contains ") {
        return Ok(statement(
            line_number,
            line,
            StatementKind::Assertion {
                assertion_id,
                target: sanitize_name(target),
                operator: AssertionOperator::Contains,
                expected: Some(parse_expression(expected.trim())),
            },
        ));
    }

    if let Some((target, expected)) = split_once_ci(body.trim(), " equals ") {
        return Ok(statement(
            line_number,
            line,
            StatementKind::Assertion {
                assertion_id,
                target: sanitize_name(target),
                operator: AssertionOperator::Equals,
                expected: Some(parse_expression(expected.trim())),
            },
        ));
    }

    Err(CompileError::single(
        line_number,
        "Unsupported assertion syntax",
        line,
    ))
}

fn statement(line: usize, source_text: &str, kind: StatementKind) -> Statement {
    Statement {
        line,
        source_text: source_text.to_string(),
        kind,
    }
}

#[derive(Debug, Clone)]
struct LoopContext {
    start_address: usize,
    break_patches: Vec<usize>,
}

struct BytecodeCompiler {
    options: CompileOptions,
    constants: Vec<Value>,
    constant_index: HashMap<String, usize>,
    symbols: Vec<String>,
    symbol_index: HashMap<String, usize>,
    instructions: Vec<Value>,
    source_map: Vec<Value>,
    effects: Vec<Value>,
    next_register: usize,
    loop_stack: Vec<LoopContext>,
    /// Sibling methods of the class being compiled, keyed by ruby_name.
    /// Method calls to these are inlined at the call site (DEVL-132) —
    /// there is no runtime call mechanism. Empty for flat programs.
    methods: HashMap<String, MethodDef>,
    /// Uniquifies the alpha-renaming prefix of each inline site.
    inline_counter: usize,
}

impl BytecodeCompiler {
    fn new(options: CompileOptions) -> Self {
        Self {
            options,
            constants: Vec::new(),
            constant_index: HashMap::new(),
            symbols: Vec::new(),
            symbol_index: HashMap::new(),
            instructions: Vec::new(),
            source_map: Vec::new(),
            effects: Vec::new(),
            methods: HashMap::new(),
            inline_counter: 0,
            next_register: 0,
            loop_stack: Vec::new(),
        }
    }

    fn compile(mut self, program: Program, closure: SourceClosure) -> BytecodePackage {
        for statement in program.statements {
            self.compile_statement(&statement);
        }
        self.emit("RETURN", Map::new(), None);

        let imports = self.imports();
        let manifest = program
            .manifest
            .filter(|m| !m.is_empty())
            .map(|m| m.to_value());
        let stdlib = closure.stdlib_value();
        BytecodePackage {
            format: FORMAT,
            format_version: FORMAT_VERSION,
            compiler_version: COMPILER_VERSION,
            source_path: self.options.source_path,
            source_hash: closure.hash,
            source_files: closure.files,
            constant_pool: self.constants,
            symbol_table: self.symbols,
            instructions: self.instructions,
            source_map: self.source_map,
            effect_table: self.effects,
            imports,
            class_info: None,
            methods: None,
            manifest,
            stdlib,
        }
    }

    fn compile_statement(&mut self, statement: &Statement) {
        match &statement.kind {
            StatementKind::Input {
                target,
                prompt,
                source,
            } => {
                let prompt_register = self.compile_expression(prompt, statement);
                self.register_symbol(target);
                self.emit(
                    "ASK",
                    map(vec![
                        ("target", string_value(target)),
                        ("prompt", string_value(&prompt_register)),
                        ("input_source", string_value(source.as_str())),
                    ]),
                    Some(statement),
                );
                self.record_effect("input", statement, vec![("target", string_value(target))]);
            }
            StatementKind::Assignment { target, value } => {
                let value_register = self.compile_expression(value, statement);
                self.register_symbol(target);
                self.emit(
                    "STORE",
                    map(vec![
                        ("symbol", string_value(target)),
                        ("value", string_value(&value_register)),
                    ]),
                    Some(statement),
                );
            }
            StatementKind::Output { value } => {
                let value_register = self.compile_expression(value, statement);
                self.emit(
                    "PRINT",
                    map(vec![("value", string_value(&value_register))]),
                    Some(statement),
                );
            }
            StatementKind::FileWrite { value, path, mode } => {
                let value_register = self.compile_expression(value, statement);
                let path_register = self.compile_expression(path, statement);
                self.emit(
                    "EXPORT",
                    map(vec![
                        ("value", string_value(&value_register)),
                        ("path", string_value(&path_register)),
                        ("mode", string_value(mode.as_str())),
                    ]),
                    Some(statement),
                );
                self.record_effect(
                    "file_write",
                    statement,
                    vec![("mode", string_value(mode.as_str()))],
                );
            }
            StatementKind::FileRead {
                path,
                target,
                format,
            } => {
                let path_register = self.compile_expression(path, statement);
                self.register_symbol(target);
                self.emit(
                    "READ_FILE",
                    map(vec![
                        ("path", string_value(&path_register)),
                        ("target", string_value(target)),
                        ("format", string_value(format.as_str())),
                    ]),
                    Some(statement),
                );
                self.record_effect(
                    "file_read",
                    statement,
                    vec![
                        ("target", string_value(target)),
                        ("format", string_value(format.as_str())),
                    ],
                );
            }
            StatementKind::Branch {
                condition,
                then_statements,
                else_statements,
            } => {
                let condition_register = self.compile_expression(condition, statement);
                let false_jump = self.emit(
                    "JUMP_IF_FALSE",
                    map(vec![
                        ("condition", string_value(&condition_register)),
                        ("target", Value::Null),
                    ]),
                    Some(statement),
                );
                for child in then_statements {
                    self.compile_statement(child);
                }
                if else_statements.is_empty() {
                    self.patch_target(false_jump, self.instructions.len());
                } else {
                    let end_jump =
                        self.emit("JUMP", map(vec![("target", Value::Null)]), Some(statement));
                    self.patch_target(false_jump, self.instructions.len());
                    for child in else_statements {
                        self.compile_statement(child);
                    }
                    self.patch_target(end_jump, self.instructions.len());
                }
            }
            StatementKind::ReadXlsxCell {
                sheet,
                cell,
                target,
            } => {
                self.register_symbol(target);
                self.emit(
                    "XLSX_READ_CELL",
                    map(vec![
                        ("target", string_value(target)),
                        ("sheet", string_value(sheet)),
                        ("cell", string_value(cell)),
                    ]),
                    Some(statement),
                );
                self.record_effect(
                    "xlsx_read_cell",
                    statement,
                    vec![
                        ("target", string_value(target)),
                        ("sheet", string_value(sheet)),
                        ("cell", string_value(cell)),
                    ],
                );
            }
            StatementKind::ReadPdfText { path, target } => {
                self.register_symbol(target);
                self.emit(
                    "PDF_READ_TEXT",
                    map(vec![
                        ("target", string_value(target)),
                        ("path", string_value(path)),
                    ]),
                    Some(statement),
                );
                self.record_effect(
                    "pdf_read_text",
                    statement,
                    vec![
                        ("target", string_value(target)),
                        ("path", string_value(path)),
                    ],
                );
            }
            StatementKind::ReadDocxText { path, target } => {
                self.register_symbol(target);
                self.emit(
                    "DOCX_READ_TEXT",
                    map(vec![
                        ("target", string_value(target)),
                        ("path", string_value(path)),
                    ]),
                    Some(statement),
                );
                self.record_effect(
                    "docx_read_text",
                    statement,
                    vec![
                        ("target", string_value(target)),
                        ("path", string_value(path)),
                    ],
                );
            }
            StatementKind::Assertion {
                assertion_id,
                target,
                operator,
                expected,
            } => {
                let expected_register = expected
                    .as_ref()
                    .map(|expr| self.compile_expression(expr, statement));
                let mut operands = map(vec![
                    ("assertion_id", string_value(assertion_id)),
                    ("target", string_value(target)),
                    ("operator", string_value(operator.as_str())),
                ]);
                if let Some(register) = expected_register {
                    operands.insert("expected".to_string(), string_value(&register));
                }
                self.emit("ASSERT", operands, Some(statement));
                self.record_effect(
                    "assertion",
                    statement,
                    vec![
                        ("assertion_id", string_value(assertion_id)),
                        ("target", string_value(target)),
                        ("operator", string_value(operator.as_str())),
                    ],
                );
            }
            StatementKind::ExportAssertions { path } => {
                let path_register = self.compile_expression(path, statement);
                self.emit(
                    "EXPORT_ASSERTIONS",
                    map(vec![("path", string_value(&path_register))]),
                    Some(statement),
                );
                self.record_effect(
                    "file_write",
                    statement,
                    vec![("mode", string_value("assertions"))],
                );
            }
            StatementKind::WhileLoop { condition, body } => {
                let loop_start = self.instructions.len();
                self.loop_stack.push(LoopContext {
                    start_address: loop_start,
                    break_patches: Vec::new(),
                });
                let cond_register = self.compile_expression(condition, statement);
                let false_jump = self.emit(
                    "JUMP_IF_FALSE",
                    map(vec![
                        ("condition", string_value(&cond_register)),
                        ("target", Value::Null),
                    ]),
                    Some(statement),
                );
                for child in body {
                    self.compile_statement(child);
                }
                self.emit(
                    "JUMP",
                    map(vec![("target", number_value(loop_start as i64))]),
                    Some(statement),
                );
                let loop_end = self.instructions.len();
                self.patch_target(false_jump, loop_end);
                let ctx = self.loop_stack.pop().unwrap();
                for patch in ctx.break_patches {
                    self.patch_target(patch, loop_end);
                }
            }
            StatementKind::UntilLoop { condition, body } => {
                let loop_start = self.instructions.len();
                self.loop_stack.push(LoopContext {
                    start_address: loop_start,
                    break_patches: Vec::new(),
                });
                let cond_register = self.compile_expression(condition, statement);
                // Until = run while condition is false, so jump out when true
                let true_jump = self.emit(
                    "JUMP_IF_FALSE",
                    map(vec![
                        ("condition", string_value(&cond_register)),
                        ("target", Value::Null),
                    ]),
                    Some(statement),
                );
                // We need to invert: jump IF TRUE (condition met).
                // Actually, Until means "loop until condition is true" = "loop while NOT condition".
                // JUMP_IF_FALSE jumps when condition is false, which means we continue.
                // We need to NOT the condition first, then JUMP_IF_FALSE.
                // Simpler: emit NOT of condition, then JUMP_IF_FALSE.
                // Let me redo this properly:
                // Actually the instruction is already emitted. Let me patch it.
                // The simplest approach: emit the condition, then negate it, then JUMP_IF_FALSE.
                // But we already emitted JUMP_IF_FALSE. Let me remove it and redo.
                // Actually we can just use the condition directly: Until means "stop when true".
                // So we want to exit when condition IS true. JUMP_IF_FALSE exits when false.
                // We need the opposite: exit when true.
                // Solution: negate condition first, then JUMP_IF_FALSE.

                // Remove the premature JUMP_IF_FALSE
                self.instructions.pop();
                self.source_map.pop();

                // Negate the condition
                let negated_register = self.next_register();
                self.emit(
                    "NOT",
                    map(vec![
                        ("dest", string_value(&negated_register)),
                        ("operand", string_value(&cond_register)),
                    ]),
                    Some(statement),
                );
                let false_jump = self.emit(
                    "JUMP_IF_FALSE",
                    map(vec![
                        ("condition", string_value(&negated_register)),
                        ("target", Value::Null),
                    ]),
                    Some(statement),
                );
                let _ = true_jump;

                for child in body {
                    self.compile_statement(child);
                }
                self.emit(
                    "JUMP",
                    map(vec![("target", number_value(loop_start as i64))]),
                    Some(statement),
                );
                let loop_end = self.instructions.len();
                self.patch_target(false_jump, loop_end);
                let ctx = self.loop_stack.pop().unwrap();
                for patch in ctx.break_patches {
                    self.patch_target(patch, loop_end);
                }
            }
            StatementKind::ForEach {
                item,
                collection,
                body,
            } => {
                let coll_register = self.compile_expression(collection, statement);
                let len_register = self.next_register();
                self.emit(
                    "LIST_LEN",
                    map(vec![
                        ("dest", string_value(&len_register)),
                        ("source", string_value(&coll_register)),
                    ]),
                    Some(statement),
                );

                // Initialize index to 0
                let idx_symbol = format!("__foreach_idx_{}", item);
                self.register_symbol(&idx_symbol);
                let zero_register = self.next_register();
                let zero_const = self.add_constant(number_value(0));
                self.emit(
                    "CONST",
                    map(vec![
                        ("dest", string_value(&zero_register)),
                        ("const", number_value(zero_const as i64)),
                    ]),
                    Some(statement),
                );
                self.emit(
                    "STORE",
                    map(vec![
                        ("symbol", string_value(&idx_symbol)),
                        ("value", string_value(&zero_register)),
                    ]),
                    Some(statement),
                );

                let loop_start = self.instructions.len();
                self.loop_stack.push(LoopContext {
                    start_address: loop_start,
                    break_patches: Vec::new(),
                });

                // Check index < length
                let idx_load = self.next_register();
                self.emit(
                    "LOAD",
                    map(vec![
                        ("dest", string_value(&idx_load)),
                        ("symbol", string_value(&idx_symbol)),
                    ]),
                    Some(statement),
                );
                let cmp_register = self.next_register();
                self.emit(
                    "LT",
                    map(vec![
                        ("dest", string_value(&cmp_register)),
                        ("left", string_value(&idx_load)),
                        ("right", string_value(&len_register)),
                    ]),
                    Some(statement),
                );
                let exit_jump = self.emit(
                    "JUMP_IF_FALSE",
                    map(vec![
                        ("condition", string_value(&cmp_register)),
                        ("target", Value::Null),
                    ]),
                    Some(statement),
                );

                // Get current item
                self.register_symbol(item);
                let item_register = self.next_register();
                let idx_load2 = self.next_register();
                self.emit(
                    "LOAD",
                    map(vec![
                        ("dest", string_value(&idx_load2)),
                        ("symbol", string_value(&idx_symbol)),
                    ]),
                    Some(statement),
                );
                self.emit(
                    "LIST_GET",
                    map(vec![
                        ("dest", string_value(&item_register)),
                        ("source", string_value(&coll_register)),
                        ("index", string_value(&idx_load2)),
                    ]),
                    Some(statement),
                );
                self.emit(
                    "STORE",
                    map(vec![
                        ("symbol", string_value(item)),
                        ("value", string_value(&item_register)),
                    ]),
                    Some(statement),
                );

                // Body
                for child in body {
                    self.compile_statement(child);
                }

                // Increment index
                let idx_load3 = self.next_register();
                self.emit(
                    "LOAD",
                    map(vec![
                        ("dest", string_value(&idx_load3)),
                        ("symbol", string_value(&idx_symbol)),
                    ]),
                    Some(statement),
                );
                let one_register = self.next_register();
                let one_const = self.add_constant(number_value(1));
                self.emit(
                    "CONST",
                    map(vec![
                        ("dest", string_value(&one_register)),
                        ("const", number_value(one_const as i64)),
                    ]),
                    Some(statement),
                );
                let new_idx = self.next_register();
                self.emit(
                    "ADD",
                    map(vec![
                        ("dest", string_value(&new_idx)),
                        ("left", string_value(&idx_load3)),
                        ("right", string_value(&one_register)),
                    ]),
                    Some(statement),
                );
                self.emit(
                    "STORE",
                    map(vec![
                        ("symbol", string_value(&idx_symbol)),
                        ("value", string_value(&new_idx)),
                    ]),
                    Some(statement),
                );

                self.emit(
                    "JUMP",
                    map(vec![("target", number_value(loop_start as i64))]),
                    Some(statement),
                );
                let loop_end = self.instructions.len();
                self.patch_target(exit_jump, loop_end);
                let ctx = self.loop_stack.pop().unwrap();
                for patch in ctx.break_patches {
                    self.patch_target(patch, loop_end);
                }
            }
            StatementKind::TryRecover { body, recovery } => {
                let try_begin = self.emit(
                    "TRY_BEGIN",
                    map(vec![("handler", Value::Null)]),
                    Some(statement),
                );
                for child in body {
                    self.compile_statement(child);
                }
                self.emit("TRY_END", Map::new(), Some(statement));
                let skip_recovery =
                    self.emit("JUMP", map(vec![("target", Value::Null)]), Some(statement));
                let recovery_start = self.instructions.len();
                self.patch_handler(try_begin, recovery_start);
                for child in recovery {
                    self.compile_statement(child);
                }
                self.patch_target(skip_recovery, self.instructions.len());
            }
            StatementKind::Break => {
                let break_jump =
                    self.emit("JUMP", map(vec![("target", Value::Null)]), Some(statement));
                if let Some(ctx) = self.loop_stack.last_mut() {
                    ctx.break_patches.push(break_jump);
                }
            }
            StatementKind::Continue => {
                if let Some(ctx) = self.loop_stack.last() {
                    let start = ctx.start_address;
                    self.emit(
                        "JUMP",
                        map(vec![("target", number_value(start as i64))]),
                        Some(statement),
                    );
                }
            }
            StatementKind::Fail { message } => {
                let msg_register = self.compile_expression(message, statement);
                self.emit(
                    "FAIL",
                    map(vec![("message", string_value(&msg_register))]),
                    Some(statement),
                );
            }
            StatementKind::Require { condition, message } => {
                let cond_register = self.compile_expression(condition, statement);
                let skip_jump = self.emit(
                    "JUMP_IF_FALSE",
                    map(vec![
                        ("condition", string_value(&cond_register)),
                        ("target", Value::Null),
                    ]),
                    Some(statement),
                );
                // If condition is true, jump past the FAIL
                // Actually JUMP_IF_FALSE jumps when false. If false, we should FAIL.
                // So: JUMP_IF_FALSE -> continue (past the end_jump)
                // No: if condition is false, we fail. So:
                // JUMP_IF_FALSE -> to FAIL block? No.
                // Let's think: require means "condition must be true, otherwise fail"
                // So: if NOT false (i.e., true), skip fail. If false, fail.
                // JUMP_IF_FALSE jumps when condition is false.
                // We want: when false -> fail. So we DON'T jump, we fall through to FAIL.
                // When true -> skip fail. So we jump past FAIL.
                // But JUMP_IF_FALSE jumps on false, not true. We need the opposite.
                // Solution: negate, then JUMP_IF_FALSE. Or just use a different pattern.
                // Simpler: remove the premature instruction and use the right pattern.

                // Remove the instruction we just emitted
                self.instructions.pop();
                self.source_map.pop();
                let _ = skip_jump;

                // Negate condition: NOT
                let neg_register = self.next_register();
                self.emit(
                    "NOT",
                    map(vec![
                        ("dest", string_value(&neg_register)),
                        ("operand", string_value(&cond_register)),
                    ]),
                    Some(statement),
                );
                // JUMP_IF_FALSE on negated = jump when NOT false = jump when true = skip fail
                let skip = self.emit(
                    "JUMP_IF_FALSE",
                    map(vec![
                        ("condition", string_value(&neg_register)),
                        ("target", Value::Null),
                    ]),
                    Some(statement),
                );
                // FAIL
                if let Some(msg) = message {
                    let msg_register = self.compile_expression(msg, statement);
                    self.emit(
                        "FAIL",
                        map(vec![("message", string_value(&msg_register))]),
                        Some(statement),
                    );
                } else {
                    let msg_register = self.next_register();
                    let msg_const =
                        self.add_constant(Value::String("Requirement failed".to_string()));
                    self.emit(
                        "CONST",
                        map(vec![
                            ("dest", string_value(&msg_register)),
                            ("const", number_value(msg_const as i64)),
                        ]),
                        Some(statement),
                    );
                    self.emit(
                        "FAIL",
                        map(vec![("message", string_value(&msg_register))]),
                        Some(statement),
                    );
                }
                self.patch_target(skip, self.instructions.len());
            }
            StatementKind::SetField {
                target,
                value,
                condition,
            } => {
                if let Some(cond) = condition {
                    let cond_register = self.compile_expression(cond, statement);
                    let skip_jump = self.emit(
                        "JUMP_IF_FALSE",
                        map(vec![
                            ("condition", string_value(&cond_register)),
                            ("target", Value::Null),
                        ]),
                        Some(statement),
                    );
                    self.compile_set_field_inner(target, value, statement);
                    self.patch_target(skip_jump, self.instructions.len());
                } else {
                    self.compile_set_field_inner(target, value, statement);
                }
            }
            StatementKind::Append { value, target } => {
                let val_register = self.compile_expression(value, statement);
                self.register_symbol(target);
                self.emit(
                    "LIST_APPEND",
                    map(vec![
                        ("target", string_value(target)),
                        ("value", string_value(&val_register)),
                    ]),
                    Some(statement),
                );
            }
            StatementKind::Pop { source, store_as } => {
                self.register_symbol(source);
                self.register_symbol(store_as);
                self.emit(
                    "LIST_POP",
                    map(vec![
                        ("source", string_value(source)),
                        ("dest", string_value(store_as)),
                    ]),
                    Some(statement),
                );
            }
            StatementKind::ConditionalAssignment {
                target,
                value,
                condition,
            } => {
                let cond_register = self.compile_expression(condition, statement);
                let skip_jump = self.emit(
                    "JUMP_IF_FALSE",
                    map(vec![
                        ("condition", string_value(&cond_register)),
                        ("target", Value::Null),
                    ]),
                    Some(statement),
                );
                let val_register = self.compile_expression(value, statement);
                self.register_symbol(target);
                self.emit(
                    "STORE",
                    map(vec![
                        ("symbol", string_value(target)),
                        ("value", string_value(&val_register)),
                    ]),
                    Some(statement),
                );
                self.patch_target(skip_jump, self.instructions.len());
            }
            StatementKind::Bind {
                source_name,
                target_name,
                ..
            } => {
                self.register_symbol(source_name);
                self.register_symbol(target_name);
                let reg = self.next_register();
                self.emit(
                    "LOAD",
                    map(vec![
                        ("dest", string_value(&reg)),
                        ("symbol", string_value(source_name)),
                    ]),
                    Some(statement),
                );
                self.emit(
                    "STORE",
                    map(vec![
                        ("symbol", string_value(target_name)),
                        ("value", string_value(&reg)),
                    ]),
                    Some(statement),
                );
            }
            StatementKind::Definition { .. } => {
                // Metadata only, no bytecode emitted
            }
            StatementKind::Validate {
                target,
                rule,
                value,
            } => {
                let target_name = expression_label(target);
                let target_reg = self.compile_expression(target, statement);
                let mut operands = vec![
                    ("target", string_value(&target_name)),
                    ("actual", string_value(&target_reg)),
                    ("rule", string_value(rule.as_str())),
                ];
                if let Some(value) = value {
                    let val_reg = self.compile_expression(value, statement);
                    operands.push(("expected", string_value(&val_reg)));
                }
                self.emit("VALIDATE", map(operands), Some(statement));
                self.record_effect(
                    "validation",
                    statement,
                    vec![
                        ("target", string_value(&target_name)),
                        ("rule", string_value(rule.as_str())),
                    ],
                );
            }
            StatementKind::ReadStdin { target } => {
                self.register_symbol(target);
                let prompt_reg = self.next_register();
                let prompt_const = self.add_constant(Value::String(String::new()));
                self.emit(
                    "CONST",
                    map(vec![
                        ("dest", string_value(&prompt_reg)),
                        ("const", number_value(prompt_const as i64)),
                    ]),
                    Some(statement),
                );
                self.emit(
                    "ASK",
                    map(vec![
                        ("target", string_value(target)),
                        ("prompt", string_value(&prompt_reg)),
                        ("input_source", string_value(InputSource::Stdin.as_str())),
                    ]),
                    Some(statement),
                );
                self.record_effect("input", statement, vec![("target", string_value(target))]);
            }
            StatementKind::ServiceCall {
                service,
                action,
                arguments,
            } => {
                let dest = self.next_register();
                let args_register = if arguments.is_empty() {
                    String::new()
                } else {
                    let mut keys = Vec::new();
                    let mut value_registers = Vec::new();
                    for (key, value) in arguments {
                        let val_reg = self.compile_expression(value, statement);
                        keys.push(string_value(key));
                        value_registers.push(string_value(&val_reg));
                    }
                    let args_reg = self.next_register();
                    self.emit(
                        "RECORD_BUILD",
                        map(vec![
                            ("dest", string_value(&args_reg)),
                            ("keys", Value::Array(keys)),
                            ("values", Value::Array(value_registers)),
                        ]),
                        Some(statement),
                    );
                    args_reg
                };
                let mut operands = vec![
                    ("service", string_value(service)),
                    ("action", string_value(action)),
                    ("dest", string_value(&dest)),
                ];
                if !args_register.is_empty() {
                    operands.push(("args", string_value(&args_register)));
                }
                self.emit("SERVICE_CALL", map(operands), Some(statement));
                self.record_effect(
                    "service_call",
                    statement,
                    vec![
                        ("service", string_value(service)),
                        ("action", string_value(action)),
                    ],
                );
            }
            StatementKind::HttpRequest {
                method,
                url,
                body,
                dest,
            } => {
                let url_reg = self.compile_expression(url, statement);
                let body_reg = if let Some(body_expr) = body {
                    self.compile_expression(body_expr, statement)
                } else {
                    String::new()
                };
                let dest_sym = sanitize_name(dest);
                let dest_reg = self.next_register();
                let mut operands = vec![
                    ("method", string_value(method)),
                    ("url", string_value(&url_reg)),
                    ("dest", string_value(&dest_reg)),
                ];
                if !body_reg.is_empty() {
                    operands.push(("body", string_value(&body_reg)));
                }
                self.emit("HTTP_REQUEST", map(operands), Some(statement));
                self.register_symbol(&dest_sym);
                self.emit(
                    "STORE",
                    map(vec![
                        ("symbol", string_value(&dest_sym)),
                        ("value", string_value(&dest_reg)),
                    ]),
                    Some(statement),
                );
                self.record_effect(
                    "http_request",
                    statement,
                    vec![("method", string_value(method))],
                );
            }
            StatementKind::HttpDownload { url, path } => {
                let url_reg = self.compile_expression(url, statement);
                let path_reg = self.compile_expression(path, statement);
                self.emit(
                    "HTTP_DOWNLOAD",
                    map(vec![
                        ("url", string_value(&url_reg)),
                        ("path", string_value(&path_reg)),
                    ]),
                    Some(statement),
                );
                self.record_effect("http_download", statement, vec![]);
            }
            StatementKind::XlsxReadRows { path, sheet, dest } => {
                let path_reg = self.compile_expression(path, statement);
                let dest_sym = sanitize_name(dest);
                let dest_reg = self.next_register();
                let mut operands = vec![
                    ("path", string_value(&path_reg)),
                    ("dest", string_value(&dest_reg)),
                ];
                if let Some(sheet_name) = sheet {
                    operands.push(("sheet", string_value(sheet_name)));
                }
                self.emit("XLSX_READ_ROWS", map(operands), Some(statement));
                self.register_symbol(&dest_sym);
                self.emit(
                    "STORE",
                    map(vec![
                        ("symbol", string_value(&dest_sym)),
                        ("value", string_value(&dest_reg)),
                    ]),
                    Some(statement),
                );
                self.record_effect("xlsx_read_rows", statement, vec![]);
            }
            StatementKind::FileCopy {
                source,
                destination,
            } => {
                let src_reg = self.compile_expression(source, statement);
                let dst_reg = self.compile_expression(destination, statement);
                self.emit(
                    "FILE_COPY",
                    map(vec![
                        ("source", string_value(&src_reg)),
                        ("destination", string_value(&dst_reg)),
                    ]),
                    Some(statement),
                );
                self.record_effect("file_copy", statement, vec![]);
            }
            StatementKind::FileMove {
                source,
                destination,
            } => {
                let src_reg = self.compile_expression(source, statement);
                let dst_reg = self.compile_expression(destination, statement);
                self.emit(
                    "FILE_MOVE",
                    map(vec![
                        ("source", string_value(&src_reg)),
                        ("destination", string_value(&dst_reg)),
                    ]),
                    Some(statement),
                );
                self.record_effect("file_move", statement, vec![]);
            }
            StatementKind::FileMkdir { path } => {
                let path_reg = self.compile_expression(path, statement);
                self.emit(
                    "FILE_MKDIR",
                    map(vec![("path", string_value(&path_reg))]),
                    Some(statement),
                );
                self.record_effect("file_mkdir", statement, vec![]);
            }
            StatementKind::FileDelete { path } => {
                let path_reg = self.compile_expression(path, statement);
                self.emit(
                    "FILE_DELETE",
                    map(vec![("path", string_value(&path_reg))]),
                    Some(statement),
                );
                self.record_effect("file_delete", statement, vec![]);
            }
            StatementKind::FileExists { path, dest } => {
                let path_reg = self.compile_expression(path, statement);
                let dest_sym = sanitize_name(dest);
                let dest_reg = self.next_register();
                self.emit(
                    "FILE_EXISTS",
                    map(vec![
                        ("path", string_value(&path_reg)),
                        ("dest", string_value(&dest_reg)),
                    ]),
                    Some(statement),
                );
                self.register_symbol(&dest_sym);
                self.emit(
                    "STORE",
                    map(vec![
                        ("symbol", string_value(&dest_sym)),
                        ("value", string_value(&dest_reg)),
                    ]),
                    Some(statement),
                );
                self.record_effect("file_exists", statement, vec![]);
            }
            StatementKind::FileStat { path, dest } => {
                let path_reg = self.compile_expression(path, statement);
                let dest_sym = sanitize_name(dest);
                let dest_reg = self.next_register();
                self.emit(
                    "FILE_STAT",
                    map(vec![
                        ("path", string_value(&path_reg)),
                        ("dest", string_value(&dest_reg)),
                    ]),
                    Some(statement),
                );
                self.register_symbol(&dest_sym);
                self.emit(
                    "STORE",
                    map(vec![
                        ("symbol", string_value(&dest_sym)),
                        ("value", string_value(&dest_reg)),
                    ]),
                    Some(statement),
                );
                self.record_effect("file_stat", statement, vec![]);
            }
            StatementKind::FileList { path, dest } => {
                let path_reg = self.compile_expression(path, statement);
                let dest_sym = sanitize_name(dest);
                let dest_reg = self.next_register();
                self.emit(
                    "FILE_LIST",
                    map(vec![
                        ("path", string_value(&path_reg)),
                        ("dest", string_value(&dest_reg)),
                    ]),
                    Some(statement),
                );
                self.register_symbol(&dest_sym);
                self.emit(
                    "STORE",
                    map(vec![
                        ("symbol", string_value(&dest_sym)),
                        ("value", string_value(&dest_reg)),
                    ]),
                    Some(statement),
                );
                self.record_effect("file_list", statement, vec![]);
            }
            StatementKind::FileGlob {
                pattern,
                directory,
                dest,
            } => {
                let pattern_reg = self.compile_expression(pattern, statement);
                let dir_reg = self.compile_expression(directory, statement);
                let dest_sym = sanitize_name(dest);
                let dest_reg = self.next_register();
                self.emit(
                    "FILE_GLOB",
                    map(vec![
                        ("pattern", string_value(&pattern_reg)),
                        ("directory", string_value(&dir_reg)),
                        ("dest", string_value(&dest_reg)),
                    ]),
                    Some(statement),
                );
                self.register_symbol(&dest_sym);
                self.emit(
                    "STORE",
                    map(vec![
                        ("symbol", string_value(&dest_sym)),
                        ("value", string_value(&dest_reg)),
                    ]),
                    Some(statement),
                );
                self.record_effect("file_glob", statement, vec![]);
            }
            StatementKind::Load { path, alias } => {
                let path_str = path.as_deref().unwrap_or("document");
                let alias_str = alias
                    .as_deref()
                    .unwrap_or_else(|| path.as_deref().unwrap_or("document"));
                let path_register = {
                    let reg = self.next_register();
                    let ci = self.add_constant(Value::String(path_str.to_string()));
                    self.emit(
                        "CONST",
                        map(vec![
                            ("const", number_value(ci as i64)),
                            ("dest", string_value(&reg)),
                        ]),
                        Some(statement),
                    );
                    reg
                };
                self.emit(
                    "LOAD_FILE",
                    map(vec![
                        ("path", string_value(&path_register)),
                        ("alias", string_value(&sanitize_name(alias_str))),
                    ]),
                    Some(statement),
                );
                self.record_effect(
                    "read_file",
                    statement,
                    vec![("path", string_value(path_str))],
                );
            }
            StatementKind::Extract { target, store_as } => {
                // Look up the most recently loaded document in context
                // and extract a field by name.
                let dest = sanitize_name(store_as);
                self.emit(
                    "EXTRACT",
                    map(vec![
                        ("source", string_value("document")),
                        ("field", string_value(&sanitize_name(target))),
                        ("dest", string_value(&dest)),
                    ]),
                    Some(statement),
                );
            }
            StatementKind::DocumentRequirement { verb, target } => {
                self.emit(
                    "REQUIRE_DOC",
                    map(vec![
                        ("target", string_value(&sanitize_name(target))),
                        ("verb", string_value(verb)),
                    ]),
                    Some(statement),
                );
            }
            StatementKind::Route {
                source,
                destination,
            } => {
                let source_register = self.compile_expression(source, statement);
                let dest_register = self.compile_expression(destination, statement);
                self.emit(
                    "ROUTE",
                    map(vec![
                        ("source", string_value(&source_register)),
                        ("dest", string_value(&dest_register)),
                    ]),
                    Some(statement),
                );
                self.record_effect("route", statement, vec![]);
            }
            StatementKind::Checkpoint {
                prompt,
                context_key,
            } => {
                let prompt_register = self.compile_expression(prompt, statement);
                let mut operands = vec![("prompt", string_value(&prompt_register))];
                if let Some(key) = context_key {
                    operands.push(("context_key", string_value(key)));
                }
                self.emit("CHECKPOINT", map(operands), Some(statement));
                self.record_effect("checkpoint", statement, vec![]);
            }
            StatementKind::Import { .. } => {
                // Import statements are resolved at compile time by
                // resolve_imports() before bytecode emission. If we reach
                // here, the import was already inlined. No-op.
            }
            StatementKind::UseModule { .. } => {
                // Use statements are resolved at compile time by
                // resolve_imports() before bytecode emission. No-op.
            }
            StatementKind::Trigger { .. } => {
                // Metadata only, no bytecode emitted
            }
            StatementKind::RespondWith { value } => {
                let val_register = self.compile_expression(value, statement);
                self.emit(
                    "RESPOND",
                    map(vec![("value", string_value(&val_register))]),
                    Some(statement),
                );
            }
        }
    }

    fn compile_set_field_inner(
        &mut self,
        target: &Expression,
        value: &Expression,
        statement: &Statement,
    ) {
        let val_register = self.compile_expression(value, statement);
        match target {
            Expression::FieldAccess { .. } => {
                if let Some((root, fields)) = field_path(target) {
                    self.register_symbol(&root);
                    self.emit(
                        "FIELD_SET_PATH",
                        map(vec![
                            ("root", string_value(&root)),
                            (
                                "path",
                                Value::Array(
                                    fields.iter().map(|field| string_value(field)).collect(),
                                ),
                            ),
                            ("value", string_value(&val_register)),
                        ]),
                        Some(statement),
                    );
                    return;
                }
                let target_register = self.compile_expression(target, statement);
                self.emit(
                    "FIELD_SET",
                    map(vec![
                        ("record", string_value(&target_register)),
                        ("field", string_value("value")),
                        ("value", string_value(&val_register)),
                    ]),
                    Some(statement),
                );
            }
            Expression::Variable(name) => {
                self.register_symbol(name);
                self.emit(
                    "STORE",
                    map(vec![
                        ("symbol", string_value(name)),
                        ("value", string_value(&val_register)),
                    ]),
                    Some(statement),
                );
            }
            _ => {
                // For other expression forms, try to compile as a store
                let target_register = self.compile_expression(target, statement);
                self.emit(
                    "STORE",
                    map(vec![
                        ("symbol", string_value(&target_register)),
                        ("value", string_value(&val_register)),
                    ]),
                    Some(statement),
                );
            }
        }
    }

    fn compile_expression(&mut self, expression: &Expression, statement: &Statement) -> String {
        match expression {
            Expression::Literal(value) => {
                let register = self.next_register();
                let constant_index = self.add_constant(value.clone());
                self.emit(
                    "CONST",
                    map(vec![
                        ("dest", string_value(&register)),
                        ("const", number_value(constant_index as i64)),
                    ]),
                    Some(statement),
                );
                register
            }
            Expression::Variable(name) => {
                let register = self.next_register();
                self.register_symbol(name);
                self.emit(
                    "LOAD",
                    map(vec![
                        ("dest", string_value(&register)),
                        ("symbol", string_value(name)),
                    ]),
                    Some(statement),
                );
                register
            }
            Expression::Binary {
                operator,
                left,
                right,
            } => {
                let left_register = self.compile_expression(left, statement);
                let right_register = self.compile_expression(right, statement);
                let register = self.next_register();
                self.emit(
                    operator.opcode(),
                    map(vec![
                        ("dest", string_value(&register)),
                        ("left", string_value(&left_register)),
                        ("right", string_value(&right_register)),
                    ]),
                    Some(statement),
                );
                register
            }
            Expression::Comparison {
                operator,
                left,
                right,
            } => {
                let left_register = self.compile_expression(left, statement);
                let right_register = self.compile_expression(right, statement);
                let register = self.next_register();
                self.emit(
                    operator.opcode(),
                    map(vec![
                        ("dest", string_value(&register)),
                        ("left", string_value(&left_register)),
                        ("right", string_value(&right_register)),
                    ]),
                    Some(statement),
                );
                register
            }
            Expression::LogicalAnd { left, right } => {
                let left_register = self.compile_expression(left, statement);
                let right_register = self.compile_expression(right, statement);
                let register = self.next_register();
                self.emit(
                    "AND",
                    map(vec![
                        ("dest", string_value(&register)),
                        ("left", string_value(&left_register)),
                        ("right", string_value(&right_register)),
                    ]),
                    Some(statement),
                );
                register
            }
            Expression::LogicalOr { left, right } => {
                let left_register = self.compile_expression(left, statement);
                let right_register = self.compile_expression(right, statement);
                let register = self.next_register();
                self.emit(
                    "OR",
                    map(vec![
                        ("dest", string_value(&register)),
                        ("left", string_value(&left_register)),
                        ("right", string_value(&right_register)),
                    ]),
                    Some(statement),
                );
                register
            }
            Expression::LogicalNot { operand } => {
                let operand_register = self.compile_expression(operand, statement);
                let register = self.next_register();
                self.emit(
                    "NOT",
                    map(vec![
                        ("dest", string_value(&register)),
                        ("operand", string_value(&operand_register)),
                    ]),
                    Some(statement),
                );
                register
            }
            Expression::ListLiteral(items) => {
                let item_registers: Vec<String> = items
                    .iter()
                    .map(|item| self.compile_expression(item, statement))
                    .collect();
                let register = self.next_register();
                let items_value =
                    Value::Array(item_registers.iter().map(|r| string_value(r)).collect());
                self.emit(
                    "LIST_BUILD",
                    map(vec![
                        ("dest", string_value(&register)),
                        ("items", items_value),
                    ]),
                    Some(statement),
                );
                register
            }
            Expression::RecordLiteral(fields) => {
                let mut keys = Vec::new();
                let mut value_registers = Vec::new();
                for (key, val) in fields {
                    keys.push(string_value(key));
                    value_registers.push(string_value(&self.compile_expression(val, statement)));
                }
                let register = self.next_register();
                self.emit(
                    "RECORD_BUILD",
                    map(vec![
                        ("dest", string_value(&register)),
                        ("keys", Value::Array(keys)),
                        ("values", Value::Array(value_registers)),
                    ]),
                    Some(statement),
                );
                register
            }
            Expression::FieldAccess { record, field } => {
                let rec_register = self.compile_expression(record, statement);
                let register = self.next_register();
                self.emit(
                    "FIELD_GET",
                    map(vec![
                        ("dest", string_value(&register)),
                        ("record", string_value(&rec_register)),
                        ("field", string_value(field)),
                    ]),
                    Some(statement),
                );
                register
            }
            Expression::BuiltinCall { name, arguments } => {
                let arg_registers: Vec<String> = arguments
                    .iter()
                    .map(|arg| self.compile_expression(arg, statement))
                    .collect();
                let register = self.next_register();
                let args_value =
                    Value::Array(arg_registers.iter().map(|r| string_value(r)).collect());
                self.emit(
                    "CALL_BUILTIN",
                    map(vec![
                        ("dest", string_value(&register)),
                        ("name", string_value(name)),
                        ("args", args_value),
                    ]),
                    Some(statement),
                );
                register
            }
            Expression::Contains { left, right } => {
                let left_register = self.compile_expression(left, statement);
                let right_register = self.compile_expression(right, statement);
                let register = self.next_register();
                self.emit(
                    "STR_CONTAINS",
                    map(vec![
                        ("dest", string_value(&register)),
                        ("left", string_value(&left_register)),
                        ("right", string_value(&right_register)),
                    ]),
                    Some(statement),
                );
                register
            }
            Expression::StartsWith { left, right } => {
                let left_register = self.compile_expression(left, statement);
                let right_register = self.compile_expression(right, statement);
                let register = self.next_register();
                self.emit(
                    "STR_STARTS_WITH",
                    map(vec![
                        ("dest", string_value(&register)),
                        ("left", string_value(&left_register)),
                        ("right", string_value(&right_register)),
                    ]),
                    Some(statement),
                );
                register
            }
            Expression::EndsWith { left, right } => {
                let left_register = self.compile_expression(left, statement);
                let right_register = self.compile_expression(right, statement);
                let register = self.next_register();
                self.emit(
                    "STR_ENDS_WITH",
                    map(vec![
                        ("dest", string_value(&register)),
                        ("left", string_value(&left_register)),
                        ("right", string_value(&right_register)),
                    ]),
                    Some(statement),
                );
                register
            }
            Expression::IsMissing(inner) => {
                let inner_register = self.compile_expression(inner, statement);
                let null_register = self.next_register();
                let null_const = self.add_constant(Value::Null);
                self.emit(
                    "CONST",
                    map(vec![
                        ("dest", string_value(&null_register)),
                        ("const", number_value(null_const as i64)),
                    ]),
                    Some(statement),
                );
                let register = self.next_register();
                self.emit(
                    "EQ",
                    map(vec![
                        ("dest", string_value(&register)),
                        ("left", string_value(&inner_register)),
                        ("right", string_value(&null_register)),
                    ]),
                    Some(statement),
                );
                register
            }
            Expression::IsIn { value, collection } => {
                let val_register = self.compile_expression(value, statement);
                let coll_register = self.compile_expression(collection, statement);
                let register = self.next_register();
                self.emit(
                    "LIST_CONTAINS",
                    map(vec![
                        ("dest", string_value(&register)),
                        ("list", string_value(&coll_register)),
                        ("value", string_value(&val_register)),
                    ]),
                    Some(statement),
                );
                register
            }
            Expression::MethodCall { name, arguments } => {
                if let Some(method) = self.methods.get(name).cloned() {
                    return self.compile_inline_method_call(&method, arguments, statement);
                }
                let arg_registers: Vec<String> = arguments
                    .iter()
                    .map(|arg| self.compile_expression(arg, statement))
                    .collect();
                let register = self.next_register();
                let args_value =
                    Value::Array(arg_registers.iter().map(|r| string_value(r)).collect());
                self.emit(
                    "CALL_METHOD",
                    map(vec![
                        ("dest", string_value(&register)),
                        ("name", string_value(name)),
                        ("args", args_value),
                    ]),
                    Some(statement),
                );
                register
            }
            Expression::QualifiedRef { module, name } => {
                // resolve_qualified_refs rewrites every QualifiedRef before
                // bytecode emission; reaching one here is a compiler bug.
                unreachable!("unresolved qualified reference {module}'s {name} at bytecode emission")
            }
            Expression::Comprehension {
                kind,
                list,
                binding,
                accumulator,
                body,
            } => self.compile_comprehension(*kind, list, binding, accumulator.as_ref(), body, statement),
        }
    }

    /// Compiles a call to a sibling class method by inlining its body at the
    /// call site (DEVL-132). The method's params and locals are alpha-renamed
    /// with a per-site prefix so repeated or nested inlining cannot clobber
    /// caller variables. Cycles are rejected before compilation
    /// (`reject_recursive_methods`), so nested inlining terminates.
    fn compile_inline_method_call(
        &mut self,
        method: &MethodDef,
        arguments: &[Expression],
        statement: &Statement,
    ) -> String {
        let uid = self.inline_counter;
        self.inline_counter += 1;
        let prefix = format!("__call_{uid}_");

        let mut locals: HashSet<String> = method.params.iter().cloned().collect();
        for stmt in &method.body {
            collect_defined_symbols(stmt, &mut locals);
        }
        let rename: HashMap<String, String> = locals
            .iter()
            .map(|name| (name.clone(), format!("{prefix}{name}")))
            .collect();

        // Bind arguments to the renamed parameters. Missing arguments are
        // null, mirroring how unassigned symbols read as null everywhere.
        for (index, param) in method.params.iter().enumerate() {
            let value_register = match arguments.get(index) {
                Some(argument) => self.compile_expression(argument, statement),
                None => {
                    let null_const = self.add_constant(Value::Null);
                    let register = self.next_register();
                    self.emit(
                        "CONST",
                        map(vec![
                            ("dest", string_value(&register)),
                            ("const", number_value(null_const as i64)),
                        ]),
                        Some(statement),
                    );
                    register
                }
            };
            let symbol = format!("{prefix}{param}");
            self.register_symbol(&symbol);
            self.emit(
                "STORE",
                map(vec![
                    ("symbol", string_value(&symbol)),
                    ("value", string_value(&value_register)),
                ]),
                Some(statement),
            );
        }

        let mut body = method.body.clone();
        for stmt in &mut body {
            rename_symbols_in_statement(stmt, &rename);
        }
        for stmt in &body {
            self.compile_statement(stmt);
        }

        match &method.return_value {
            Some(return_value) => {
                let mut return_value = return_value.clone();
                rename_expression(&mut return_value, &rename);
                self.compile_expression(&return_value, statement)
            }
            None => {
                let null_const = self.add_constant(Value::Null);
                let register = self.next_register();
                self.emit(
                    "CONST",
                    map(vec![
                        ("dest", string_value(&register)),
                        ("const", number_value(null_const as i64)),
                    ]),
                    Some(statement),
                );
                register
            }
        }
    }

    /// Compiles a callback-style collection operation by inlining an index
    /// loop over the list (the same skeleton ForEach uses). The element is
    /// bound to `binding` each iteration and `body` is compiled in place, so
    /// arbitrary expressions (including method calls and nested helpers) work
    /// without any runtime function values or call frames (DEVL-132). Like
    /// ForEach's loop variable, the binding lives in the flat symbol
    /// namespace: an outer variable with the same name is overwritten.
    fn compile_comprehension(
        &mut self,
        kind: ComprehensionKind,
        list: &Expression,
        binding: &str,
        accumulator: Option<&(String, Box<Expression>)>,
        body: &Expression,
        statement: &Statement,
    ) -> String {
        let list_register = self.compile_expression(list, statement);
        let len_register = self.next_register();
        self.emit(
            "LIST_LEN",
            map(vec![
                ("dest", string_value(&len_register)),
                ("source", string_value(&list_register)),
            ]),
            Some(statement),
        );

        // Unique per comprehension: the instruction stream only grows, and
        // every comprehension emits instructions before the next one starts.
        let uid = self.instructions.len();
        let idx_symbol = format!("__comp_idx_{uid}");
        let result_symbol = format!("__comp_result_{uid}");
        self.register_symbol(&idx_symbol);

        // Initialize the accumulator/result before the loop.
        let acc_symbol = match kind {
            ComprehensionKind::Reduce => {
                let (name, init) = accumulator
                    .expect("reduce comprehension always carries an accumulator");
                self.register_symbol(name);
                let init_register = self.compile_expression(init, statement);
                self.emit(
                    "STORE",
                    map(vec![
                        ("symbol", string_value(name)),
                        ("value", string_value(&init_register)),
                    ]),
                    Some(statement),
                );
                name.clone()
            }
            ComprehensionKind::Map
            | ComprehensionKind::Filter
            | ComprehensionKind::Reject
            | ComprehensionKind::SortBy => {
                self.register_symbol(&result_symbol);
                let empty_register = self.next_register();
                self.emit(
                    "LIST_BUILD",
                    map(vec![
                        ("dest", string_value(&empty_register)),
                        ("items", Value::Array(Vec::new())),
                    ]),
                    Some(statement),
                );
                self.emit(
                    "STORE",
                    map(vec![
                        ("symbol", string_value(&result_symbol)),
                        ("value", string_value(&empty_register)),
                    ]),
                    Some(statement),
                );
                result_symbol.clone()
            }
            ComprehensionKind::Find | ComprehensionKind::Any | ComprehensionKind::All => {
                self.register_symbol(&result_symbol);
                let init_value = match kind {
                    ComprehensionKind::Find => Value::Null,
                    ComprehensionKind::Any => Value::Bool(false),
                    _ => Value::Bool(true),
                };
                let init_const = self.add_constant(init_value);
                let init_register = self.next_register();
                self.emit(
                    "CONST",
                    map(vec![
                        ("dest", string_value(&init_register)),
                        ("const", number_value(init_const as i64)),
                    ]),
                    Some(statement),
                );
                self.emit(
                    "STORE",
                    map(vec![
                        ("symbol", string_value(&result_symbol)),
                        ("value", string_value(&init_register)),
                    ]),
                    Some(statement),
                );
                result_symbol.clone()
            }
        };

        // idx = 0
        let zero_register = self.next_register();
        let zero_const = self.add_constant(number_value(0));
        self.emit(
            "CONST",
            map(vec![
                ("dest", string_value(&zero_register)),
                ("const", number_value(zero_const as i64)),
            ]),
            Some(statement),
        );
        self.emit(
            "STORE",
            map(vec![
                ("symbol", string_value(&idx_symbol)),
                ("value", string_value(&zero_register)),
            ]),
            Some(statement),
        );

        let loop_start = self.instructions.len();

        // idx < len, else exit
        let idx_load = self.next_register();
        self.emit(
            "LOAD",
            map(vec![
                ("dest", string_value(&idx_load)),
                ("symbol", string_value(&idx_symbol)),
            ]),
            Some(statement),
        );
        let cmp_register = self.next_register();
        self.emit(
            "LT",
            map(vec![
                ("dest", string_value(&cmp_register)),
                ("left", string_value(&idx_load)),
                ("right", string_value(&len_register)),
            ]),
            Some(statement),
        );
        let mut exit_patches = vec![self.emit(
            "JUMP_IF_FALSE",
            map(vec![
                ("condition", string_value(&cmp_register)),
                ("target", Value::Null),
            ]),
            Some(statement),
        )];

        // element = list[idx]; bind it
        self.register_symbol(binding);
        let idx_load2 = self.next_register();
        self.emit(
            "LOAD",
            map(vec![
                ("dest", string_value(&idx_load2)),
                ("symbol", string_value(&idx_symbol)),
            ]),
            Some(statement),
        );
        let item_register = self.next_register();
        self.emit(
            "LIST_GET",
            map(vec![
                ("dest", string_value(&item_register)),
                ("source", string_value(&list_register)),
                ("index", string_value(&idx_load2)),
            ]),
            Some(statement),
        );
        self.emit(
            "STORE",
            map(vec![
                ("symbol", string_value(binding)),
                ("value", string_value(&item_register)),
            ]),
            Some(statement),
        );

        // Per-element behavior. `skip_patches` jump to the increment section.
        let mut skip_patches = Vec::new();
        match kind {
            ComprehensionKind::Map | ComprehensionKind::SortBy => {
                let value_register = self.compile_expression(body, statement);
                self.emit(
                    "LIST_APPEND",
                    map(vec![
                        ("target", string_value(&acc_symbol)),
                        ("value", string_value(&value_register)),
                    ]),
                    Some(statement),
                );
            }
            ComprehensionKind::Filter | ComprehensionKind::Reject => {
                let mut cond_register = self.compile_expression(body, statement);
                if kind == ComprehensionKind::Reject {
                    let negated = self.next_register();
                    self.emit(
                        "NOT",
                        map(vec![
                            ("dest", string_value(&negated)),
                            ("operand", string_value(&cond_register)),
                        ]),
                        Some(statement),
                    );
                    cond_register = negated;
                }
                skip_patches.push(self.emit(
                    "JUMP_IF_FALSE",
                    map(vec![
                        ("condition", string_value(&cond_register)),
                        ("target", Value::Null),
                    ]),
                    Some(statement),
                ));
                self.emit(
                    "LIST_APPEND",
                    map(vec![
                        ("target", string_value(&acc_symbol)),
                        ("value", string_value(&item_register)),
                    ]),
                    Some(statement),
                );
            }
            ComprehensionKind::Find => {
                let cond_register = self.compile_expression(body, statement);
                skip_patches.push(self.emit(
                    "JUMP_IF_FALSE",
                    map(vec![
                        ("condition", string_value(&cond_register)),
                        ("target", Value::Null),
                    ]),
                    Some(statement),
                ));
                self.emit(
                    "STORE",
                    map(vec![
                        ("symbol", string_value(&acc_symbol)),
                        ("value", string_value(&item_register)),
                    ]),
                    Some(statement),
                );
                exit_patches.push(self.emit(
                    "JUMP",
                    map(vec![("target", Value::Null)]),
                    Some(statement),
                ));
            }
            ComprehensionKind::Any | ComprehensionKind::All => {
                let mut cond_register = self.compile_expression(body, statement);
                if kind == ComprehensionKind::All {
                    // Exit early when the predicate FAILS.
                    let negated = self.next_register();
                    self.emit(
                        "NOT",
                        map(vec![
                            ("dest", string_value(&negated)),
                            ("operand", string_value(&cond_register)),
                        ]),
                        Some(statement),
                    );
                    cond_register = negated;
                }
                skip_patches.push(self.emit(
                    "JUMP_IF_FALSE",
                    map(vec![
                        ("condition", string_value(&cond_register)),
                        ("target", Value::Null),
                    ]),
                    Some(statement),
                ));
                let outcome = self.add_constant(Value::Bool(kind == ComprehensionKind::Any));
                let outcome_register = self.next_register();
                self.emit(
                    "CONST",
                    map(vec![
                        ("dest", string_value(&outcome_register)),
                        ("const", number_value(outcome as i64)),
                    ]),
                    Some(statement),
                );
                self.emit(
                    "STORE",
                    map(vec![
                        ("symbol", string_value(&acc_symbol)),
                        ("value", string_value(&outcome_register)),
                    ]),
                    Some(statement),
                );
                exit_patches.push(self.emit(
                    "JUMP",
                    map(vec![("target", Value::Null)]),
                    Some(statement),
                ));
            }
            ComprehensionKind::Reduce => {
                let value_register = self.compile_expression(body, statement);
                self.emit(
                    "STORE",
                    map(vec![
                        ("symbol", string_value(&acc_symbol)),
                        ("value", string_value(&value_register)),
                    ]),
                    Some(statement),
                );
            }
        }

        // Increment index and loop.
        let increment_start = self.instructions.len();
        for patch in skip_patches {
            self.patch_target(patch, increment_start);
        }
        let idx_load3 = self.next_register();
        self.emit(
            "LOAD",
            map(vec![
                ("dest", string_value(&idx_load3)),
                ("symbol", string_value(&idx_symbol)),
            ]),
            Some(statement),
        );
        let one_register = self.next_register();
        let one_const = self.add_constant(number_value(1));
        self.emit(
            "CONST",
            map(vec![
                ("dest", string_value(&one_register)),
                ("const", number_value(one_const as i64)),
            ]),
            Some(statement),
        );
        let next_register = self.next_register();
        self.emit(
            "ADD",
            map(vec![
                ("dest", string_value(&next_register)),
                ("left", string_value(&idx_load3)),
                ("right", string_value(&one_register)),
            ]),
            Some(statement),
        );
        self.emit(
            "STORE",
            map(vec![
                ("symbol", string_value(&idx_symbol)),
                ("value", string_value(&next_register)),
            ]),
            Some(statement),
        );
        self.emit(
            "JUMP",
            map(vec![("target", number_value(loop_start as i64))]),
            Some(statement),
        );

        let exit = self.instructions.len();
        for patch in exit_patches {
            self.patch_target(patch, exit);
        }

        // Produce the final value.
        match kind {
            ComprehensionKind::SortBy => {
                let keys_register = self.next_register();
                self.emit(
                    "LOAD",
                    map(vec![
                        ("dest", string_value(&keys_register)),
                        ("symbol", string_value(&acc_symbol)),
                    ]),
                    Some(statement),
                );
                let register = self.next_register();
                self.emit(
                    "CALL_BUILTIN",
                    map(vec![
                        ("dest", string_value(&register)),
                        ("name", string_value("sort_by_keys")),
                        (
                            "args",
                            Value::Array(vec![
                                string_value(&list_register),
                                string_value(&keys_register),
                            ]),
                        ),
                    ]),
                    Some(statement),
                );
                register
            }
            _ => {
                let register = self.next_register();
                self.emit(
                    "LOAD",
                    map(vec![
                        ("dest", string_value(&register)),
                        ("symbol", string_value(&acc_symbol)),
                    ]),
                    Some(statement),
                );
                register
            }
        }
    }

    fn emit(
        &mut self,
        op: &str,
        mut operands: Map<String, Value>,
        source: Option<&Statement>,
    ) -> usize {
        let address = self.instructions.len();
        let mut instruction = Map::new();
        instruction.insert("op".to_string(), string_value(op));
        instruction.append(&mut operands);
        self.instructions.push(Value::Object(instruction));

        let mut source_entry = Map::new();
        source_entry.insert("address".to_string(), number_value(address as i64));
        if let Some(statement) = source {
            source_entry.insert("line".to_string(), number_value(statement.line as i64));
            source_entry.insert(
                "source_text".to_string(),
                string_value(&statement.source_text),
            );
        }
        self.source_map.push(Value::Object(source_entry));
        address
    }

    fn patch_target(&mut self, address: usize, target: usize) {
        if let Some(Value::Object(instruction)) = self.instructions.get_mut(address) {
            instruction.insert("target".to_string(), number_value(target as i64));
        }
    }

    fn patch_handler(&mut self, address: usize, target: usize) {
        if let Some(Value::Object(instruction)) = self.instructions.get_mut(address) {
            instruction.insert("handler".to_string(), number_value(target as i64));
        }
    }

    fn record_effect(&mut self, kind: &str, statement: &Statement, fields: Vec<(&str, Value)>) {
        let mut effect = Map::new();
        effect.insert("kind".to_string(), string_value(kind));
        effect.insert("line".to_string(), number_value(statement.line as i64));
        effect.insert(
            "source_text".to_string(),
            string_value(&statement.source_text),
        );
        for (key, value) in fields {
            effect.insert(key.to_string(), value);
        }
        self.effects.push(Value::Object(effect));
    }

    fn add_constant(&mut self, value: Value) -> usize {
        let key = serde_json::to_string(&value).unwrap_or_default();
        if let Some(index) = self.constant_index.get(&key) {
            return *index;
        }
        let index = self.constants.len();
        self.constants.push(value);
        self.constant_index.insert(key, index);
        index
    }

    fn register_symbol(&mut self, name: &str) -> usize {
        if let Some(index) = self.symbol_index.get(name) {
            return *index;
        }
        let index = self.symbols.len();
        self.symbols.push(name.to_string());
        self.symbol_index.insert(name.to_string(), index);
        index
    }

    fn next_register(&mut self) -> String {
        let register = format!("r{}", self.next_register);
        self.next_register += 1;
        register
    }

    fn imports(&self) -> Vec<String> {
        let mut imports = vec!["emit_event".to_string()];
        if self.effects.iter().any(|effect| {
            effect
                .get("kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind == "input")
        }) {
            imports.push("request_input".to_string());
        }
        if self.effects.iter().any(|effect| {
            effect
                .get("kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind == "file_write")
        }) {
            imports.push("write_file".to_string());
        }
        if self.effects.iter().any(|effect| {
            effect
                .get("kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| kind == "file_read")
        }) {
            imports.push("read_file".to_string());
        }
        imports
    }
}

/// Collects the ruby_names of sibling methods a method's body and return
/// value call, for recursion detection before inlining.
fn collect_sibling_calls(
    method: &MethodDef,
    siblings: &HashSet<String>,
    out: &mut HashSet<String>,
) {
    fn walk_expr(expr: &Expression, siblings: &HashSet<String>, out: &mut HashSet<String>) {
        // The only expression walker is the mutable one; clone to reuse it.
        let mut clone = expr.clone();
        walk_expression_mut(&mut clone, &mut |node| {
            if let Expression::MethodCall { name, .. } = node {
                if siblings.contains(name.as_str()) {
                    out.insert(name.clone());
                }
            }
        });
    }
    fn walk_stmts(statements: &[Statement], siblings: &HashSet<String>, out: &mut HashSet<String>) {
        for statement in statements {
            each_expression_in_statement(statement, &mut |expr| walk_expr(expr, siblings, out));
            for block in child_statement_blocks(statement) {
                walk_stmts(block, siblings, out);
            }
        }
    }
    walk_stmts(&method.body, siblings, out);
    if let Some(return_value) = &method.return_value {
        walk_expr(return_value, siblings, out);
    }
}

/// Method calls compile by inlining the callee's body (DEVL-132), so a
/// recursive call chain would inline forever. Reject cycles loudly at
/// compile time.
fn reject_recursive_methods(class_program: &ClassProgram) -> Result<(), CompileError> {
    let sibling_names: HashSet<String> = class_program
        .methods
        .iter()
        .map(|m| m.ruby_name.clone())
        .collect();
    let mut calls: HashMap<String, HashSet<String>> = HashMap::new();
    for method in &class_program.methods {
        let mut called = HashSet::new();
        collect_sibling_calls(method, &sibling_names, &mut called);
        calls.insert(method.ruby_name.clone(), called);
    }
    fn reaches(
        from: &str,
        target: &str,
        calls: &HashMap<String, HashSet<String>>,
        visited: &mut HashSet<String>,
    ) -> bool {
        let Some(called) = calls.get(from) else {
            return false;
        };
        if called.contains(target) {
            return true;
        }
        called.iter().any(|next| {
            visited.insert(next.clone()) && reaches(next, target, calls, visited)
        })
    }
    for method in &class_program.methods {
        let mut visited = HashSet::new();
        if reaches(&method.ruby_name, &method.ruby_name, &calls, &mut visited) {
            return Err(CompileError::single(
                method.line,
                format!(
                    "Method '{}' calls itself (directly or through other methods). \
                     Method calls are inlined at compile time, so recursion is not \
                     supported.",
                    method.name
                ),
                &method.source_text,
            ));
        }
    }
    Ok(())
}

fn compile_class_program(
    options: CompileOptions,
    class_program: &ClassProgram,
    closure: SourceClosure,
) -> Result<BytecodePackage, CompileError> {
    reject_recursive_methods(class_program)?;
    let sibling_methods: HashMap<String, MethodDef> = class_program
        .methods
        .iter()
        .map(|m| (m.ruby_name.clone(), m.clone()))
        .collect();
    // Build class_info metadata
    let mut class_info = Map::new();
    class_info.insert(
        "module".to_string(),
        string_value(&class_program.module_name),
    );
    class_info.insert("class".to_string(), string_value(&class_program.class_name));
    if let Some((parent_module, parent_class)) = &class_program.parent_class {
        let mut parent = Map::new();
        parent.insert("module".to_string(), string_value(parent_module));
        parent.insert("class".to_string(), string_value(parent_class));
        class_info.insert("parent".to_string(), Value::Object(parent));
    }

    // Compile each method into its own bytecode representation
    let mut method_entries: Vec<Value> = Vec::new();
    let mut all_constants: Vec<Value> = Vec::new();
    let mut all_symbols: Vec<String> = Vec::new();
    let mut all_instructions: Vec<Value> = Vec::new();
    let mut all_source_map: Vec<Value> = Vec::new();
    let mut all_effects: Vec<Value> = Vec::new();

    for method in &class_program.methods {
        let entry_point = all_instructions.len();

        // Compile the method body with a fresh compiler
        let mut compiler = BytecodeCompiler::new(options.clone());
        compiler.methods = sibling_methods.clone();

        // Pre-load params into symbol table
        for param in &method.params {
            compiler.register_symbol(param);
        }

        for stmt in &method.body {
            compiler.compile_statement(stmt);
        }

        // If there is a return value, compile it
        if let Some(return_expr) = &method.return_value {
            let dummy_stmt = Statement {
                line: method.line,
                source_text: method.source_text.clone(),
                kind: StatementKind::RespondWith {
                    value: return_expr.clone(),
                },
            };
            let val_reg = compiler.compile_expression(return_expr, &dummy_stmt);
            compiler.register_symbol("__return__");
            compiler.emit(
                "STORE",
                map(vec![
                    ("symbol", string_value("__return__")),
                    ("value", string_value(&val_reg)),
                ]),
                Some(&dummy_stmt),
            );
        }

        compiler.emit("RETURN", Map::new(), None);

        let mut method_meta = Map::new();
        method_meta.insert("name".to_string(), string_value(&method.name));
        method_meta.insert("ruby_name".to_string(), string_value(&method.ruby_name));
        method_meta.insert(
            "params".to_string(),
            Value::Array(method.params.iter().map(|p| string_value(p)).collect()),
        );
        method_meta.insert("is_private".to_string(), Value::Bool(method.is_private));
        method_meta.insert("entry_point".to_string(), number_value(entry_point as i64));
        method_meta.insert("line".to_string(), number_value(method.line as i64));

        // Offset instruction addresses in source map
        for sm in &compiler.source_map {
            let mut entry = sm.clone();
            if let Value::Object(ref mut obj) = entry {
                if let Some(Value::Number(addr)) = obj.get("address") {
                    let offset_addr = addr.as_i64().unwrap_or(0) + entry_point as i64;
                    obj.insert("address".to_string(), number_value(offset_addr));
                }
            }
            all_source_map.push(entry);
        }

        // Each method compiles with a fresh compiler, so its constant indices
        // and control-flow addresses are method-relative. Rebase both onto
        // the concatenated pools, or every method after the first reads the
        // wrong constants and jumps into the wrong method when run from its
        // entry_point.
        let const_base = all_constants.len();
        all_constants.extend(compiler.constants);
        all_symbols.extend(compiler.symbols);
        for mut instruction in compiler.instructions {
            if let Value::Object(ref mut fields) = instruction {
                let op = fields
                    .get("op")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let offset_field = |fields: &mut Map<String, Value>, key: &str, base: usize| {
                    if let Some(Value::Number(index)) = fields.get(key) {
                        let rebased = index.as_i64().unwrap_or(0) + base as i64;
                        fields.insert(key.to_string(), number_value(rebased));
                    }
                };
                match op.as_str() {
                    "CONST" => offset_field(fields, "const", const_base),
                    "JUMP" | "JUMP_IF_FALSE" => offset_field(fields, "target", entry_point),
                    "TRY_BEGIN" => offset_field(fields, "handler", entry_point),
                    _ => {}
                }
            }
            all_instructions.push(instruction);
        }
        all_effects.extend(compiler.effects);
        method_entries.push(Value::Object(method_meta));
    }

    // Deduplicate symbols
    let mut unique_symbols: Vec<String> = Vec::new();
    for sym in all_symbols {
        if !unique_symbols.contains(&sym) {
            unique_symbols.push(sym);
        }
    }

    let stdlib = closure.stdlib_value();
    Ok(BytecodePackage {
        format: FORMAT,
        format_version: FORMAT_VERSION,
        compiler_version: COMPILER_VERSION,
        source_path: options.source_path,
        source_hash: closure.hash,
        source_files: closure.files,
        constant_pool: all_constants,
        symbol_table: unique_symbols,
        instructions: all_instructions,
        source_map: all_source_map,
        effect_table: all_effects,
        imports: vec!["emit_event".to_string()],
        class_info: Some(Value::Object(class_info)),
        methods: Some(method_entries),
        manifest: None,
        stdlib,
    })
}

/// Variable names can never legitimately contain the `equals` keyword: the
/// statement parser splits assignments at the first ` equals `, so a fallback
/// variable that still carries the token means the line was malformed
/// (e.g. `x equals equals 5`).
fn find_reserved_word_variable(expression: &Expression) -> Option<&str> {
    fn name_has_equals(name: &str) -> bool {
        name == "equals"
            || name.starts_with("equals_")
            || name.ends_with("_equals")
            || name.contains("_equals_")
    }
    match expression {
        Expression::Variable(name) => name_has_equals(name).then_some(name.as_str()),
        Expression::Literal(_) => None,
        Expression::Binary { left, right, .. }
        | Expression::Comparison { left, right, .. }
        | Expression::LogicalAnd { left, right }
        | Expression::LogicalOr { left, right }
        | Expression::Contains { left, right }
        | Expression::StartsWith { left, right }
        | Expression::EndsWith { left, right } => {
            find_reserved_word_variable(left).or_else(|| find_reserved_word_variable(right))
        }
        Expression::LogicalNot { operand } | Expression::IsMissing(operand) => {
            find_reserved_word_variable(operand)
        }
        Expression::IsIn { value, collection } => {
            find_reserved_word_variable(value).or_else(|| find_reserved_word_variable(collection))
        }
        Expression::ListLiteral(items) => items.iter().find_map(find_reserved_word_variable),
        Expression::RecordLiteral(fields) => fields
            .iter()
            .find_map(|(_, expr)| find_reserved_word_variable(expr)),
        Expression::FieldAccess { record, .. } => find_reserved_word_variable(record),
        Expression::BuiltinCall { arguments, .. } | Expression::MethodCall { arguments, .. } => {
            arguments.iter().find_map(find_reserved_word_variable)
        }
        Expression::Comprehension {
            list,
            accumulator,
            body,
            ..
        } => find_reserved_word_variable(list)
            .or_else(|| {
                accumulator
                    .as_ref()
                    .and_then(|(_, init)| find_reserved_word_variable(init))
            })
            .or_else(|| find_reserved_word_variable(body)),
        Expression::QualifiedRef { .. } => None,
    }
}

fn parse_expression(raw: &str) -> Expression {
    let value = raw.trim();

    // Boolean and nil literals
    let lower = value.to_ascii_lowercase();
    if lower == "true" || lower == "yes" {
        return Expression::Literal(Value::Bool(true));
    }
    if lower == "false" || lower == "no" {
        return Expression::Literal(Value::Bool(false));
    }
    if lower == "nil" || lower == "null" || lower == "nothing" {
        return Expression::Literal(Value::Null);
    }

    // list of X, Y, Z / list of X and Y and Z
    if let Some(rest) = strip_prefix_ci(value, "list of ") {
        let items = split_list_items(rest.trim());
        let exprs: Vec<Expression> = items.iter().map(|item| parse_expression(item)).collect();
        return Expression::ListLiteral(exprs);
    }

    // record with X as a, Y as b / record with X as a and Y as b
    if let Some(rest) = strip_prefix_ci(value, "record with ") {
        let items = split_list_items(rest.trim());
        let mut fields = Vec::new();
        for item in items {
            if let Some((val_text, key_text)) = split_once_ci(item.trim(), " as ") {
                fields.push((
                    sanitize_name(key_text.trim()),
                    parse_expression(val_text.trim()),
                ));
            }
        }
        return Expression::RecordLiteral(fields);
    }

    // Collection verbs claim the whole phrase before operator splitting, so a
    // callback body containing `times`/`plus`/... stays inside the verb:
    // `map xs to item times 2` is a map whose body multiplies, not a
    // multiplication whose left side is a map (DEVL-132).
    if [
        "reduce ", "map ", "filter ", "reject ", "find ", "any of ", "all of ", "sort ",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
        // `round <expr> to N decimal places` claims the whole phrase too, so
        // an arithmetic expression inside it is the rounding target
        // (DEVL-134). Plain `round x plus 1` still splits at the operator.
        || (lower.starts_with("round ") && lower.contains(" decimal place"))
    {
        if let Some(expr) = parse_builtin_call(value) {
            return expr;
        }
    }

    // Binary arithmetic expressions
    if let Some((left, operator, right)) = split_binary_expression(value) {
        return Expression::Binary {
            operator,
            left: Box::new(parse_expression(left.trim())),
            right: Box::new(parse_expression(right.trim())),
        };
    }

    // Quoted strings
    if let Some((value, rest)) = quoted_prefix(value) {
        if rest.trim().is_empty() {
            let value = value.replace("\\n", "\n").replace("\\t", "\t");
            return Expression::Literal(Value::String(value));
        }
    }

    // Integer literals
    if is_integer(value) {
        if let Ok(number) = value.parse::<i64>() {
            return Expression::Literal(number_value(number));
        }
    }
    // Decimal literals
    if is_decimal(value) {
        if let Ok(number) = value.parse::<f64>() {
            if let Some(number) = Number::from_f64(number) {
                return Expression::Literal(Value::Number(number));
            }
        }
    }

    // <X> squared / <X> cubed (DEVL-136)
    for (suffix, exponent) in [(" squared", 2), (" cubed", 3)] {
        if let Some(base) = strip_suffix_ci(value, suffix) {
            let base = base.trim();
            if !base.is_empty() {
                return Expression::Binary {
                    operator: BinaryOperator::Power,
                    left: Box::new(parse_expression(base)),
                    right: Box::new(Expression::Literal(number_value(exponent))),
                };
            }
        }
    }

    // Built-in function patterns (30+ patterns)
    if let Some(expr) = parse_builtin_call(value) {
        return expr;
    }

    // <field> of <record> (generic field access, LAST as fallback)
    // Must come after builtin calls since many builtins use "X of Y" patterns
    if let Some((field, record)) = split_once_ci(value, " of ") {
        let field_trimmed = field.trim();
        let record_trimmed = record.trim();
        // Only treat as field access if both parts look like identifiers
        if !field_trimmed.is_empty() && !record_trimmed.is_empty() && !field_trimmed.contains(' ') {
            return Expression::FieldAccess {
                record: Box::new(parse_expression(record_trimmed)),
                field: sanitize_name(field_trimmed),
            };
        }
    }

    // Method call: <name> using <arg1> [and <arg2> ...]
    if let Some((name_part, args_part)) = split_once_ci(value, " using ") {
        let name_trimmed = name_part.trim();
        // Only treat as method call if the name part looks like an identifier (no operators)
        if !name_trimmed.is_empty()
            && !name_trimmed.contains(" plus ")
            && !name_trimmed.contains(" minus ")
            && !name_trimmed.contains(" times ")
            && !name_trimmed.contains(" divided by ")
        {
            let arguments: Vec<Expression> = args_part
                .split(" and ")
                .map(|a| parse_expression(a.trim()))
                .collect();
            return Expression::MethodCall {
                name: sanitize_name(name_trimmed),
                arguments,
            };
        }
    }

    // Module-qualified possessive: `math's pi` / `statistics' mean`
    if let Some(expr) = parse_qualified_ref(value) {
        return expr;
    }

    if let Some(expr) = parse_dotted_field_access(value) {
        return expr;
    }

    Expression::Variable(sanitize_name(value))
}

/// Parses the TARGET of a `Set X to Y` statement. Write targets are names or
/// field paths, never module references (modules are read-only), so a
/// possessive in a plain name target folds into the name
/// (`Set salesperson's commission to 5` binds salesperson_commission) instead
/// of parsing as a qualified module reference (DEVL-131 follow-up).
fn parse_set_target(text: &str) -> Expression {
    let name_like = !text.is_empty()
        && text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == ' ' || ch == '_' || ch == '\'');
    if name_like && text.contains('\'') {
        return Expression::Variable(sanitize_name(text));
    }
    parse_expression(text)
}

/// Parses a possessive module qualification: `math's pi`, or the trailing
/// apostrophe form for module names ending in s (`statistics' mean`). The
/// module part must be a single identifier; the symbol part may be a
/// multi-word English name but not a nested expression.
fn parse_qualified_ref(value: &str) -> Option<Expression> {
    let (module_part, name_part) = if let Some(idx) = value.find("'s ") {
        (&value[..idx], &value[idx + 3..])
    } else if let Some(idx) = value.find("' ") {
        (&value[..idx], &value[idx + 2..])
    } else {
        return None;
    };
    let module_part = module_part.trim();
    let name_part = name_part.trim();
    if module_part.is_empty() || name_part.is_empty() || !is_identifier_text(module_part) {
        return None;
    }
    if !name_part
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == ' ' || ch == '_')
    {
        return None;
    }
    Some(Expression::QualifiedRef {
        module: sanitize_name(module_part),
        name: sanitize_name(name_part),
    })
}

fn parse_dotted_field_access(value: &str) -> Option<Expression> {
    let parts: Vec<&str> = value.split('.').map(str::trim).collect();
    if parts.len() < 2 {
        return None;
    }
    if parts.iter().any(|part| !is_identifier_text(part)) {
        return None;
    }

    let mut expr = Expression::Variable(sanitize_name(parts[0]));
    for field in parts.iter().skip(1) {
        expr = Expression::FieldAccess {
            record: Box::new(expr),
            field: sanitize_name(field),
        };
    }
    Some(expr)
}

fn is_identifier_text(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn field_path(expression: &Expression) -> Option<(String, Vec<String>)> {
    match expression {
        Expression::Variable(name) => Some((name.clone(), Vec::new())),
        Expression::FieldAccess { record, field } => {
            let (root, mut fields) = field_path(record)?;
            fields.push(field.clone());
            Some((root, fields))
        }
        _ => None,
    }
}

fn expression_label(expression: &Expression) -> String {
    match expression {
        Expression::Variable(name) => name.clone(),
        Expression::FieldAccess { record, field } => {
            format!("{}.{}", expression_label(record), field)
        }
        Expression::BuiltinCall { name, .. } => name.clone(),
        Expression::Literal(_) => "value".to_string(),
        _ => "expression".to_string(),
    }
}

fn unary_builtin(name: &str, value: &str) -> Expression {
    Expression::BuiltinCall {
        name: name.to_string(),
        arguments: vec![parse_expression(value.trim())],
    }
}

fn field_builtin(name: &str, list: &str, field: &str) -> Expression {
    Expression::BuiltinCall {
        name: name.to_string(),
        arguments: vec![
            parse_expression(list.trim()),
            Expression::Literal(Value::String(sanitize_name(field.trim()))),
        ],
    }
}

fn predicate_builtin(
    name: &str,
    list: &str,
    field: &str,
    operator: &str,
    expected: Expression,
) -> Expression {
    Expression::BuiltinCall {
        name: name.to_string(),
        arguments: vec![
            parse_expression(list.trim()),
            Expression::Literal(Value::String(field.to_string())),
            Expression::Literal(Value::String(operator.to_string())),
            expected,
        ],
    }
}

/// Strips a trailing ` ignoring case` and reports it as the `i` regex flag.
fn strip_ignoring_case(value: &str) -> (&str, Option<Expression>) {
    match strip_suffix_ci(value.trim_end(), " ignoring case") {
        Some(rest) => (
            rest,
            Some(Expression::Literal(Value::String("i".to_string()))),
        ),
        None => (value, None),
    }
}

fn regex_builtin(name: &str, mut arguments: Vec<Expression>, flags: Option<Expression>) -> Expression {
    if let Some(flags) = flags {
        arguments.push(flags);
    }
    Expression::BuiltinCall {
        name: name.to_string(),
        arguments,
    }
}

fn parse_field_predicate(text: &str) -> Option<(String, String, Expression)> {
    // Compound conditions are general predicates, not field/operator/value
    // tuples; the caller compiles them as inline loops (DEVL-132).
    if split_once_ci_outside_quotes(text, " and ").is_some()
        || split_once_ci_outside_quotes(text, " or ").is_some()
    {
        return None;
    }
    for (needle, operator) in [
        (" is greater than or equal to ", "gte"),
        (" is less than or equal to ", "lte"),
        (" is greater than ", "gt"),
        (" is less than ", "lt"),
        (" is at least ", "gte"),
        (" is at most ", "lte"),
        (" greater than or equal to ", "gte"),
        (" less than or equal to ", "lte"),
        (" not equals ", "neq"),
        (" equals ", "eq"),
        (" is not ", "neq"),
        (" is ", "eq"),
        (" contains ", "contains"),
        (">=", "gte"),
        ("<=", "lte"),
        ("!=", "neq"),
        ("==", "eq"),
        (">", "gt"),
        ("<", "lt"),
    ] {
        if let Some((field, expected)) = split_once_ci(text, needle) {
            // Only a plain field name qualifies for the field/operator/value
            // fast path. Anything else (arithmetic, and/or chains, nested
            // expressions) is a general predicate the caller compiles as an
            // inline comprehension loop instead (DEVL-132).
            let raw_field = field.trim();
            if !raw_field
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == ' ' || ch == '_')
            {
                return None;
            }
            // `total of item > 100` is field access on the element and
            // `item times 2 > 10` is arithmetic on it, not fields literally
            // named "total of item" / "times 2".
            if split_once_ci(raw_field, " of ").is_some()
                || split_binary_expression(raw_field).is_some()
            {
                return None;
            }
            let field = sanitize_name(raw_field.trim_start_matches("item "));
            if !field.is_empty() {
                return Some((
                    field,
                    operator.to_string(),
                    parse_expression(expected.trim()),
                ));
            }
        }
    }
    None
}

/// Builds a callback comprehension for a `<verb> <list> where <predicate>`
/// phrasing whose predicate is a general condition over `item`.
fn where_comprehension(kind: ComprehensionKind, list: &str, predicate: &str) -> Expression {
    Expression::Comprehension {
        kind,
        list: Box::new(parse_expression(list.trim())),
        binding: "item".to_string(),
        accumulator: None,
        body: Box::new(parse_condition_expression(predicate.trim())),
    }
}

/// Builds a callback comprehension for a `<verb> <list> using <helper>`
/// phrasing: the named helper method is called with each element.
fn helper_comprehension(kind: ComprehensionKind, list: &str, helper: &str) -> Option<Expression> {
    let helper = sanitize_name(helper.trim());
    if !is_identifier_text(&helper) {
        return None;
    }
    Some(Expression::Comprehension {
        kind,
        list: Box::new(parse_expression(list.trim())),
        binding: "item".to_string(),
        accumulator: None,
        body: Box::new(Expression::MethodCall {
            name: helper,
            arguments: vec![Expression::Variable("item".to_string())],
        }),
    })
}

fn parse_builtin_call(value: &str) -> Option<Expression> {
    // find/filter/reject/any/all <list> where <field> <operator> <value>
    for (prefix, builtin, kind) in [
        ("find ", "find_where", ComprehensionKind::Find),
        ("filter ", "filter_where", ComprehensionKind::Filter),
        ("reject ", "reject_where", ComprehensionKind::Reject),
        ("any of ", "any_where", ComprehensionKind::Any),
        ("all of ", "all_where", ComprehensionKind::All),
    ] {
        if let Some(rest) = strip_prefix_ci(value, prefix) {
            if let Some((list, predicate)) = split_once_ci(rest.trim(), " where ") {
                if let Some((field, operator, expected)) = parse_field_predicate(predicate.trim())
                {
                    return Some(predicate_builtin(builtin, list, &field, &operator, expected));
                }
                // General predicate over `item`, compiled as an inline loop
                // (DEVL-132): `filter xs where item times 2 > 10`,
                // `find xs where amount of item > cap and status of item is "open"`.
                return Some(where_comprehension(kind, list, predicate));
            }
            if let Some((list, helper)) = split_once_ci(rest.trim(), " using ") {
                if let Some(expr) = helper_comprehension(kind, list, helper) {
                    return Some(expr);
                }
            }
            // Bare `any of flags` / `all of flags`: each element's own
            // truthiness is the predicate.
            if matches!(kind, ComprehensionKind::Any | ComprehensionKind::All) {
                return Some(Expression::Comprehension {
                    kind,
                    list: Box::new(parse_expression(rest.trim())),
                    binding: "item".to_string(),
                    accumulator: None,
                    body: Box::new(Expression::Variable("item".to_string())),
                });
            }
        }
    }
    if let Some(rest) = strip_prefix_ci(value, "map ") {
        if let Some((list, transform)) = split_once_ci(rest.trim(), " to ") {
            let transform = transform.trim();
            for (phrase, operation) in [
                ("trim item", "trim"),
                ("uppercase item", "uppercase"),
                ("lowercase item", "lowercase"),
                ("normalize whitespace of item", "normalize_whitespace"),
                ("normalize whitespace item", "normalize_whitespace"),
            ] {
                if transform.eq_ignore_ascii_case(phrase) {
                    return Some(Expression::BuiltinCall {
                        name: "map_transform".to_string(),
                        arguments: vec![
                            parse_expression(list.trim()),
                            Expression::Literal(Value::String(operation.to_string())),
                        ],
                    });
                }
            }
            if !transform.contains(' ') && !transform.eq_ignore_ascii_case("item") {
                return Some(Expression::BuiltinCall {
                    name: "pluck".to_string(),
                    arguments: vec![
                        parse_expression(list.trim()),
                        Expression::Literal(Value::String(sanitize_name(transform))),
                    ],
                });
            }
            // General per-element transform, compiled as an inline loop
            // (DEVL-132): `map xs to item times 2`,
            // `map invoices to amount of item times rate`.
            return Some(Expression::Comprehension {
                kind: ComprehensionKind::Map,
                list: Box::new(parse_expression(list.trim())),
                binding: "item".to_string(),
                accumulator: None,
                body: Box::new(parse_expression(transform)),
            });
        }
        if let Some((list, helper)) = split_once_ci(rest.trim(), " using ") {
            if let Some(expr) = helper_comprehension(ComprehensionKind::Map, list, helper) {
                return Some(expr);
            }
        }
    }
    if let Some(rest) = strip_prefix_ci(value, "reduce ") {
        if let Some((list, after_starting)) = split_once_ci(rest.trim(), " starting at ") {
            if let Some((init, after_with)) = split_once_ci(after_starting.trim(), " with ") {
                if let Some((accum_and_item, transform)) = split_once_ci(after_with.trim(), " to ")
                {
                    let accum = accum_and_item
                        .split(" and ")
                        .next()
                        .unwrap_or("total")
                        .trim();
                    if transform
                        .trim()
                        .eq_ignore_ascii_case(&format!("{accum} plus 1"))
                    {
                        return Some(Expression::BuiltinCall {
                            name: "reduce_count".to_string(),
                            arguments: vec![
                                parse_expression(list.trim()),
                                parse_expression(init.trim()),
                            ],
                        });
                    }
                    // General reduce, compiled as an inline loop (DEVL-132):
                    // `reduce xs starting at 0 with total and item to total plus item`.
                    let parts: Vec<&str> = accum_and_item.split(" and ").collect();
                    let accum = sanitize_name(parts.first().unwrap_or(&"total").trim());
                    let item = sanitize_name(parts.get(1).unwrap_or(&"item").trim());
                    return Some(Expression::Comprehension {
                        kind: ComprehensionKind::Reduce,
                        list: Box::new(parse_expression(list.trim())),
                        binding: item,
                        accumulator: Some((
                            accum,
                            Box::new(parse_expression(init.trim())),
                        )),
                        body: Box::new(parse_expression(transform.trim())),
                    });
                }
            }
        }
    }

    // group/index/partition helpers over records
    if let Some(rest) = strip_prefix_ci(value, "group ") {
        if let Some((list, field)) = split_once_ci(rest.trim(), " by ") {
            return Some(field_builtin("group_by", list, field));
        }
    }
    if let Some(rest) = strip_prefix_ci(value, "index ") {
        if let Some((list, field)) = split_once_ci(rest.trim(), " by ") {
            return Some(field_builtin("index_by", list, field));
        }
    }
    if let Some(rest) = strip_prefix_ci(value, "partition ") {
        if let Some((list, predicate)) = split_once_ci(rest.trim(), " where ") {
            if let Some((field, operator, expected)) = parse_field_predicate(predicate.trim()) {
                return Some(predicate_builtin(
                    "partition_where",
                    list,
                    &field,
                    &operator,
                    expected,
                ));
            }
        }
    }

    // take/drop/zip/chunk and set-style list operations
    if let Some(rest) = strip_prefix_ci(value, "take ") {
        if let Some((count, list)) = split_once_ci(rest.trim(), " of ") {
            return Some(Expression::BuiltinCall {
                name: "take".to_string(),
                arguments: vec![
                    parse_expression(list.trim()),
                    parse_expression(count.trim()),
                ],
            });
        }
    }
    if let Some(rest) = strip_prefix_ci(value, "drop ") {
        if let Some((count, list)) = split_once_ci(rest.trim(), " of ") {
            return Some(Expression::BuiltinCall {
                name: "drop".to_string(),
                arguments: vec![
                    parse_expression(list.trim()),
                    parse_expression(count.trim()),
                ],
            });
        }
    }
    if let Some(rest) = strip_prefix_ci(value, "zip ") {
        if let Some((left, right)) = split_once_ci(rest.trim(), " with ") {
            return Some(Expression::BuiltinCall {
                name: "zip".to_string(),
                arguments: vec![
                    parse_expression(left.trim()),
                    parse_expression(right.trim()),
                ],
            });
        }
    }
    if let Some(rest) = strip_prefix_ci(value, "chunk ") {
        if let Some((list, size)) = split_once_ci(rest.trim(), " by ") {
            return Some(Expression::BuiltinCall {
                name: "chunk".to_string(),
                arguments: vec![parse_expression(list.trim()), parse_expression(size.trim())],
            });
        }
    }
    for (prefix, name) in [
        ("union of ", "union"),
        ("intersection of ", "intersection"),
        ("difference of ", "difference"),
    ] {
        if let Some(rest) = strip_prefix_ci(value, prefix) {
            if let Some((left, right)) = split_once_ci(rest.trim(), " and ") {
                return Some(Expression::BuiltinCall {
                    name: name.to_string(),
                    arguments: vec![
                        parse_expression(left.trim()),
                        parse_expression(right.trim()),
                    ],
                });
            }
        }
    }

    // text cleanup and search helpers
    if let Some(rest) = strip_prefix_ci(value, "normalize whitespace of ") {
        return Some(unary_builtin("normalize_whitespace", rest));
    }
    if let Some(rest) = strip_prefix_ci(value, "normalize whitespace ") {
        return Some(unary_builtin("normalize_whitespace", rest));
    }
    if let Some(rest) = strip_prefix_ci(value, "slugify ") {
        return Some(unary_builtin("slugify", rest));
    }
    if let Some(rest) = strip_prefix_ci(value, "title case of ") {
        return Some(unary_builtin("title_case", rest));
    }
    if let Some(rest) = strip_prefix_ci(value, "title case ") {
        return Some(unary_builtin("title_case", rest));
    }
    if let Some(rest) = strip_prefix_ci(value, "sentence case of ") {
        return Some(unary_builtin("sentence_case", rest));
    }
    if let Some(rest) = strip_prefix_ci(value, "sentence case ") {
        return Some(unary_builtin("sentence_case", rest));
    }
    if let Some(rest) = strip_prefix_ci(value, "words of ") {
        return Some(unary_builtin("words", rest));
    }
    if let Some(rest) = strip_prefix_ci(value, "tokens of ") {
        return Some(unary_builtin("words", rest));
    }
    if let Some(rest) = strip_prefix_ci(value, "contains ") {
        if let Some((needle, haystack)) = split_once_ci(rest.trim(), " in ") {
            return Some(Expression::BuiltinCall {
                name: "contains_text".to_string(),
                arguments: vec![
                    parse_expression(haystack.trim()),
                    parse_expression(needle.trim()),
                ],
            });
        }
    }
    if let Some(rest) = strip_prefix_ci(value, "starts with ") {
        if let Some((needle, haystack)) = split_once_ci(rest.trim(), " in ") {
            return Some(Expression::BuiltinCall {
                name: "starts_with_text".to_string(),
                arguments: vec![
                    parse_expression(haystack.trim()),
                    parse_expression(needle.trim()),
                ],
            });
        }
    }
    if let Some(rest) = strip_prefix_ci(value, "ends with ") {
        if let Some((needle, haystack)) = split_once_ci(rest.trim(), " in ") {
            return Some(Expression::BuiltinCall {
                name: "ends_with_text".to_string(),
                arguments: vec![
                    parse_expression(haystack.trim()),
                    parse_expression(needle.trim()),
                ],
            });
        }
    }

    // deterministic ISO date helpers
    if let Some(rest) = strip_prefix_ci(value, "date from ") {
        return Some(unary_builtin("date_parse", rest));
    }
    if let Some(rest) = strip_prefix_ci(value, "add ") {
        if let Some((days, date)) = split_once_ci(rest.trim(), " days to ") {
            return Some(Expression::BuiltinCall {
                name: "date_add_days".to_string(),
                arguments: vec![parse_expression(date.trim()), parse_expression(days.trim())],
            });
        }
    }
    if let Some(rest) = strip_prefix_ci(value, "days between ") {
        if let Some((start, end)) = split_once_ci(rest.trim(), " and ") {
            return Some(Expression::BuiltinCall {
                name: "days_between".to_string(),
                arguments: vec![parse_expression(start.trim()), parse_expression(end.trim())],
            });
        }
    }
    if let Some(rest) = strip_prefix_ci(value, "business days between ") {
        if let Some((start, end)) = split_once_ci(rest.trim(), " and ") {
            return Some(Expression::BuiltinCall {
                name: "business_days_between".to_string(),
                arguments: vec![parse_expression(start.trim()), parse_expression(end.trim())],
            });
        }
    }

    // count of X
    if let Some(rest) = strip_prefix_ci(value, "count of ") {
        return Some(Expression::BuiltinCall {
            name: "count".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    // first of X
    if let Some(rest) = strip_prefix_ci(value, "first of ") {
        return Some(Expression::BuiltinCall {
            name: "first".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    // last of X
    if let Some(rest) = strip_prefix_ci(value, "last of ") {
        return Some(Expression::BuiltinCall {
            name: "last".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    // unique of X
    if let Some(rest) = strip_prefix_ci(value, "unique of ") {
        return Some(Expression::BuiltinCall {
            name: "unique".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    // flatten X
    if let Some(rest) = strip_prefix_ci(value, "flatten ") {
        return Some(Expression::BuiltinCall {
            name: "flatten".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    // minimum/min of X
    if let Some(rest) = strip_prefix_ci(value, "minimum of ") {
        return Some(Expression::BuiltinCall {
            name: "minimum".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    if let Some(rest) = strip_prefix_ci(value, "min of ") {
        return Some(Expression::BuiltinCall {
            name: "minimum".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    // maximum/max of X
    if let Some(rest) = strip_prefix_ci(value, "maximum of ") {
        return Some(Expression::BuiltinCall {
            name: "maximum".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    if let Some(rest) = strip_prefix_ci(value, "max of ") {
        return Some(Expression::BuiltinCall {
            name: "maximum".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    // sum of X / average of X (previously fell through to field access and
    // silently evaluated to null; DEVL-134 makes them real and exact over
    // decimals/fractions)
    if let Some(rest) = strip_prefix_ci(value, "sum of ") {
        return Some(Expression::BuiltinCall {
            name: "sum".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    for prefix in ["average of ", "avg of "] {
        if let Some(rest) = strip_prefix_ci(value, prefix) {
            return Some(Expression::BuiltinCall {
                name: "average".to_string(),
                arguments: vec![parse_expression(rest.trim())],
            });
        }
    }
    // uppercase of X / uppercase X
    if let Some(rest) = strip_prefix_ci(value, "uppercase of ") {
        return Some(Expression::BuiltinCall {
            name: "uppercase".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    if let Some(rest) = strip_prefix_ci(value, "uppercase ") {
        return Some(Expression::BuiltinCall {
            name: "uppercase".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    // lowercase of X / lowercase X
    if let Some(rest) = strip_prefix_ci(value, "lowercase of ") {
        return Some(Expression::BuiltinCall {
            name: "lowercase".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    if let Some(rest) = strip_prefix_ci(value, "lowercase ") {
        return Some(Expression::BuiltinCall {
            name: "lowercase".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    // trim of X / trim X
    if let Some(rest) = strip_prefix_ci(value, "trim of ") {
        return Some(Expression::BuiltinCall {
            name: "trim".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    if let Some(rest) = strip_prefix_ci(value, "trim ") {
        return Some(Expression::BuiltinCall {
            name: "trim".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    // Numeric tower (DEVL-134).
    // decimal of X (dynamic conversion) / decimal <literal> (exact at compile
    // time, straight from the source digits so 19.99 means 19.99).
    if let Some(rest) = strip_prefix_ci(value, "decimal of ") {
        return Some(Expression::BuiltinCall {
            name: "to_decimal".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    if let Some(rest) = strip_prefix_ci(value, "decimal ") {
        let rest = rest.trim();
        let literal_text = if let Some((text, remainder)) = quoted_prefix(rest) {
            remainder.trim().is_empty().then_some(text)
        } else {
            (is_integer(rest) || is_decimal(rest)).then(|| rest.to_string())
        };
        if let Some(text) = literal_text {
            return Some(match devlish_vm::parse_decimal(&text) {
                Ok(tagged) => Expression::Literal(tagged),
                // Fold failed: emit the conversion call so the compile-time
                // literal validation pass reports the error with a line.
                Err(_) => Expression::BuiltinCall {
                    name: "to_decimal".to_string(),
                    arguments: vec![Expression::Literal(Value::String(text))],
                },
            });
        }
        return Some(Expression::BuiltinCall {
            name: "to_decimal".to_string(),
            arguments: vec![parse_expression(rest)],
        });
    }
    // fraction A over B (folded to an exact literal when both are integers)
    if let Some(rest) = strip_prefix_ci(value, "fraction ") {
        if let Some((numerator, denominator)) = split_once_ci_outside_quotes(rest.trim(), " over ")
        {
            let numerator = parse_expression(numerator.trim());
            let denominator = parse_expression(denominator.trim());
            if let (
                Expression::Literal(Value::Number(n)),
                Expression::Literal(Value::Number(d)),
            ) = (&numerator, &denominator)
            {
                if let (Some(n), Some(d)) = (n.as_i64(), d.as_i64()) {
                    if let Ok(tagged) = devlish_vm::fraction_json(n, d) {
                        return Some(Expression::Literal(tagged));
                    }
                }
            }
            return Some(Expression::BuiltinCall {
                name: "to_fraction".to_string(),
                arguments: vec![numerator, denominator],
            });
        }
    }
    // numeric value of X (explicit conversion to a plain number)
    if let Some(rest) = strip_prefix_ci(value, "numeric value of ") {
        return Some(Expression::BuiltinCall {
            name: "to_number".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    // round X to N decimal places [rounding <mode>] -> exact decimal rounding
    if let Some(rest) = strip_prefix_ci(value, "round ") {
        if let Some((target, after_to)) = split_once_ci_outside_quotes(rest.trim(), " to ") {
            let after_to = after_to.trim();
            for marker in [" decimal places", " decimal place"] {
                if let Some(index) = after_to.to_ascii_lowercase().find(marker) {
                    let places = &after_to[..index];
                    let tail = after_to[index + marker.len()..].trim();
                    let mode = strip_prefix_ci(tail, "rounding ").map(str::trim);
                    if !tail.is_empty() && mode.is_none() {
                        break;
                    }
                    let mut arguments = vec![
                        parse_expression(target.trim()),
                        parse_expression(places.trim()),
                    ];
                    if let Some(mode) = mode {
                        arguments.push(Expression::Literal(Value::String(mode.to_string())));
                    }
                    return Some(Expression::BuiltinCall {
                        name: "decimal_round".to_string(),
                        arguments,
                    });
                }
            }
        }
    }
    // round of X / round X
    if let Some(rest) = strip_prefix_ci(value, "round of ") {
        return Some(Expression::BuiltinCall {
            name: "round".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    if let Some(rest) = strip_prefix_ci(value, "round ") {
        return Some(Expression::BuiltinCall {
            name: "round".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    // absolute value of X
    if let Some(rest) = strip_prefix_ci(value, "absolute value of ") {
        return Some(Expression::BuiltinCall {
            name: "abs".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    // Regex helpers (DEVL-133). A trailing " ignoring case" adds the
    // case-insensitive flag; quoted patterns protect " in " / " with " /
    // " by " from splitting.
    // first match of P in T -> match record (text/start/end/groups/named)
    if let Some(rest) = strip_prefix_ci(value, "first match of ") {
        let (rest, flags) = strip_ignoring_case(rest);
        if let Some((pattern, text)) = split_once_ci_outside_quotes(rest.trim(), " in ") {
            return Some(regex_builtin(
                "regex_match",
                vec![
                    parse_expression(text.trim()),
                    parse_expression(pattern.trim()),
                ],
                flags,
            ));
        }
    }
    // all matches of P in T -> list of matched strings
    if let Some(rest) = strip_prefix_ci(value, "all matches of ") {
        let (rest, flags) = strip_ignoring_case(rest);
        if let Some((pattern, text)) = split_once_ci_outside_quotes(rest.trim(), " in ") {
            return Some(regex_builtin(
                "regex_find_all",
                vec![
                    parse_expression(text.trim()),
                    parse_expression(pattern.trim()),
                ],
                flags,
            ));
        }
    }
    // replace matches of P in T with R (must precede the literal replace arm)
    if let Some(rest) = strip_prefix_ci(value, "replace matches of ") {
        let (rest, flags) = strip_ignoring_case(rest);
        if let Some((pattern, after_in)) = split_once_ci_outside_quotes(rest.trim(), " in ") {
            if let Some((text, replacement)) =
                split_once_ci_outside_quotes(after_in.trim(), " with ")
            {
                return Some(regex_builtin(
                    "regex_replace",
                    vec![
                        parse_expression(text.trim()),
                        parse_expression(pattern.trim()),
                        parse_expression(replacement.trim()),
                    ],
                    flags,
                ));
            }
        }
    }
    // split T by pattern P (must precede the literal split arm)
    if let Some(rest) = strip_prefix_ci(value, "split ") {
        let (rest, flags) = strip_ignoring_case(rest);
        if let Some((text, pattern)) = split_once_ci_outside_quotes(rest.trim(), " by pattern ") {
            return Some(regex_builtin(
                "regex_split",
                vec![
                    parse_expression(text.trim()),
                    parse_expression(pattern.trim()),
                ],
                flags,
            ));
        }
    }
    // replace X in Y with Z
    if let Some(rest) = strip_prefix_ci(value, "replace ") {
        if let Some((needle, after_in)) = split_once_ci(rest.trim(), " in ") {
            if let Some((haystack, replacement)) = split_once_ci(after_in.trim(), " with ") {
                return Some(Expression::BuiltinCall {
                    name: "replace".to_string(),
                    arguments: vec![
                        parse_expression(haystack.trim()),
                        parse_expression(needle.trim()),
                        parse_expression(replacement.trim()),
                    ],
                });
            }
        }
    }
    // split X by Y
    if let Some(rest) = strip_prefix_ci(value, "split ") {
        if let Some((text, delimiter)) = split_once_ci(rest.trim(), " by ") {
            return Some(Expression::BuiltinCall {
                name: "split".to_string(),
                arguments: vec![
                    parse_expression(text.trim()),
                    parse_expression(delimiter.trim()),
                ],
            });
        }
    }
    // join X with Y
    if let Some(rest) = strip_prefix_ci(value, "join ") {
        if let Some((list, separator)) = split_once_ci(rest.trim(), " with ") {
            return Some(Expression::BuiltinCall {
                name: "join".to_string(),
                arguments: vec![
                    parse_expression(list.trim()),
                    parse_expression(separator.trim()),
                ],
            });
        }
    }
    // item N of X
    if let Some(rest) = strip_prefix_ci(value, "item ") {
        if let Some((index_text, list)) = split_once_ci(rest.trim(), " of ") {
            return Some(Expression::BuiltinCall {
                name: "item".to_string(),
                arguments: vec![
                    parse_expression(list.trim()),
                    parse_expression(index_text.trim()),
                ],
            });
        }
    }
    // slice X from N to M
    if let Some(rest) = strip_prefix_ci(value, "slice ") {
        if let Some((list, range)) = split_once_ci(rest.trim(), " from ") {
            if let Some((start, end)) = split_once_ci(range.trim(), " to ") {
                return Some(Expression::BuiltinCall {
                    name: "slice".to_string(),
                    arguments: vec![
                        parse_expression(list.trim()),
                        parse_expression(start.trim()),
                        parse_expression(end.trim()),
                    ],
                });
            }
        }
    }
    // keys of X
    if let Some(rest) = strip_prefix_ci(value, "keys of ") {
        return Some(Expression::BuiltinCall {
            name: "keys".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    // values of X
    if let Some(rest) = strip_prefix_ci(value, "values of ") {
        return Some(Expression::BuiltinCall {
            name: "values".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    // entries of X
    if let Some(rest) = strip_prefix_ci(value, "entries of ") {
        return Some(Expression::BuiltinCall {
            name: "entries".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }
    // sort X [by field-or-key-expression]
    if let Some(rest) = strip_prefix_ci(value, "sort ") {
        if let Some((list, key)) = split_once_ci(rest.trim(), " by ") {
            let key_expr = parse_expression(key.trim());
            if matches!(key_expr, Expression::Variable(_)) {
                // A plain name stays a record-field sort.
                return Some(Expression::BuiltinCall {
                    name: "sort".to_string(),
                    arguments: vec![
                        parse_expression(list.trim()),
                        Expression::Literal(Value::String(sanitize_name(key.trim()))),
                    ],
                });
            }
            // An expression key is computed per element with an inline loop,
            // then handed to sort_by_keys (DEVL-132):
            // `sort invoices by amount of item times quantity of item`.
            return Some(Expression::Comprehension {
                kind: ComprehensionKind::SortBy,
                list: Box::new(parse_expression(list.trim())),
                binding: "item".to_string(),
                accumulator: None,
                body: Box::new(key_expr),
            });
        }
        return Some(Expression::BuiltinCall {
            name: "sort".to_string(),
            arguments: vec![parse_expression(rest.trim())],
        });
    }

    None
}

fn parse_condition_expression(raw: &str) -> Expression {
    let value = raw.trim();

    // Split on ` or ` (lowest precedence)
    if let Some((left, right)) = split_once_ci_outside_quotes(value, " or ") {
        return Expression::LogicalOr {
            left: Box::new(parse_condition_expression(left.trim())),
            right: Box::new(parse_condition_expression(right.trim())),
        };
    }

    // Split on ` and `
    if let Some((left, right)) = split_once_ci_outside_quotes(value, " and ") {
        return Expression::LogicalAnd {
            left: Box::new(parse_condition_expression(left.trim())),
            right: Box::new(parse_condition_expression(right.trim())),
        };
    }

    // not <X>
    if let Some(rest) = strip_prefix_ci(value, "not ") {
        return Expression::LogicalNot {
            operand: Box::new(parse_condition_expression(rest.trim())),
        };
    }

    // <record> has fields <a, b, c>
    if let Some((record, fields_text)) = split_once_ci(value, " has fields ") {
        return Expression::BuiltinCall {
            name: "has_fields".to_string(),
            arguments: vec![
                parse_expression(record.trim()),
                field_names_expression(fields_text.trim()),
            ],
        };
    }

    // <record> has field <a>
    if let Some((record, field_text)) = split_once_ci(value, " has field ") {
        return Expression::BuiltinCall {
            name: "has_fields".to_string(),
            arguments: vec![
                parse_expression(record.trim()),
                field_names_expression(field_text.trim()),
            ],
        };
    }

    // <text> matches the pattern <regex> [ignoring case] (DEVL-133)
    for needle in [" matches the pattern ", " matches pattern "] {
        if let Some((text, pattern)) = split_once_ci_outside_quotes(value, needle) {
            let (pattern, flags) = strip_ignoring_case(pattern);
            return regex_builtin(
                "regex_test",
                vec![
                    parse_expression(text.trim()),
                    parse_expression(pattern.trim()),
                ],
                flags,
            );
        }
    }

    // <record> matches shape <shape_record>
    if let Some((record, shape)) = split_once_ci(value, " matches shape ") {
        return Expression::BuiltinCall {
            name: "matches_shape".to_string(),
            arguments: vec![
                parse_expression(record.trim()),
                parse_expression(shape.trim()),
            ],
        };
    }

    // <record> matches schema <schema_record>
    if let Some((record, schema)) = split_once_ci(value, " matches schema ") {
        return Expression::BuiltinCall {
            name: "matches_shape".to_string(),
            arguments: vec![
                parse_expression(record.trim()),
                parse_expression(schema.trim()),
            ],
        };
    }

    // <X> contains <Y>
    if let Some((left, right)) = split_once_ci(value, " contains ") {
        return Expression::Contains {
            left: Box::new(parse_expression(left.trim())),
            right: Box::new(parse_expression(right.trim())),
        };
    }

    // <X> starts with <Y>
    if let Some((left, right)) = split_once_ci(value, " starts with ") {
        return Expression::StartsWith {
            left: Box::new(parse_expression(left.trim())),
            right: Box::new(parse_expression(right.trim())),
        };
    }

    // <X> ends with <Y>
    if let Some((left, right)) = split_once_ci(value, " ends with ") {
        return Expression::EndsWith {
            left: Box::new(parse_expression(left.trim())),
            right: Box::new(parse_expression(right.trim())),
        };
    }

    // <X> is present  (the negation of "is missing"; mirrors the present/missing
    // pairing already used by validations and assertions)
    if let Some(target) = strip_suffix_ci(value, " is present") {
        return Expression::LogicalNot {
            operand: Box::new(Expression::IsMissing(Box::new(parse_expression(target.trim())))),
        };
    }

    // <X> is missing
    if let Some(target) = strip_suffix_ci(value, " is missing") {
        return Expression::IsMissing(Box::new(parse_expression(target.trim())));
    }

    // <X> is in <Y>
    if let Some((left, right)) = split_once_ci(value, " is in ") {
        return Expression::IsIn {
            value: Box::new(parse_expression(left.trim())),
            collection: Box::new(parse_expression(right.trim())),
        };
    }

    // <X> is not <Y>
    if let Some((left, right)) = split_once_ci(value, " is not ") {
        return Expression::Comparison {
            operator: ComparisonOperator::NotEquals,
            left: Box::new(parse_expression(left.trim())),
            right: Box::new(parse_expression(right.trim())),
        };
    }

    // Standard comparison operators.
    // Compound " is X" forms must come before bare " is " so that
    // "x is greater than 3" matches as x > 3, not x_is == "greater than 3".
    for (needle, operator) in [
        (
            " is greater than or equal to ",
            ComparisonOperator::GreaterOrEqual,
        ),
        (
            " is less than or equal to ",
            ComparisonOperator::LessOrEqual,
        ),
        (" is greater than ", ComparisonOperator::GreaterThan),
        (" is less than ", ComparisonOperator::LessThan),
        (" is at least ", ComparisonOperator::GreaterOrEqual),
        (" is at most ", ComparisonOperator::LessOrEqual),
        (
            " greater than or equal to ",
            ComparisonOperator::GreaterOrEqual,
        ),
        (" less than or equal to ", ComparisonOperator::LessOrEqual),
        (" not equals ", ComparisonOperator::NotEquals),
        (" greater than ", ComparisonOperator::GreaterThan),
        (" less than ", ComparisonOperator::LessThan),
        (" at least ", ComparisonOperator::GreaterOrEqual),
        (" at most ", ComparisonOperator::LessOrEqual),
        (" equals ", ComparisonOperator::Equals),
        (" is ", ComparisonOperator::Equals),
        (">=", ComparisonOperator::GreaterOrEqual),
        ("<=", ComparisonOperator::LessOrEqual),
        ("!=", ComparisonOperator::NotEquals),
        ("==", ComparisonOperator::Equals),
        (">", ComparisonOperator::GreaterThan),
        ("<", ComparisonOperator::LessThan),
    ] {
        if let Some((left, right)) = split_once_ci(value, needle) {
            return Expression::Comparison {
                operator,
                left: Box::new(parse_expression(left.trim())),
                right: Box::new(parse_expression(right.trim())),
            };
        }
    }

    parse_expression(value)
}

/// Like split_once_ci but avoids splitting inside quoted strings.
fn split_once_ci_outside_quotes<'a>(text: &'a str, needle: &str) -> Option<(&'a str, &'a str)> {
    let lower = text.to_ascii_lowercase();
    let needle_lower = needle.to_ascii_lowercase();
    let needle_len = needle.len();
    let mut in_quote = false;
    let bytes = text.as_bytes();

    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i] as char;
        if in_quote {
            if ch == '\\' {
                i += 2;
                continue;
            }
            if ch == '"' {
                in_quote = false;
            }
        } else {
            // Only double quotes delimit strings. Apostrophes are English
            // text (possessives, contractions) and never suppress operator
            // splitting (DEVL-127, DEVL-131).
            if ch == '"' {
                in_quote = true;
            } else if i + needle_len <= lower.len() && lower[i..i + needle_len] == needle_lower {
                return Some((&text[..i], &text[i + needle_len..]));
            }
        }
        i += 1;
    }
    None
}

fn split_binary_expression(value: &str) -> Option<(&str, BinaryOperator, &str)> {
    // Earlier needles split first and therefore bind loosest; exponentiation
    // sits last so it binds tighter than multiplication (DEVL-136).
    for (needle, operator) in [
        (" plus ", BinaryOperator::Add),
        (" minus ", BinaryOperator::Subtract),
        (" + ", BinaryOperator::Add),
        (" - ", BinaryOperator::Subtract),
        (" integer divided by ", BinaryOperator::IntDivide),
        (" divided by ", BinaryOperator::Divide),
        (" modulo ", BinaryOperator::Modulo),
        (" times ", BinaryOperator::Multiply),
        (" // ", BinaryOperator::IntDivide),
        (" / ", BinaryOperator::Divide),
        (" % ", BinaryOperator::Modulo),
        (" * ", BinaryOperator::Multiply),
        (" to the power of ", BinaryOperator::Power),
        (" ** ", BinaryOperator::Power),
        (" ^ ", BinaryOperator::Power),
    ] {
        // Skip operator substrings that fall inside quoted string literals, so
        // e.g. `name plus " / " plus tier` concatenates rather than being split
        // at the slash and parsed as division.
        if let Some((left, right)) = split_once_ci_outside_quotes(value, needle) {
            return Some((left, operator, right));
        }
    }
    None
}

fn split_assertion_id(text: &str) -> Option<(&str, String)> {
    let lower = text.to_ascii_lowercase();
    let mut search_start = 0usize;
    let mut found = None;
    while let Some(offset) = lower[search_start..].find(" as ") {
        found = Some(search_start + offset);
        search_start += offset + 4;
    }
    let index = found?;
    let body = text[..index].trim_end();
    let id_text = text[index + 4..].trim_start();
    let (assertion_id, rest) = quoted_prefix(id_text)?;
    if rest.trim().is_empty() {
        Some((body, assertion_id))
    } else {
        None
    }
}

fn leading_spaces(value: &str) -> usize {
    value.chars().take_while(|ch| *ch == ' ').count()
}

fn starts_with_lowercase_name(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_lowercase())
}

fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .map(|_| &text[prefix.len()..])
}

fn strip_suffix_ci<'a>(text: &'a str, suffix: &str) -> Option<&'a str> {
    if text.len() < suffix.len() {
        return None;
    }
    let start = text.len() - suffix.len();
    text.get(start..)
        .filter(|tail| tail.eq_ignore_ascii_case(suffix))
        .map(|_| &text[..start])
}

fn split_once_ci<'a>(text: &'a str, needle: &str) -> Option<(&'a str, &'a str)> {
    let lower = text.to_ascii_lowercase();
    let needle_lower = needle.to_ascii_lowercase();
    let index = lower.find(&needle_lower)?;
    Some((&text[..index], &text[index + needle.len()..]))
}

/// Splits a `Set` statement body into its target and value, absorbing the
/// filler that can sit between them (DEVL-126): `equal to`, `equals`, `to be`,
/// or a bare `to`. The earliest delimiter in the string wins so that a value
/// which itself contains `equals` (a comparison) still binds correctly; ties
/// at the same position are broken toward the longer delimiter so that
/// `... equal to ...` is not mistaken for a bare `to`.
fn split_set_target_value(text: &str) -> Option<(&str, &str)> {
    const DELIMITERS: &[&str] = &[" equal to ", " to be ", " equals ", " to "];
    let lower = text.to_ascii_lowercase();
    let mut best: Option<(usize, &str)> = None;
    for delimiter in DELIMITERS {
        if let Some(pos) = lower.find(delimiter) {
            let take = match best {
                None => true,
                Some((best_pos, best_delim)) => {
                    pos < best_pos || (pos == best_pos && delimiter.len() > best_delim.len())
                }
            };
            if take {
                best = Some((pos, delimiter));
            }
        }
    }
    let (pos, delimiter) = best?;
    Some((&text[..pos], &text[pos + delimiter.len()..]))
}

/// Returns the first `[` or `]` that appears outside of a double-quoted
/// string, or `None` when every bracket is inside quoted text. Used to reject
/// bracket tokens in expression position (DEVL-127) without flagging brackets
/// that are legitimately part of a string literal.
fn first_bracket_outside_quotes(text: &str) -> Option<char> {
    let mut in_quote = false;
    let mut escaped = false;
    for ch in text.chars() {
        if in_quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_quote = false;
            }
            continue;
        }
        match ch {
            // Only double quotes delimit strings; `'` is English text and
            // never disarms the bracket guard (DEVL-127, DEVL-131).
            '"' => in_quote = true,
            '[' | ']' => return Some(ch),
            _ => {}
        }
    }
    None
}

fn quoted_prefix(text: &str) -> Option<(String, &str)> {
    // String literals are double-quoted ONLY. A single quote is never a
    // string delimiter in Devlish: apostrophes are ordinary English text
    // (possessives, contractions), so `salesperson's commission` can never
    // silently swallow the rest of a line (DEVL-131 follow-up).
    let mut chars = text.char_indices();
    let (_, quote) = chars.next()?;
    if quote != '"' {
        return None;
    }

    let mut escaped = false;
    let mut value = String::new();
    for (index, ch) in chars {
        if escaped {
            value.push(match ch {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                other => other,
            });
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            return Some((value, &text[index + ch.len_utf8()..]));
        }
        value.push(ch);
    }
    None
}

/// Bare filler articles stripped from multi-word names (DEVL-126). Only
/// whole, space-separated occurrences are removed, so a name written with
/// explicit underscores (e.g. `the_white_house`) is a single token and passes
/// through untouched. Matching is case-sensitive: capitalization is the intent
/// signal (D1), so only the all-lowercase forms below are ever stripped.
const NAME_STOP_WORDS: &[&str] = &["a", "an", "the"];

/// Drops bare article words from a space-separated name. Applied before the
/// character-level underscore sanitization in `sanitize_name`, so
/// `the discount` becomes `discount` while `the_white_house` (one token) and a
/// standalone `a` are left intact.
///
/// Capitalization is the intent signal (D1): an article is stripped only when
/// written all-lowercase in the source. Any capitalization (`A`, `The`, `AN`)
/// marks it as an intentional part of the name and preserves the word, so
/// `Set exhibit A to 1` keeps `A` and `Set The Hague to 1` keeps `The`.
fn strip_name_stop_words(value: &str) -> String {
    let words: Vec<&str> = value.split_whitespace().collect();
    if words.len() < 2 {
        // Single token (or empty): nothing to strip. Protects short names like
        // `a` and underscore-escaped names like `the_white_house`.
        return value.trim().to_string();
    }
    let kept: Vec<&str> = words
        .iter()
        .copied()
        .filter(|word| !NAME_STOP_WORDS.contains(word))
        .collect();
    if kept.is_empty() {
        // Every word was an article; keep the original rather than emit an
        // empty name.
        return value.trim().to_string();
    }
    kept.join(" ")
}

fn sanitize_name(value: &str) -> String {
    let stripped = strip_name_stop_words(value.trim());
    // English possessive markers fold into the name with `_` as the only
    // connector: `salesperson's commission` -> salesperson_commission,
    // `owners' equity` -> owners_equity. The `'s` (when the s ends its word)
    // drops as a unit; a bare apostrophe just drops (DEVL-131 follow-up).
    let chars: Vec<char> = stripped.trim().chars().collect();
    let mut cleaned = String::with_capacity(chars.len());
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if ch == '\'' {
            let possessive_s = matches!(chars.get(index + 1), Some('s') | Some('S'))
                && chars
                    .get(index + 2)
                    .map_or(true, |next| !next.is_ascii_alphanumeric());
            index += if possessive_s { 2 } else { 1 };
            continue;
        }
        cleaned.push(ch);
        index += 1;
    }
    let mut out = String::new();
    let mut previous_underscore = false;
    for ch in cleaned.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch.to_ascii_lowercase());
            previous_underscore = false;
        } else if !previous_underscore {
            out.push('_');
            previous_underscore = true;
        }
    }
    out.trim_matches('_').to_string()
}

fn is_integer(value: &str) -> bool {
    let value = value.strip_prefix('-').unwrap_or(value);
    !value.is_empty() && value.chars().all(|ch| ch.is_ascii_digit())
}

fn is_decimal(value: &str) -> bool {
    let value = value.strip_prefix('-').unwrap_or(value);
    let mut parts = value.split('.');
    let Some(left) = parts.next() else {
        return false;
    };
    let Some(right) = parts.next() else {
        return false;
    };
    parts.next().is_none()
        && !left.is_empty()
        && !right.is_empty()
        && left.chars().all(|ch| ch.is_ascii_digit())
        && right.chars().all(|ch| ch.is_ascii_digit())
}

fn map(fields: Vec<(&str, Value)>) -> Map<String, Value> {
    fields
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

fn string_value(value: &str) -> Value {
    Value::String(value.to_string())
}

fn number_value(value: i64) -> Value {
    Value::Number(Number::from(value))
}

/// SHA-256 of `input` as lowercase hex. The implementation lives in the VM
/// crate so audit records hash identically on every runtime; re-exported here
/// so the compiler and CLI keep a single hashing primitive.
pub use devlish_vm::sha256_hex;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn compile_fixture(source: &str, source_path: &str) -> Value {
        let package = compile_source(
            source,
            CompileOptions {
                source_path: Some(source_path.to_string()),
                search_paths: vec![],
            },
        )
        .expect("fixture compiles");
        serde_json::to_value(package).expect("package serializes")
    }

    fn opcodes(package: &Value) -> Vec<String> {
        package["instructions"]
            .as_array()
            .expect("instructions array")
            .iter()
            .map(|instruction| {
                instruction["op"]
                    .as_str()
                    .expect("instruction op")
                    .to_string()
            })
            .collect()
    }

    fn parse_only(source: &str) -> Program {
        parse_source(source).expect("source parses")
    }

    fn compile_ok(source: &str) -> Value {
        compile_fixture(source, "test.dvl")
    }

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn compiles_xlsx_expected_cells_like_ruby_bytecode() {
        let source = include_str!("../../../examples/xlsx_expected_cells/workflow.dvl");
        let expected: Value = serde_json::from_str(include_str!(
            "../../../examples/xlsx_expected_cells/workflow.dvlc.json"
        ))
        .expect("fixture JSON");

        assert_eq!(
            compile_fixture(source, "examples/xlsx_expected_cells/workflow.dvl"),
            expected
        );
    }

    #[test]
    fn compiles_xlsx_due_diligence_like_ruby_bytecode() {
        let source = include_str!("../../../examples/xlsx_due_diligence_packet/workflow.dvl");
        let expected: Value = serde_json::from_str(include_str!(
            "../../../examples/xlsx_due_diligence_packet/workflow.dvlc.json"
        ))
        .expect("fixture JSON");

        assert_eq!(
            compile_fixture(source, "examples/xlsx_due_diligence_packet/workflow.dvl"),
            expected
        );
    }

    #[test]
    fn compiles_pdf_transfer_packet_like_ruby_bytecode() {
        let source = include_str!("../../../examples/pdf_transfer_packet/workflow.dvl");
        let expected: Value = serde_json::from_str(include_str!(
            "../../../examples/pdf_transfer_packet/workflow.dvlc.json"
        ))
        .expect("fixture JSON");

        assert_eq!(
            compile_fixture(source, "examples/pdf_transfer_packet/workflow.dvl"),
            expected
        );
    }

    #[test]
    fn compiles_docx_contract_review_like_ruby_bytecode() {
        let source = include_str!("../../../examples/docx_contract_review/workflow.dvl");
        let expected: Value = serde_json::from_str(include_str!(
            "../../../examples/docx_contract_review/workflow.dvlc.json"
        ))
        .expect("fixture JSON");

        assert_eq!(
            compile_fixture(source, "examples/docx_contract_review/workflow.dvl"),
            expected
        );
    }

    #[test]
    fn compiles_review_score_control_flow_like_ruby_bytecode() {
        let source = include_str!("../../../examples/bytecode_wasm/review_score.dvl");
        let package = compile_fixture(source, "examples/bytecode_wasm/review_score.dvl");

        assert_eq!(
            package["source_hash"],
            json!("4f433a864e91ec9616bce835e465d867b0ea534bc2b9fca1fffac6aeb6444321")
        );
        assert_eq!(
            package["constant_pool"],
            json!(["Customer tier?", 10, 5, "priority", "tmp/review-score.json"])
        );
        assert_eq!(
            package["symbol_table"],
            json!(["customer_tier", "base_score", "bonus_score", "review_score"])
        );
        assert_eq!(
            opcodes(&package),
            vec![
                "CONST",
                "ASK",
                "CONST",
                "STORE",
                "CONST",
                "STORE",
                "LOAD",
                "LOAD",
                "ADD",
                "STORE",
                "LOAD",
                "CONST",
                "EQ",
                "JUMP_IF_FALSE",
                "LOAD",
                "CONST",
                "ADD",
                "STORE",
                "LOAD",
                "PRINT",
                "LOAD",
                "CONST",
                "EXPORT",
                "RETURN",
            ]
        );
        assert_eq!(package["instructions"][13]["target"], json!(18));
        assert_eq!(
            package["effect_table"],
            json!([
                {
                    "kind": "input",
                    "line": 1,
                    "source_text": "Ask \"Customer tier?\" as customer tier",
                    "target": "customer_tier"
                },
                {
                    "kind": "file_write",
                    "line": 10,
                    "source_text": "Export review_score to \"tmp/review-score.json\"",
                    "mode": "export"
                }
            ])
        );
        assert_eq!(
            package["imports"],
            json!(["emit_event", "request_input", "write_file"])
        );
    }

    #[test]
    fn reports_unsupported_lines_with_source_location() {
        let error = compile_source(
            "Read XLSX cell \"Summary!B2\" as site name\n@@@unsupported gibberish###",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect_err("unsupported line fails");

        assert_eq!(error.diagnostics[0].line, 2);
        assert!(error.diagnostics[0]
            .message
            .contains("Unsupported native compiler statement"));
    }

    // --- New tests for expanded language coverage ---

    #[test]
    fn parses_while_loop() {
        let source =
            "counter equals 0\nWhile counter less than 10:\n  counter equals counter plus 1";
        let package = compile_ok(source);
        let ops = opcodes(&package);
        // Should contain: CONST, STORE (counter=0), then loop: LOAD, CONST, LT, JUMP_IF_FALSE, LOAD, CONST, ADD, STORE, JUMP, RETURN
        assert!(ops.contains(&"JUMP_IF_FALSE".to_string()));
        assert!(ops.contains(&"JUMP".to_string()));
        assert!(ops.contains(&"LT".to_string()));
        // The last JUMP before RETURN should point back to loop start
        let jump_indices: Vec<usize> = ops
            .iter()
            .enumerate()
            .filter(|(_, op)| op.as_str() == "JUMP")
            .map(|(i, _)| i)
            .collect();
        assert!(!jump_indices.is_empty());
    }

    #[test]
    fn parses_until_loop() {
        let source = "done equals false\nUntil done is true:\n  done equals true";
        let package = compile_ok(source);
        let ops = opcodes(&package);
        assert!(ops.contains(&"NOT".to_string()));
        assert!(ops.contains(&"JUMP_IF_FALSE".to_string()));
        assert!(ops.contains(&"JUMP".to_string()));
    }

    #[test]
    fn parses_for_each() {
        let source = "items equals list of 1, 2, 3\nFor each item in items:\n  Print item";
        let package = compile_ok(source);
        let ops = opcodes(&package);
        assert!(ops.contains(&"LIST_LEN".to_string()));
        assert!(ops.contains(&"LIST_GET".to_string()));
        assert!(ops.contains(&"JUMP_IF_FALSE".to_string()));
        assert!(ops.contains(&"JUMP".to_string()));
        assert!(ops.contains(&"PRINT".to_string()));
    }

    #[test]
    fn parses_break_and_continue() {
        let source = "While true:\n  break";
        let package = compile_ok(source);
        let ops = opcodes(&package);
        // break emits a JUMP
        let jump_count = ops.iter().filter(|op| op.as_str() == "JUMP").count();
        assert!(jump_count >= 2); // one for break, one for loop back

        let source2 = "While true:\n  continue";
        let package2 = compile_ok(source2);
        let ops2 = opcodes(&package2);
        let jump_count2 = ops2.iter().filter(|op| op.as_str() == "JUMP").count();
        assert!(jump_count2 >= 2);
    }

    #[test]
    fn parses_if_otherwise() {
        let source = "If x is 1:\n  Print \"yes\"\nOtherwise:\n  Print \"no\"";
        let package = compile_ok(source);
        let ops = opcodes(&package);
        // Should have JUMP_IF_FALSE, PRINT, JUMP, PRINT, RETURN
        assert!(ops.contains(&"JUMP_IF_FALSE".to_string()));
        assert!(ops.contains(&"JUMP".to_string()));
        let print_count = ops.iter().filter(|op| op.as_str() == "PRINT").count();
        assert_eq!(print_count, 2);
    }

    #[test]
    fn parses_conditional_assignment() {
        let source = "x equals 10 if y > 5";
        let package = compile_ok(source);
        let ops = opcodes(&package);
        assert!(ops.contains(&"JUMP_IF_FALSE".to_string()));
        assert!(ops.contains(&"STORE".to_string()));
        assert!(ops.contains(&"GT".to_string()));
    }

    #[test]
    fn parses_fail_and_require() {
        let source = "Fail with \"something went wrong\"";
        let package = compile_ok(source);
        let ops = opcodes(&package);
        assert!(ops.contains(&"FAIL".to_string()));

        let source2 = "Require x greater than 0 otherwise fail with \"x must be positive\"";
        let package2 = compile_ok(source2);
        let ops2 = opcodes(&package2);
        assert!(ops2.contains(&"NOT".to_string()));
        assert!(ops2.contains(&"JUMP_IF_FALSE".to_string()));
        assert!(ops2.contains(&"FAIL".to_string()));
    }

    #[test]
    fn parses_append_and_pop() {
        let source = "Append 42 to my_list";
        let package = compile_ok(source);
        let ops = opcodes(&package);
        assert!(ops.contains(&"LIST_APPEND".to_string()));

        let source2 = "Pop from my_list and save as last_item";
        let package2 = compile_ok(source2);
        let ops2 = opcodes(&package2);
        assert!(ops2.contains(&"LIST_POP".to_string()));
    }

    #[test]
    fn parses_boolean_and_nil_literals() {
        let source = "x equals true\ny equals false\nz equals nil";
        let package = compile_ok(source);
        assert!(package["constant_pool"]
            .as_array()
            .unwrap()
            .contains(&json!(true)));
        assert!(package["constant_pool"]
            .as_array()
            .unwrap()
            .contains(&json!(false)));
        assert!(package["constant_pool"]
            .as_array()
            .unwrap()
            .contains(&json!(null)));
    }

    #[test]
    fn parses_logical_operators() {
        let source = "If x > 0 and y > 0:\n  Print \"both positive\"";
        let package = compile_ok(source);
        let ops = opcodes(&package);
        assert!(ops.contains(&"AND".to_string()));

        let source2 = "If a is 1 or b is 2:\n  Print \"one matches\"";
        let package2 = compile_ok(source2);
        let ops2 = opcodes(&package2);
        assert!(ops2.contains(&"OR".to_string()));
    }

    #[test]
    fn parses_list_literal() {
        let source = "colors equals list of \"red\", \"green\", \"blue\"";
        let package = compile_ok(source);
        let ops = opcodes(&package);
        assert!(ops.contains(&"LIST_BUILD".to_string()));
    }

    #[test]
    fn parses_record_literal() {
        let source = "person equals record with \"Alice\" as name and 30 as age";
        let package = compile_ok(source);
        let ops = opcodes(&package);
        assert!(ops.contains(&"RECORD_BUILD".to_string()));
    }

    #[test]
    fn parses_builtin_calls() {
        let source = "n equals count of items";
        let package = compile_ok(source);
        let ops = opcodes(&package);
        assert!(ops.contains(&"CALL_BUILTIN".to_string()));

        let source2 = "f equals first of items";
        let package2 = compile_ok(source2);
        let ops2 = opcodes(&package2);
        assert!(ops2.contains(&"CALL_BUILTIN".to_string()));

        let source3 = "u equals uppercase name";
        let package3 = compile_ok(source3);
        let ops3 = opcodes(&package3);
        assert!(ops3.contains(&"CALL_BUILTIN".to_string()));
    }

    #[test]
    fn parses_field_access() {
        let source = "n equals name of person";
        let package = compile_ok(source);
        let ops = opcodes(&package);
        assert!(ops.contains(&"FIELD_GET".to_string()));
    }

    #[test]
    fn parses_string_comparisons() {
        let source = "If name contains \"test\":\n  Print \"found\"";
        let package = compile_ok(source);
        let ops = opcodes(&package);
        assert!(ops.contains(&"STR_CONTAINS".to_string()));

        let source2 = "If name starts with \"pre\":\n  Print \"starts\"";
        let package2 = compile_ok(source2);
        let ops2 = opcodes(&package2);
        assert!(ops2.contains(&"STR_STARTS_WITH".to_string()));

        let source3 = "If name ends with \"end\":\n  Print \"ends\"";
        let package3 = compile_ok(source3);
        let ops3 = opcodes(&package3);
        assert!(ops3.contains(&"STR_ENDS_WITH".to_string()));
    }

    #[test]
    fn parses_record_shape_conditions() {
        let package = compile_ok(
            "invoice equals record with 1200 as amount\nRequire invoice has fields amount, status",
        );
        let ops = opcodes(&package);
        assert!(ops.contains(&"CALL_BUILTIN".to_string()));

        let package2 = compile_ok(
            "invoice equals record with 1200 as amount\nshape equals record with \"number\" as amount\nRequire invoice matches shape shape",
        );
        let ops2 = opcodes(&package2);
        assert!(ops2.contains(&"CALL_BUILTIN".to_string()));
    }

    #[test]
    fn parses_set_field() {
        let source = "Set x to 10";
        let package = compile_ok(source);
        let ops = opcodes(&package);
        assert!(ops.contains(&"STORE".to_string()));

        let source2 = "Set amount of invoice of review_packet to 1300";
        let package2 = compile_ok(source2);
        let ops2 = opcodes(&package2);
        assert!(ops2.contains(&"FIELD_SET_PATH".to_string()));
    }

    #[test]
    fn parses_bind() {
        let source = "Alias source as target";
        let package = compile_ok(source);
        let ops = opcodes(&package);
        assert!(ops.contains(&"LOAD".to_string()));
        assert!(ops.contains(&"STORE".to_string()));
        assert!(package["symbol_table"]
            .as_array()
            .unwrap()
            .contains(&json!("source")));
        assert!(package["symbol_table"]
            .as_array()
            .unwrap()
            .contains(&json!("target")));
    }

    #[test]
    fn parses_load_and_extract() {
        let source = "Load \"data.txt\" as doc";
        let package = compile_ok(source);
        let ops = opcodes(&package);
        assert!(ops.contains(&"LOAD_FILE".to_string()));

        let source2 = "Find total amount and save as amount";
        let package2 = compile_ok(source2);
        let ops2 = opcodes(&package2);
        assert!(ops2.contains(&"EXTRACT".to_string()));
    }

    #[test]
    fn parses_structured_file_io() {
        let package = compile_ok("Read JSON from \"input.json\" as packet");
        let ops = opcodes(&package);
        assert!(ops.contains(&"READ_FILE".to_string()));
        assert_eq!(package["instructions"][1]["format"], json!("json"));

        let package2 = compile_ok("Read CSV from \"input.csv\" as rows");
        let ops2 = opcodes(&package2);
        assert!(ops2.contains(&"READ_FILE".to_string()));
        assert_eq!(package2["instructions"][1]["format"], json!("csv"));

        let package3 = compile_ok("Export rows to \"out.csv\" as CSV");
        assert_eq!(package3["instructions"][2]["mode"], json!("csv"));

        let package4 = compile_ok("Append \"line\" to file \"out.txt\"");
        assert_eq!(package4["instructions"][2]["mode"], json!("append"));

        let package5 = compile_ok("Overwrite \"fresh\" to \"out.txt\"");
        assert_eq!(package5["instructions"][2]["mode"], json!("overwrite"));
    }

    #[test]
    fn parses_multiline_input_helpers() {
        let package = compile_ok("Ask multiline \"Paste details\" as details");
        assert_eq!(
            package["instructions"][1]["input_source"],
            json!("multiline_prompt")
        );

        let package2 = compile_ok("Read multiline input as notes");
        assert_eq!(
            package2["instructions"][1]["input_source"],
            json!("multiline_stdin")
        );
    }

    #[test]
    fn parses_service_calls() {
        let source = "Send email to ops";
        let package = compile_ok(source);
        let ops = opcodes(&package);
        assert!(ops.contains(&"SERVICE_CALL".to_string()));

        let source2 = "Send message to admin";
        let package2 = compile_ok(source2);
        let ops2 = opcodes(&package2);
        assert!(ops2.contains(&"SERVICE_CALL".to_string()));
    }

    #[test]
    fn parses_validate() {
        let source = "amount must be at least 100";
        let package = compile_ok(source);
        let ops = opcodes(&package);
        assert!(ops.contains(&"VALIDATE".to_string()));
        assert_eq!(package["instructions"][2]["rule"], json!("minimum"));

        let source2 = "quantity must be at most 90";
        let package2 = compile_ok(source2);
        let ops2 = opcodes(&package2);
        assert!(ops2.contains(&"VALIDATE".to_string()));
        assert_eq!(package2["instructions"][2]["rule"], json!("maximum"));
    }

    #[test]
    fn parses_definition() {
        let program = parse_only("A customer is a person who buys things");
        assert_eq!(program.statements.len(), 1);
        match &program.statements[0].kind {
            StatementKind::Definition { name, definition } => {
                assert_eq!(name, "A customer");
                assert_eq!(definition, "a person who buys things");
            }
            other => panic!("Expected Definition, got {:?}", other),
        }
    }

    #[test]
    fn parses_route() {
        let source = "Route invoice to approved_queue";
        let package = compile_ok(source);
        let ops = opcodes(&package);
        assert!(ops.contains(&"ROUTE".to_string()));
    }

    #[test]
    fn parses_import() {
        // Import is resolved at compile time. Without a real file on disk,
        // it produces a compile error (import not found). Test the parse
        // path directly instead.
        let program = parse_only("Import \"shared/helpers.dvl\"\nx equals 1");
        assert_eq!(program.statements.len(), 2);
        match &program.statements[0].kind {
            StatementKind::Import { path } => assert_eq!(path, "shared/helpers.dvl"),
            other => panic!("Expected Import, got {:?}", other),
        }
    }

    #[test]
    fn parses_triggers() {
        let program = parse_only("Every day at 9am:\n  Print \"hello\"");
        assert_eq!(program.statements.len(), 1);
        match &program.statements[0].kind {
            StatementKind::Trigger {
                trigger_type,
                params,
            } => {
                assert_eq!(trigger_type, "schedule");
                assert!(params.iter().any(|(k, _)| k == "time"));
            }
            other => panic!("Expected Trigger, got {:?}", other),
        }

        let program2 = parse_only("When new document arrives:\n  Print \"got it\"");
        assert_eq!(program2.statements.len(), 1);
        match &program2.statements[0].kind {
            StatementKind::Trigger {
                trigger_type,
                params,
            } => {
                assert_eq!(trigger_type, "event");
                assert!(params.iter().any(|(k, _)| k == "event"));
            }
            other => panic!("Expected Trigger, got {:?}", other),
        }
    }

    #[test]
    fn existing_fixtures_unchanged() {
        // This test validates that all existing fixture tests still produce
        // byte-identical output. It re-runs them as a grouped sanity check.
        let xlsx_cells_source = include_str!("../../../examples/xlsx_expected_cells/workflow.dvl");
        let xlsx_cells_expected: Value = serde_json::from_str(include_str!(
            "../../../examples/xlsx_expected_cells/workflow.dvlc.json"
        ))
        .unwrap();
        assert_eq!(
            compile_fixture(
                xlsx_cells_source,
                "examples/xlsx_expected_cells/workflow.dvl"
            ),
            xlsx_cells_expected
        );

        let xlsx_dd_source =
            include_str!("../../../examples/xlsx_due_diligence_packet/workflow.dvl");
        let xlsx_dd_expected: Value = serde_json::from_str(include_str!(
            "../../../examples/xlsx_due_diligence_packet/workflow.dvlc.json"
        ))
        .unwrap();
        assert_eq!(
            compile_fixture(
                xlsx_dd_source,
                "examples/xlsx_due_diligence_packet/workflow.dvl"
            ),
            xlsx_dd_expected
        );

        let pdf_source = include_str!("../../../examples/pdf_transfer_packet/workflow.dvl");
        let pdf_expected: Value = serde_json::from_str(include_str!(
            "../../../examples/pdf_transfer_packet/workflow.dvlc.json"
        ))
        .unwrap();
        assert_eq!(
            compile_fixture(pdf_source, "examples/pdf_transfer_packet/workflow.dvl"),
            pdf_expected
        );

        let docx_source = include_str!("../../../examples/docx_contract_review/workflow.dvl");
        let docx_expected: Value = serde_json::from_str(include_str!(
            "../../../examples/docx_contract_review/workflow.dvlc.json"
        ))
        .unwrap();
        assert_eq!(
            compile_fixture(docx_source, "examples/docx_contract_review/workflow.dvl"),
            docx_expected
        );
    }

    // --- Class-style parsing and compilation tests ---

    #[test]
    fn parses_class_declaration() {
        let source = "HR's Payroll Calculator:\n  calculate wages using hours worked and hourly rate:\n    wages equals hours worked times hourly rate\n    respond with wages";
        let result = parse_class_source(source).expect("parses");
        assert_eq!(result.module_name, "HR");
        assert_eq!(result.class_name, "Payroll Calculator");
        assert_eq!(result.parent_class, None);
    }

    #[test]
    fn parses_class_with_method() {
        let source = "HR's Payroll Calculator:\n  calculate wages using hours worked and hourly rate:\n    wages equals hours worked times hourly rate\n    respond with wages";
        let result = parse_class_source(source).expect("parses");
        assert_eq!(result.methods.len(), 1);
        assert_eq!(result.methods[0].name, "calculate wages");
        assert_eq!(result.methods[0].ruby_name, "calculate_wages");
        assert!(!result.methods[0].body.is_empty());
    }

    #[test]
    fn parses_private_method() {
        let source = "Operations's Invoice Reviewer:\n  review invoice using invoice amount:\n    review_needed equals false\n    respond with review_needed\n\n  privately escalation label using review needed:\n    escalation_label equals \"standard\"\n    respond with escalation_label";
        let result = parse_class_source(source).expect("parses");
        assert_eq!(result.methods.len(), 2);
        assert!(!result.methods[0].is_private);
        assert!(result.methods[1].is_private);
        assert_eq!(result.methods[1].name, "escalation label");
    }

    #[test]
    fn parses_method_with_params() {
        let source = "HR's Payroll Calculator:\n  calculate wages using hours worked and hourly rate:\n    wages equals hours worked times hourly rate\n    respond with wages";
        let result = parse_class_source(source).expect("parses");
        assert_eq!(
            result.methods[0].params,
            vec!["hours_worked", "hourly_rate"]
        );
    }

    #[test]
    fn parses_respond_with() {
        let source = "HR's Payroll Calculator:\n  calculate wages using hours worked and hourly rate:\n    wages equals hours worked times hourly rate\n    respond with wages";
        let result = parse_class_source(source).expect("parses");
        assert!(result.methods[0].return_value.is_some());
        match &result.methods[0].return_value {
            Some(Expression::Variable(name)) => assert_eq!(name, "wages"),
            other => panic!("Expected Variable(wages), got {:?}", other),
        }
    }

    #[test]
    fn parses_class_with_inheritance() {
        let source = "Finance's Senior Calculator based on HR's Payroll Calculator:\n  apply bonus using base:\n    result equals base times 2\n    respond with result";
        let result = parse_class_source(source).expect("parses");
        assert_eq!(result.module_name, "Finance");
        assert_eq!(result.class_name, "Senior Calculator");
        assert_eq!(
            result.parent_class,
            Some(("HR".to_string(), "Payroll Calculator".to_string()))
        );
    }

    #[test]
    fn compiles_simple_class_method() {
        let source = "HR's Payroll Calculator:\n  calculate wages using hours worked and hourly rate:\n    wages equals hours worked times hourly rate\n    respond with wages";
        let package = compile_ok(source);
        let ops = opcodes(&package);
        // Should have LOAD (params), MUL, STORE (wages), LOAD (wages), STORE (__return__), RETURN, RETURN
        assert!(ops.contains(&"MUL".to_string()));
        assert!(ops.contains(&"STORE".to_string()));
        assert!(ops.contains(&"RETURN".to_string()));
        // Should have class_info
        assert!(package.get("class_info").is_some());
        assert_eq!(package["class_info"]["module"], json!("HR"));
        assert_eq!(package["class_info"]["class"], json!("Payroll Calculator"));
        // Should have methods metadata
        assert!(package.get("methods").is_some());
        let methods = package["methods"].as_array().unwrap();
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0]["name"], json!("calculate wages"));
        assert_eq!(methods[0]["ruby_name"], json!("calculate_wages"));
    }

    #[test]
    fn parses_method_call_expression() {
        let expr = parse_expression("escalation label using review_needed");
        match expr {
            Expression::MethodCall { name, arguments } => {
                assert_eq!(name, "escalation_label");
                assert_eq!(arguments.len(), 1);
            }
            other => panic!("Expected MethodCall, got {:?}", other),
        }
    }

    #[test]
    fn parses_slash_inside_string_literal_as_concat_not_division() {
        // A "/" inside a quoted literal must not be split as the division
        // operator when the literal is an operand of `plus`.
        let expr = parse_expression("a plus \" / \" plus b");
        match expr {
            Expression::Binary { operator, .. } => assert!(
                matches!(operator, BinaryOperator::Add),
                "expected concat (Add), got {:?}",
                operator
            ),
            other => panic!("Expected Binary Add, got {:?}", other),
        }
    }

    #[test]
    fn parses_is_present_as_not_missing() {
        // Conditions route through parse_condition_expression; "is present" must
        // resolve to NOT(is missing), not collapse to a bare variable.
        let expr = parse_condition_expression("name is present");
        match expr {
            Expression::LogicalNot { operand } => match *operand {
                Expression::IsMissing(_) => {}
                other => panic!("Expected LogicalNot(IsMissing), got LogicalNot({:?})", other),
            },
            other => panic!("Expected LogicalNot, got {:?}", other),
        }
    }

    // --- End-to-end tests: compile AND run through the VM ---

    use devlish_vm::{HostEffects, Vm};

    struct TestHost {
        events: Vec<Value>,
        files_written: Vec<Value>,
    }

    impl TestHost {
        fn new() -> Self {
            Self {
                events: Vec::new(),
                files_written: Vec::new(),
            }
        }
    }

    impl HostEffects for TestHost {
        fn emit_event(&mut self, event: &Value) {
            self.events.push(event.clone());
        }
        fn write_file(&mut self, request: &Value) -> Result<(), String> {
            self.files_written.push(request.clone());
            Ok(())
        }
        fn read_file(&mut self, request: &Value) -> Result<Value, String> {
            let path = request
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| "read_file request missing path".to_string())?;
            std::fs::read_to_string(path)
                .map(Value::String)
                .map_err(|error| format!("failed to read {path}: {error}"))
        }
    }

    fn compile_and_run_with_host(source: &str, input: Value) -> Result<(Value, TestHost), String> {
        let json = compile_source_to_json(
            source,
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .map_err(|e| format!("compile error: {e}"))?;
        let package: Value =
            serde_json::from_str(&json).map_err(|e| format!("json parse error: {e}"))?;
        let mut host = TestHost::new();
        let mut vm = Vm::new(package, input).map_err(|e| e.message)?;
        let result = vm.run(&mut host).map_err(|e| e.message)?;
        Ok((result, host))
    }

    fn compile_and_run(source: &str, input: Value) -> Result<Value, String> {
        compile_and_run_with_host(source, input).map(|(result, _host)| result)
    }

    fn compile_and_run_ok(source: &str, input: Value) -> Value {
        compile_and_run(source, input).expect("expected successful run")
    }

    fn compile_and_run_err(source: &str, input: Value) -> String {
        compile_and_run(source, input).expect_err("expected runtime error")
    }

    fn run_package_err(package: Value, input: Value) -> String {
        let mut host = TestHost::new();
        let mut vm = Vm::new(package, input).expect("VM init should succeed");
        vm.run(&mut host)
            .expect_err("expected runtime error")
            .message
    }

    #[test]
    fn e2e_assignment_and_print() {
        let result =
            compile_and_run_ok("greeting equals \"hello world\"\nPrint greeting", json!({}));
        assert_eq!(
            result
                .get("context")
                .and_then(|c| c.get("greeting"))
                .and_then(Value::as_str),
            Some("hello world")
        );
    }

    #[test]
    fn e2e_oxford_comma_list_literal() {
        let result = compile_and_run_ok(
            "line_items equals list of 1200, 450, and 89\nn equals count of line_items\ntotal equals 0\nFor each item in line_items:\n  total equals total plus item",
            json!({}),
        );
        let context = result.get("context").expect("context");
        assert_eq!(context.get("line_items"), Some(&json!([1200, 450, 89])));
        assert_eq!(context.get("n"), Some(&json!(3)));
        assert_eq!(context.get("total"), Some(&json!(1739)));
    }

    #[test]
    fn e2e_arithmetic() {
        let result = compile_and_run_ok("x equals 10\ny equals 3\nz equals x plus y", json!({}));
        // VM stores integers as JSON integers, not floats
        let z = result.get("context").and_then(|c| c.get("z"));
        assert_eq!(z, Some(&json!(13)));
    }

    #[test]
    fn e2e_if_otherwise() {
        // "x is greater than 3" now works after DEVL-36 fix
        let result = compile_and_run_ok(
            "x equals 5\nIf x is greater than 3:\n  label equals \"big\"\nOtherwise:\n  label equals \"small\"",
            json!({}),
        );
        assert_eq!(
            result
                .get("context")
                .and_then(|c| c.get("label"))
                .and_then(Value::as_str),
            Some("big")
        );
    }

    #[test]
    fn e2e_for_each_loop() {
        let result = compile_and_run_ok(
            "items equals list of \"a\", \"b\", \"c\"\ncount equals 0\nFor each item in items:\n  count equals count plus 1",
            json!({}),
        );
        let count = result.get("context").and_then(|c| c.get("count"));
        assert_eq!(count, Some(&json!(3)));
    }

    #[test]
    fn e2e_while_loop() {
        // "n is less than 5" now works after DEVL-36 fix
        let result = compile_and_run_ok(
            "n equals 0\nWhile n is less than 5:\n  n equals n plus 1",
            json!({}),
        );
        let n = result.get("context").and_then(|c| c.get("n"));
        assert_eq!(n, Some(&json!(5)));
    }

    #[test]
    fn e2e_expect_pass() {
        let result =
            compile_and_run_ok("x equals 42\nExpect x equals 42 as \"x-is-42\"", json!({}));
        let assertions = result
            .get("results")
            .and_then(|r| r.get("assertions"))
            .and_then(Value::as_array);
        assert!(
            assertions.is_some(),
            "expected assertions in results, got: {result}"
        );
        let first = &assertions.unwrap()[0];
        assert_eq!(first.get("passed").and_then(Value::as_bool), Some(true));
    }

    #[test]
    fn e2e_expect_fail() {
        let result =
            compile_and_run_ok("x equals 10\nExpect x equals 99 as \"x-is-99\"", json!({}));
        let assertions = result
            .get("results")
            .and_then(|r| r.get("assertions"))
            .and_then(Value::as_array);
        assert!(
            assertions.is_some(),
            "expected assertions in results, got: {result}"
        );
        let first = &assertions.unwrap()[0];
        assert_eq!(first.get("passed").and_then(Value::as_bool), Some(false));
    }

    #[test]
    fn e2e_fail_with() {
        let err = compile_and_run_err("Fail with \"something went wrong\"", json!({}));
        assert!(err.contains("something went wrong"), "got: {err}");
    }

    #[test]
    fn e2e_require_passes() {
        let result = compile_and_run_ok("x equals 1\nRequire x equals 1", json!({}));
        assert_eq!(
            result.get("context").and_then(|c| c.get("x")),
            Some(&json!(1))
        );
    }

    #[test]
    fn e2e_require_fails() {
        let err = compile_and_run_err(
            "x equals 1\nRequire x equals 99 or fail with \"bad x\"",
            json!({}),
        );
        // The custom message may or may not appear; the key is that it errors
        assert!(
            err.contains("fail") || err.contains("Requirement"),
            "got: {err}"
        );
    }

    #[test]
    fn e2e_input_from_json() {
        let result = compile_and_run_ok(
            "Ask \"name?\" as user_name\nPrint user_name",
            json!({"user_name": "Alice"}),
        );
        assert_eq!(
            result
                .get("context")
                .and_then(|c| c.get("user_name"))
                .and_then(Value::as_str),
            Some("Alice")
        );
    }

    #[test]
    fn e2e_multiline_input_from_json() {
        let result = compile_and_run_ok(
            "Ask multiline \"Paste notes\" as notes\nRead multiline input as transcript",
            json!({
                "notes": "first line\nsecond line",
                "transcript": "alpha\nbeta",
            }),
        );
        assert_eq!(
            result
                .get("context")
                .and_then(|c| c.get("notes"))
                .and_then(Value::as_str),
            Some("first line\nsecond line"),
        );
        assert_eq!(
            result
                .get("context")
                .and_then(|c| c.get("transcript"))
                .and_then(Value::as_str),
            Some("alpha\nbeta"),
        );
    }

    #[test]
    fn e2e_record_and_field_access() {
        // Syntax is "value as key", not "key as value"
        let result = compile_and_run_ok(
            "person equals record with \"Ada\" as name and 36 as age\nresult equals name of person",
            json!({}),
        );
        assert_eq!(
            result
                .get("context")
                .and_then(|c| c.get("result"))
                .and_then(Value::as_str),
            Some("Ada")
        );
    }

    #[test]
    fn e2e_nested_field_set_updates_root_record() {
        let result = compile_and_run_ok(
            "Set amount of invoice of review_packet to 1300\nresult equals amount of invoice of review_packet",
            json!({"review_packet": {"invoice": {"amount": 1200, "status": "pending"}}}),
        );
        assert_eq!(
            result
                .get("context")
                .and_then(|c| c.get("review_packet"))
                .and_then(|p| p.get("invoice"))
                .and_then(|i| i.get("amount")),
            Some(&json!(1300)),
        );
        assert_eq!(
            result.get("context").and_then(|c| c.get("result")),
            Some(&json!(1300)),
        );
    }

    #[test]
    fn e2e_concat_with_slash_literal() {
        let result = compile_and_run_ok(
            "combined equals a plus \" / \" plus b",
            json!({"a": "x", "b": "y"}),
        );
        assert_eq!(
            result.get("context").and_then(|c| c.get("combined")),
            Some(&json!("x / y")),
        );
    }

    #[test]
    fn e2e_is_present_condition_true_and_false_branches() {
        // present -> condition true -> value overridden
        let present = compile_and_run_ok(
            "flag equals \"no\"\nflag equals \"yes\" if name is present",
            json!({"name": "Acme"}),
        );
        assert_eq!(
            present.get("context").and_then(|c| c.get("flag")),
            Some(&json!("yes")),
        );
        // missing -> condition false -> default kept
        let absent = compile_and_run_ok(
            "flag equals \"no\"\nflag equals \"yes\" if name is present",
            json!({}),
        );
        assert_eq!(
            absent.get("context").and_then(|c| c.get("flag")),
            Some(&json!("no")),
        );
    }

    #[test]
    fn e2e_nested_field_set_creates_missing_intermediate_records() {
        let result =
            compile_and_run_ok("Set amount of invoice of review_packet to 1300", json!({}));
        assert_eq!(
            result
                .get("context")
                .and_then(|c| c.get("review_packet"))
                .and_then(|p| p.get("invoice"))
                .and_then(|i| i.get("amount")),
            Some(&json!(1300)),
        );
    }

    #[test]
    fn e2e_nested_field_set_reports_non_record_paths() {
        let err = compile_and_run_err(
            "Set amount of invoice of review_packet to 1300",
            json!({"review_packet": {"invoice": 7}}),
        );
        assert!(
            err.contains("Set field failed for review_packet"),
            "got: {err}"
        );
        assert!(
            err.contains("amount cannot be set on a non-record value")
                || err.contains("invoice cannot be set on a non-record value"),
            "got: {err}"
        );
    }

    #[test]
    fn e2e_record_field_and_shape_requirements() {
        let result = compile_and_run_ok(
            "invoice equals record with 1200 as amount and \"Ada\" as customer\ninvoice_shape equals record with \"number\" as amount and \"text\" as customer\nRequire invoice has fields amount, customer\nRequire invoice matches shape invoice_shape",
            json!({}),
        );
        assert_eq!(
            result.get("context").and_then(|c| c.get("invoice")),
            Some(&json!({"amount": 1200, "customer": "Ada"})),
        );

        let err = compile_and_run_err(
            "invoice equals record with \"1200\" as amount\ninvoice_shape equals record with \"number\" as amount\nRequire invoice matches shape invoice_shape otherwise fail with \"bad invoice shape\"",
            json!({}),
        );
        assert!(err.contains("bad invoice shape"), "got: {err}");
    }

    #[test]
    fn e2e_list_append_and_count() {
        let result = compile_and_run_ok(
            "items equals list of 1, 2, 3\nAppend 4 to items\nn equals count of items",
            json!({}),
        );
        let n = result.get("context").and_then(|c| c.get("n"));
        assert_eq!(n, Some(&json!(4)));
    }

    #[test]
    fn e2e_collection_query_transform_helpers() {
        let result = compile_and_run_ok(
            "invoice_one equals record with 1200 as amount and \"pending\" as status\ninvoice_two equals record with 300 as amount and \"approved\" as status\ninvoices equals list of invoice_one and invoice_two\npending_invoice equals find invoices where status equals \"pending\"\nlarge_invoices equals filter invoices where amount >= 1000\napproved_invoices equals reject invoices where status equals \"pending\"\nany_pending equals any of invoices where status equals \"pending\"\nall_large equals all of invoices where amount >= 300\ngroups equals group invoices by status\nindexed equals index invoices by status\nfirst_invoice_list equals take 1 of invoices\nafter_first equals drop 1 of invoices\nchunks equals chunk invoices by 1\npairs equals zip list of \"name\", \"amount\" with list of \"Ada\", \"1200\"\ncombined equals union of list of \"a\", \"b\" and list of \"b\", \"c\"",
            json!({}),
        );
        let context = result.get("context").and_then(Value::as_object).unwrap();
        assert_eq!(
            context
                .get("pending_invoice")
                .and_then(|value| value.get("amount")),
            Some(&json!(1200)),
        );
        assert_eq!(
            context
                .get("large_invoices")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1),
        );
        assert_eq!(context.get("any_pending"), Some(&json!(true)));
        assert_eq!(context.get("all_large"), Some(&json!(true)));
        assert!(context
            .get("groups")
            .and_then(|groups| groups.get("pending"))
            .is_some());
        assert!(context
            .get("indexed")
            .and_then(|index| index.get("approved"))
            .is_some());
        assert_eq!(
            context
                .get("first_invoice_list")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1),
        );
        assert_eq!(context.get("combined"), Some(&json!(["a", "b", "c"])));
    }

    #[test]
    fn e2e_beginner_map_reject_reduce_patterns() {
        let result = compile_and_run_ok(
            "statuses equals list of \" pending \", \"approved\", \" pending \"\ncleaned_statuses equals map statuses to trim item\nkept_statuses equals reject cleaned_statuses where item is \"pending\"\nstatus_count equals reduce kept_statuses starting at 0 with total and item to total plus 1",
            json!({}),
        );
        assert_eq!(
            result
                .get("context")
                .and_then(|c| c.get("cleaned_statuses")),
            Some(&json!(["pending", "approved", "pending"])),
        );
        assert_eq!(
            result.get("context").and_then(|c| c.get("kept_statuses")),
            Some(&json!(["approved"])),
        );
        assert_eq!(
            result.get("context").and_then(|c| c.get("status_count")),
            Some(&json!(1)),
        );
    }

    #[test]
    fn e2e_text_and_date_helpers() {
        let result = compile_and_run_ok(
            "messy equals \"  hello   world  \"\nclean equals normalize whitespace of messy\nslug equals slugify \"Hello, World!\"\ntitled equals title case of \"hello world\"\nsentence equals sentence case of \"hELLO WORLD\"\nwords_list equals words of clean\nhas_world equals contains \"world\" in clean\ndue_date equals add 7 days to \"2026-06-30\"\nspan equals days between \"2026-06-30\" and due_date\nbusiness_span equals business days between \"2026-06-30\" and due_date",
            json!({}),
        );
        let context = result.get("context").and_then(Value::as_object).unwrap();
        assert_eq!(context.get("clean"), Some(&json!("hello world")));
        assert_eq!(context.get("slug"), Some(&json!("hello-world")));
        assert_eq!(context.get("titled"), Some(&json!("Hello World")));
        assert_eq!(context.get("sentence"), Some(&json!("Hello world")));
        assert_eq!(context.get("words_list"), Some(&json!(["hello", "world"])));
        assert_eq!(context.get("has_world"), Some(&json!(true)));
        assert_eq!(context.get("due_date"), Some(&json!("2026-07-07")));
        assert_eq!(context.get("span"), Some(&json!(7)));
        assert_eq!(context.get("business_span"), Some(&json!(5)));
    }

    #[test]
    fn e2e_validation_vocabulary_and_recovery() {
        let result = compile_and_run_ok(
            "status equals \"approved\"\ncode equals \"INV-123\"\nnote equals \"hello world\"\ninvoice equals record with 1200 as amount and \"approved\" as status\nstatus must equal \"approved\"\nnote must contain \"world\"\ncode must match \"INV-*\"\nmissing_field must be missing\nstatus must be one of list of \"approved\", \"pending\"\ninvoice.status must equal \"approved\"\ninvoice.amount must be at least 1000",
            json!({}),
        );
        assert_eq!(
            result
                .get("results")
                .and_then(|r| r.get("validations"))
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(7),
        );

        let recovered = compile_and_run_ok(
            "status equals \"pending\"\nTry:\n  status must equal \"approved\"\nOtherwise:\n  status equals \"manual_review\"",
            json!({}),
        );
        assert_eq!(
            recovered.get("context").and_then(|c| c.get("status")),
            Some(&json!("manual_review")),
        );
        assert!(recovered
            .get("context")
            .and_then(|c| c.get("last_error"))
            .is_some());

        let err = compile_and_run_err(
            "status equals \"pending\"\nstatus must equal \"approved\"",
            json!({}),
        );
        assert!(err.contains("expected status to equal"), "got: {err}");
    }

    #[test]
    fn e2e_builtin_uppercase() {
        // "uppercase of x" now works after DEVL-38 fix
        let result = compile_and_run_ok("x equals \"hello\"\ny equals uppercase of x", json!({}));
        assert_eq!(
            result
                .get("context")
                .and_then(|c| c.get("y"))
                .and_then(Value::as_str),
            Some("HELLO")
        );
    }

    #[test]
    fn e2e_service_call_errors_without_host() {
        // TestHost doesn't implement call_service, so it uses the default
        // which returns an error
        let err = compile_and_run_err("Send email to ops", json!({}));
        assert!(
            err.contains("Service call") || err.contains("not implemented"),
            "got: {err}"
        );
    }

    #[test]
    fn e2e_load_file_errors_without_host() {
        // TestHost doesn't implement read_file, so it uses the default
        let err = compile_and_run_err("Load \"data.csv\" as report", json!({}));
        assert!(
            err.contains("Load file failed") || err.contains("read_file not implemented"),
            "got: {err}"
        );
    }

    #[test]
    fn e2e_extract_from_context() {
        // Extract pulls a field from a context variable named "document"
        let result = compile_and_run_ok(
            "Find total amount and save as total_amount",
            json!({"document": {"total_amount": 42}}),
        );
        let val = result.get("context").and_then(|c| c.get("total_amount"));
        assert_eq!(val, Some(&json!(42)));
    }

    #[test]
    fn e2e_extract_missing_returns_null() {
        let result = compile_and_run_ok("Find missing field and save as result", json!({}));
        let val = result.get("context").and_then(|c| c.get("result"));
        assert_eq!(val, Some(&Value::Null));
    }

    #[test]
    fn e2e_require_doc_passes() {
        let result = compile_and_run_ok(
            "Document must contain contract",
            json!({"contract": "signed"}),
        );
        assert!(result.get("context").is_some());
    }

    #[test]
    fn e2e_require_doc_fails() {
        let err = compile_and_run_err("Document must contain contract", json!({}));
        assert!(err.contains("requirement not met"), "got: {err}");
    }

    #[test]
    fn e2e_route_calls_write_file() {
        // Route delegates to write_file on the host
        let result = compile_and_run_ok("x equals \"data\"\nRoute x to \"output\"", json!({}));
        assert!(result.get("context").is_some());
    }

    #[test]
    fn e2e_structured_json_and_csv_file_reads() {
        let dir = std::env::temp_dir().join("devlish_test_structured_reads");
        std::fs::create_dir_all(&dir).unwrap();
        let json_path = dir.join("packet.json");
        let csv_path = dir.join("rows.csv");
        std::fs::write(&json_path, r#"{"invoice":{"amount":1200}}"#).unwrap();
        std::fs::write(&csv_path, "name,amount\nAda,1200\nGrace,800\n").unwrap();

        let source = format!(
            "Read JSON from \"{}\" as packet\nRead CSV from \"{}\" as rows\namount equals amount of invoice of packet\nfirst_name equals name of first of rows",
            json_path.to_string_lossy(),
            csv_path.to_string_lossy(),
        );
        let result = compile_and_run_ok(&source, json!({}));
        assert_eq!(
            result.get("context").and_then(|c| c.get("amount")),
            Some(&json!(1200)),
        );
        assert_eq!(
            result
                .get("context")
                .and_then(|c| c.get("first_name"))
                .and_then(Value::as_str),
            Some("Ada"),
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn e2e_csv_export_and_append_modes_call_host() {
        let (result, host) = compile_and_run_with_host(
            "row equals record with \"Ada\" as name and 1200 as amount\nrows equals list of row\nExport rows to \"out.csv\" as CSV\nAppend \"done\\n\" to file \"log.txt\"\nOverwrite \"fresh\" to \"out.txt\"",
            json!({}),
        )
        .expect("run succeeds");
        assert!(result.get("context").is_some());
        assert_eq!(host.files_written.len(), 3);
        assert_eq!(host.files_written[0]["mode"], json!("csv"));
        assert_eq!(
            host.files_written[0]["content"],
            json!("amount,name\n1200,Ada")
        );
        assert_eq!(host.files_written[1]["mode"], json!("append"));
        assert_eq!(host.files_written[1]["content"], json!("done\n"));
        assert_eq!(host.files_written[2]["mode"], json!("overwrite"));
        assert_eq!(host.files_written[2]["content"], json!("fresh"));
    }

    #[test]
    fn e2e_import_inlines_file() {
        // Write a helper .dvl to a temp file, import it, verify its
        // variables are available in the main program
        let dir = std::env::temp_dir().join("devlish_test_import");
        std::fs::create_dir_all(&dir).unwrap();
        let helper_path = dir.join("helper.dvl");
        std::fs::write(&helper_path, "base_value equals 100").unwrap();

        let main_source = format!(
            "Import \"{}\"\nresult equals base_value",
            helper_path.to_string_lossy()
        );
        let result = compile_and_run_ok(&main_source, json!({}));
        assert_eq!(
            result.get("context").and_then(|c| c.get("base_value")),
            Some(&json!(100)),
        );
        assert_eq!(
            result.get("context").and_then(|c| c.get("result")),
            Some(&json!(100)),
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn e2e_import_not_found_errors() {
        let err = compile_and_run_err("Import \"nonexistent.dvl\"\nx equals 1", json!({}));
        assert!(err.contains("Import not found"), "got: {err}");
    }

    #[test]
    fn duplicate_imports_report_clear_diagnostics() {
        let dir = std::env::temp_dir().join("devlish_test_duplicate_import");
        std::fs::create_dir_all(&dir).unwrap();
        let helper_path = dir.join("helper.dvl");
        std::fs::write(&helper_path, "base_value equals 100").unwrap();

        let source = format!(
            "Import \"{}\"\nImport \"{}\"",
            helper_path.to_string_lossy(),
            helper_path.to_string_lossy()
        );
        let error = compile_source_to_json(
            &source,
            CompileOptions {
                source_path: Some(dir.join("main.dvl").to_string_lossy().to_string()),
                search_paths: vec![],
            },
        )
        .expect_err("duplicate import fails");
        assert!(
            error.to_string().contains("Duplicate import"),
            "got: {error}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn import_name_collisions_report_clear_diagnostics() {
        let dir = std::env::temp_dir().join("devlish_test_import_collision");
        std::fs::create_dir_all(&dir).unwrap();
        let helper_path = dir.join("helper.dvl");
        std::fs::write(&helper_path, "result equals 100").unwrap();

        let source = format!(
            "Import \"{}\"\nresult equals 200",
            helper_path.to_string_lossy()
        );
        let error = compile_source_to_json(
            &source,
            CompileOptions {
                source_path: Some(dir.join("main.dvl").to_string_lossy().to_string()),
                search_paths: vec![],
            },
        )
        .expect_err("name collision fails");
        assert!(
            error.to_string().contains("Import name collision: result"),
            "got: {error}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn duplicated_equals_keyword_is_a_compile_error() {
        let error = compile_source_to_json(
            "x equals equals 5",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect_err("duplicated equals fails");
        assert!(
            error.to_string().contains("reserved word 'equals'"),
            "got: {error}"
        );
    }

    #[test]
    fn equals_nested_in_expression_is_a_compile_error() {
        let error = compile_source_to_json(
            "x equals 1 plus equals",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect_err("equals nested inside a binary expression fails");
        assert!(
            error.to_string().contains("reserved word 'equals'"),
            "got: {error}"
        );
    }

    #[test]
    fn conditional_assignment_with_duplicated_equals_is_a_compile_error() {
        let error = compile_source_to_json(
            "y equals 1\nx equals equals 5 if y is 1",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect_err("duplicated equals in a conditional assignment fails");
        assert!(
            error.to_string().contains("reserved word 'equals'"),
            "got: {error}"
        );
    }

    #[test]
    fn equals_inside_list_literal_is_a_compile_error() {
        let error = compile_source_to_json(
            "x equals list of 1, equals 5",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect_err("reserved word inside a list literal fails");
        assert!(
            error.to_string().contains("reserved word 'equals'"),
            "got: {error}"
        );
    }

    #[test]
    fn equals_inside_predicate_still_compiles() {
        let source = "items equals list of 1, 2\nmatches equals filter items where item equals 1";
        compile_source_to_json(
            source,
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect("equals inside a where predicate is valid");
    }

    #[test]
    fn e2e_import_circular_is_safe() {
        // Two files that import each other should not infinite loop
        let dir = std::env::temp_dir().join("devlish_test_circular");
        std::fs::create_dir_all(&dir).unwrap();
        let a_path = dir.join("a.dvl");
        let b_path = dir.join("b.dvl");
        std::fs::write(
            &a_path,
            format!("Import \"{}\"\nx equals 1", b_path.to_string_lossy()),
        )
        .unwrap();
        std::fs::write(
            &b_path,
            format!("Import \"{}\"\ny equals 2", a_path.to_string_lossy()),
        )
        .unwrap();

        let source = std::fs::read_to_string(&a_path).unwrap();
        let json = compile_source_to_json(
            &source,
            CompileOptions {
                source_path: Some(a_path.to_string_lossy().to_string()),
                search_paths: vec![],
            },
        );
        assert!(
            json.is_ok(),
            "circular import should not error: {:?}",
            json.err()
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn source_hash_covers_imported_files() {
        let dir = std::env::temp_dir().join("devlish_test_closure_covers_imports");
        std::fs::create_dir_all(&dir).unwrap();
        let helper = dir.join("helper.dvl");
        std::fs::write(&helper, "base_value equals 100").unwrap();
        let main_path = dir.join("main.dvl");
        let source = format!(
            "Import \"{}\"\nresult equals base_value",
            helper.to_string_lossy()
        );

        let compile = |src: &str| {
            let json = compile_source_to_json(
                src,
                CompileOptions {
                    source_path: Some(main_path.to_string_lossy().to_string()),
                    search_paths: vec![],
                },
            )
            .expect("compiles");
            serde_json::from_str::<Value>(&json).unwrap()
        };

        let pkg = compile(&source);
        let files = pkg["source_files"]
            .as_array()
            .expect("source_files present when imports exist");
        assert_eq!(files.len(), 2, "entry + helper, got {files:?}");
        let hash_before = pkg["source_hash"].as_str().unwrap().to_string();

        // Editing the imported file must change source_hash even though the
        // top-level source string is byte-identical.
        std::fs::write(&helper, "base_value equals 999").unwrap();
        let hash_after = compile(&source)["source_hash"]
            .as_str()
            .unwrap()
            .to_string();
        assert_ne!(
            hash_before, hash_after,
            "editing an inlined import must change source_hash"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn source_files_manifest_is_relative_and_covers_transitive_imports() {
        let dir = std::env::temp_dir().join("devlish_test_closure_transitive");
        std::fs::create_dir_all(&dir).unwrap();
        let leaf = dir.join("leaf.dvl");
        let mid = dir.join("mid.dvl");
        std::fs::write(&leaf, "leaf_value equals 1").unwrap();
        std::fs::write(
            &mid,
            format!("Import \"{}\"\nmid_value equals 2", leaf.to_string_lossy()),
        )
        .unwrap();
        let main_path = dir.join("main.dvl");
        let source = format!(
            "Import \"{}\"\nresult equals mid_value",
            mid.to_string_lossy()
        );

        let compile = |src: &str| {
            let json = compile_source_to_json(
                src,
                CompileOptions {
                    source_path: Some(main_path.to_string_lossy().to_string()),
                    search_paths: vec![],
                },
            )
            .expect("compiles");
            serde_json::from_str::<Value>(&json).unwrap()
        };

        let pkg = compile(&source);
        let files = pkg["source_files"].as_array().unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f["path"].as_str().unwrap()).collect();
        assert_eq!(paths.len(), 3, "main + mid + leaf, got {paths:?}");
        for path in &paths {
            assert!(
                !std::path::Path::new(path).is_absolute(),
                "manifest paths must be relative, got {path}"
            );
            assert!(
                !path.contains("devlish_test_closure_transitive"),
                "manifest paths must not embed the absolute temp dir, got {path}"
            );
        }
        // Sibling files in the same directory relativize to bare basenames.
        assert!(paths.contains(&"main.dvl"));
        assert!(paths.contains(&"mid.dvl"));
        assert!(paths.contains(&"leaf.dvl"));

        let hash_before = pkg["source_hash"].as_str().unwrap().to_string();
        // Editing the transitively-imported leaf must change source_hash.
        std::fs::write(&leaf, "leaf_value equals 42").unwrap();
        let hash_after = compile(&source)["source_hash"]
            .as_str()
            .unwrap()
            .to_string();
        assert_ne!(
            hash_before, hash_after,
            "editing a transitive import must change source_hash"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn single_file_program_omits_source_files_and_keeps_raw_hash() {
        let json = compile_source_to_json(
            "result equals 1",
            CompileOptions {
                source_path: Some("solo.dvl".to_string()),
                search_paths: vec![],
            },
        )
        .expect("compiles");
        let pkg: Value = serde_json::from_str(&json).unwrap();
        assert!(
            pkg.get("source_files").is_none(),
            "single-file program must omit source_files"
        );
        assert_eq!(
            pkg["source_hash"].as_str().unwrap(),
            sha256_hex(b"result equals 1"),
            "single-file source_hash stays the raw file hash"
        );
    }

    #[test]
    fn source_hash_is_content_only_and_reproducible() {
        // The same entry + import contents compiled from two differently-named
        // directories must produce the same source_hash: the hash covers file
        // contents, not machine-specific paths. This is the reproducibility
        // property that lets a second party recompile and compare.
        let compile_in = |dirname: &str| {
            let dir = std::env::temp_dir().join(dirname);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("helper.dvl"), "base_value equals 100").unwrap();
            let main_path = dir.join("main.dvl");
            // A relative import keeps the entry source byte-identical across
            // directories; the import resolves relative to main.dvl.
            let source = "Import \"helper.dvl\"\nresult equals base_value";
            let json = compile_source_to_json(
                source,
                CompileOptions {
                    source_path: Some(main_path.to_string_lossy().to_string()),
                    search_paths: vec![],
                },
            )
            .expect("compiles");
            let hash = serde_json::from_str::<Value>(&json).unwrap()["source_hash"]
                .as_str()
                .unwrap()
                .to_string();
            std::fs::remove_dir_all(&dir).ok();
            hash
        };

        let hash_a = compile_in("devlish_test_reproducible_a");
        let hash_b = compile_in("devlish_test_reproducible_b");
        assert_eq!(
            hash_a, hash_b,
            "identical source contents in different directories must hash identically"
        );

        // And recompiling the exact same closure is deterministic.
        assert_eq!(compile_in("devlish_test_reproducible_a"), hash_a);
    }

    #[test]
    fn e2e_import_resolves_project_lib_from_devlish_toml() {
        let dir = std::env::temp_dir().join("devlish_test_project_import");
        let workflows = dir.join("workflows");
        let lib = dir.join("lib");
        std::fs::create_dir_all(&workflows).unwrap();
        std::fs::create_dir_all(&lib).unwrap();
        std::fs::write(dir.join("devlish.toml"), "name = \"test\"\n").unwrap();
        std::fs::write(lib.join("shared.dvl"), "base_value equals 100").unwrap();
        let main_path = workflows.join("main.dvl");
        let source = "Import \"shared.dvl\"\nresult equals base_value";

        let json = compile_source_to_json(
            source,
            CompileOptions {
                source_path: Some(main_path.to_string_lossy().to_string()),
                search_paths: vec![],
            },
        )
        .expect("project lib import resolves");
        let package: Value = serde_json::from_str(&json).unwrap();
        assert!(opcodes(&package).contains(&"STORE".to_string()));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn class_method_imports_inline_workflow_fragments() {
        let dir = std::env::temp_dir().join("devlish_test_class_method_import");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("shared.dvl"),
            "review_threshold equals 1000\nfinal_label equals \"needs_review\"",
        )
        .unwrap();
        let main_path = dir.join("reviewer.dvl");
        let source = "Operations's Invoice Reviewer:\n  review invoice:\n    Import \"shared.dvl\"\n    respond with final_label";

        let json = compile_source_to_json(
            source,
            CompileOptions {
                source_path: Some(main_path.to_string_lossy().to_string()),
                search_paths: vec![],
            },
        )
        .expect("class method import resolves");
        let package: Value = serde_json::from_str(&json).unwrap();
        assert!(package["symbol_table"]
            .as_array()
            .unwrap()
            .contains(&json!("final_label")));
        // The symbol name alone is not enough: it appears simply because the
        // method body references `final_label`. Assert the imported VALUE is
        // actually inlined so a dropped import (DEVL-123) cannot pass silently.
        assert!(
            package["constant_pool"]
                .as_array()
                .unwrap()
                .contains(&json!("needs_review")),
            "expected imported value \"needs_review\" inlined into constant_pool, got {:?}",
            package["constant_pool"]
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn class_methods_can_reuse_the_same_imported_fragment() {
        let dir = std::env::temp_dir().join("devlish_test_class_method_reimport");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("shared.dvl"), "final_label equals \"ready\"").unwrap();
        let main_path = dir.join("reviewer.dvl");
        let source = "Operations's Invoice Reviewer:\n  review invoice:\n    Import \"shared.dvl\"\n    respond with final_label\n\n  review refund:\n    Import \"shared.dvl\"\n    respond with final_label";

        let json = compile_source_to_json(
            source,
            CompileOptions {
                source_path: Some(main_path.to_string_lossy().to_string()),
                search_paths: vec![],
            },
        )
        .expect("each method resolves its own import");
        let package: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(package["methods"].as_array().unwrap().len(), 2);
        assert!(opcodes(&package).contains(&"STORE".to_string()));
        // Both methods import the same fragment; assert its VALUE ("ready") is
        // inlined, not merely that the symbol name is referenced (DEVL-123).
        assert!(
            package["constant_pool"]
                .as_array()
                .unwrap()
                .contains(&json!("ready")),
            "expected imported value \"ready\" inlined into constant_pool, got {:?}",
            package["constant_pool"]
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn e2e_class_method_import_returns_inlined_value() {
        // A dropped class-method import (DEVL-123) compiles to a LOAD of an
        // undefined symbol, so the method returns null instead of the imported
        // constant. Compile AND run to prove the imported value reaches runtime.
        let dir = std::env::temp_dir().join("devlish_test_class_method_import_value");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("shared.dvl"), "magic_number equals 42").unwrap();
        let main_path = dir.join("calc.dvl");
        let source = "Operations's Calculator:\n  compute:\n    Import \"shared.dvl\"\n    Respond with record with magic_number as answer";

        let json = compile_source_to_json(
            source,
            CompileOptions {
                source_path: Some(main_path.to_string_lossy().to_string()),
                search_paths: vec![],
            },
        )
        .expect("class method import resolves");
        let package: Value = serde_json::from_str(&json).unwrap();
        assert!(
            package["constant_pool"]
                .as_array()
                .unwrap()
                .contains(&json!(42)),
            "expected imported constant 42 in constant_pool, got {:?}",
            package["constant_pool"]
        );

        let mut host = TestHost::new();
        let mut vm = Vm::new(package, json!({})).expect("VM init should succeed");
        let result = vm.run(&mut host).expect("expected successful run");
        assert_eq!(
            result["context"]["__return__"]["answer"],
            json!(42),
            "expected imported value returned at runtime, got {:?}",
            result
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn duplicate_class_methods_report_clear_diagnostics() {
        let error = compile_source_to_json(
            "Operations's Invoice Reviewer:\n  review invoice:\n    respond with \"one\"\n  review invoice:\n    respond with \"two\"",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect_err("duplicate method fails");
        assert!(
            error.to_string().contains("Duplicate method name"),
            "got: {error}"
        );
    }

    #[test]
    fn e2e_checkpoint_returns_prompt_and_context() {
        let result = compile_and_run_ok(
            "x equals 42\nCheckpoint \"Please review the value of x\"",
            json!({}),
        );
        assert_eq!(
            result.get("is_checkpoint").and_then(Value::as_bool),
            Some(true),
        );
        assert_eq!(
            result.get("prompt").and_then(Value::as_str),
            Some("Please review the value of x"),
        );
        let ctx = result.get("context").and_then(|c| c.get("x"));
        assert_eq!(ctx, Some(&json!(42)));
    }

    #[test]
    fn e2e_checkpoint_stops_execution() {
        // Code after Checkpoint should NOT execute
        let result = compile_and_run_ok(
            "x equals 1\nCheckpoint \"pause here\"\nx equals 999",
            json!({}),
        );
        assert_eq!(
            result.get("is_checkpoint").and_then(Value::as_bool),
            Some(true)
        );
        // x should be 1, not 999
        let ctx = result.get("context").and_then(|c| c.get("x"));
        assert_eq!(ctx, Some(&json!(1)));
    }

    #[test]
    fn e2e_checkpoint_with_context_key() {
        let result = compile_and_run_ok(
            "status equals \"pending\"\nCheckpoint \"needs approval\" saving context as approval_state",
            json!({}),
        );
        assert_eq!(
            result.get("is_checkpoint").and_then(Value::as_bool),
            Some(true)
        );
        let saved = result.get("approval_state");
        assert!(saved.is_some(), "expected approval_state in result");
        assert_eq!(
            saved.and_then(|s| s.get("status")).and_then(Value::as_str),
            Some("pending"),
        );
    }

    #[test]
    fn parses_filesystem_keywords() {
        // Copy
        let package = compile_ok(r#"Copy file from "/a.txt" to "/b.txt""#);
        assert!(opcodes(&package).contains(&"FILE_COPY".to_string()));

        // Move
        let package = compile_ok(r#"Move file from "/a.txt" to "/b.txt""#);
        assert!(opcodes(&package).contains(&"FILE_MOVE".to_string()));

        // Mkdir
        let package = compile_ok(r#"Create directory "/tmp/test""#);
        assert!(opcodes(&package).contains(&"FILE_MKDIR".to_string()));

        // Delete
        let package = compile_ok(r#"Delete file "/tmp/test.txt""#);
        assert!(opcodes(&package).contains(&"FILE_DELETE".to_string()));

        // Exists
        let package = compile_ok(r#"Check if "/tmp/test.txt" exists as file_found"#);
        assert!(opcodes(&package).contains(&"FILE_EXISTS".to_string()));

        // Stat
        let package = compile_ok(r#"Get file info for "/tmp/test.txt" as info"#);
        assert!(opcodes(&package).contains(&"FILE_STAT".to_string()));

        // List
        let package = compile_ok(r#"List files in "/tmp" as entries"#);
        assert!(opcodes(&package).contains(&"FILE_LIST".to_string()));

        // Glob
        let package = compile_ok(r#"Find files matching "*.txt" in "/tmp" as matches"#);
        assert!(opcodes(&package).contains(&"FILE_GLOB".to_string()));
    }

    #[test]
    fn e2e_filesystem_errors_without_host() {
        // TestHost doesn't implement filesystem ops, so they use defaults
        let err = compile_and_run_err(r#"Copy file from "/a" to "/b""#, json!({}));
        assert!(
            err.contains("file_copy not implemented") || err.contains("failed"),
            "got: {err}"
        );
    }

    #[test]
    fn e2e_filesystem_keywords_produce_effects() {
        // Verify that filesystem keywords record effects in the effect_table
        let package = compile_ok(
            r#"Copy file from "/a" to "/b"
Move file from "/c" to "/d"
Create directory "/e"
Delete file "/f""#,
        );
        let effects = package
            .get("effect_table")
            .and_then(Value::as_array)
            .expect("effect_table should be an array");
        let kinds: Vec<&str> = effects
            .iter()
            .filter_map(|e| e.get("kind").and_then(Value::as_str))
            .collect();
        assert!(kinds.contains(&"file_copy"), "missing file_copy effect");
        assert!(kinds.contains(&"file_move"), "missing file_move effect");
        assert!(kinds.contains(&"file_mkdir"), "missing file_mkdir effect");
        assert!(kinds.contains(&"file_delete"), "missing file_delete effect");
    }

    #[test]
    fn manifest_compiles_into_bytecode_metadata() {
        let source = r#"Permissions:
  Read files from "/inbox/"
  Write files to "/receipts/"
  HTTP requests
  Call Gmail service

Boundaries:
  No writes outside "/Users/admin/Dropbox/"

Callers:
  Any MCP client

greeting equals "hello""#;

        let package = compile_ok(source);

        let manifest = package.get("manifest").expect("manifest should be present");
        let permissions = manifest
            .get("permissions")
            .and_then(Value::as_array)
            .expect("permissions should be an array");
        assert_eq!(permissions.len(), 4);
        assert_eq!(
            permissions[0].get("kind").and_then(Value::as_str),
            Some("read_file")
        );
        assert_eq!(
            permissions[0].get("scope").and_then(Value::as_str),
            Some("/inbox/")
        );
        assert_eq!(
            permissions[3].get("kind").and_then(Value::as_str),
            Some("service_call")
        );

        let boundaries = manifest
            .get("boundaries")
            .and_then(Value::as_array)
            .expect("boundaries should be an array");
        assert_eq!(boundaries.len(), 1);

        let callers = manifest
            .get("callers")
            .and_then(Value::as_array)
            .expect("callers should be an array");
        assert_eq!(callers.len(), 1);
        assert_eq!(callers[0].as_str(), Some("Any MCP client"));
    }

    #[test]
    fn manifest_absent_when_no_declarations() {
        let package = compile_ok(r#"greeting equals "hello""#);
        assert!(
            package.get("manifest").is_none(),
            "manifest should be absent when no declarations"
        );
    }

    #[test]
    fn rule_metadata_compiles_into_manifest() {
        let source = "Rule:\n  id: credit_verification.dti_check\n  version: 2.1.0\n  author: \"Andrew Dubinsky\"\n  effective from 2026-01-01\n  effective until 2026-12-31\n\napplicant_dti equals 0.35";
        let package = compile_ok(source);
        let rule = package
            .get("manifest")
            .and_then(|m| m.get("rule"))
            .expect("manifest.rule should be present");
        assert_eq!(
            rule.get("id").and_then(Value::as_str),
            Some("credit_verification.dti_check")
        );
        assert_eq!(rule.get("version").and_then(Value::as_str), Some("2.1.0"));
        assert_eq!(
            rule.get("author").and_then(Value::as_str),
            Some("Andrew Dubinsky")
        );
        assert_eq!(
            rule.get("effective_from").and_then(Value::as_str),
            Some("2026-01-01")
        );
        assert_eq!(
            rule.get("effective_until").and_then(Value::as_str),
            Some("2026-12-31")
        );
    }

    #[test]
    fn rule_metadata_requires_only_id_and_version() {
        let source = "Rule:\n  id: pricing.tier\n  version: 1.0.0\n\nx equals 1";
        let package = compile_ok(source);
        let rule = package.get("manifest").and_then(|m| m.get("rule")).unwrap();
        assert_eq!(rule.get("id").and_then(Value::as_str), Some("pricing.tier"));
        assert!(
            rule.get("author").is_none(),
            "optional fields omitted when absent"
        );
        assert!(rule.get("effective_from").is_none());
        assert!(rule.get("effective_until").is_none());
    }

    #[test]
    fn rule_coexists_with_permissions() {
        let source =
            "Permissions:\n  Read files\n\nRule:\n  id: a.b\n  version: 1.2.3\n\nx equals 1";
        let package = compile_ok(source);
        let manifest = package.get("manifest").unwrap();
        assert_eq!(
            manifest
                .get("permissions")
                .and_then(Value::as_array)
                .map(|a| a.len()),
            Some(1)
        );
        assert_eq!(
            manifest
                .get("rule")
                .and_then(|r| r.get("version"))
                .and_then(Value::as_str),
            Some("1.2.3")
        );
    }

    #[test]
    fn rule_invalid_version_is_a_compile_error() {
        let error = compile_source_to_json(
            "Rule:\n  id: foo.bar\n  version: 2.1\n\nx equals 1",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect_err("non-semver version fails");
        let msg = error.to_string();
        assert!(msg.contains("must be semantic"), "got: {msg}");
        assert!(
            msg.contains("line 3"),
            "error should cite the version line, got: {msg}"
        );
    }

    #[test]
    fn rule_invalid_id_is_a_compile_error() {
        let error = compile_source_to_json(
            "Rule:\n  id: 9bad.id\n  version: 1.0.0\n\nx equals 1",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect_err("id starting with a digit fails");
        assert!(
            error.to_string().contains("dotted identifier"),
            "got: {error}"
        );
    }

    #[test]
    fn rule_missing_version_is_a_compile_error() {
        let error = compile_source_to_json(
            "Rule:\n  id: foo.bar\n\nx equals 1",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect_err("missing version fails");
        assert!(
            error.to_string().contains("missing 'version'"),
            "got: {error}"
        );
    }

    #[test]
    fn rule_bad_effective_date_is_a_compile_error() {
        let error = compile_source_to_json(
            "Rule:\n  id: foo.bar\n  version: 1.0.0\n  effective from 2026-13-01\n\nx equals 1",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect_err("month 13 fails");
        assert!(
            error.to_string().contains("must be YYYY-MM-DD"),
            "got: {error}"
        );
    }

    #[test]
    fn rule_effective_until_before_from_is_a_compile_error() {
        let error = compile_source_to_json(
            "Rule:\n  id: foo.bar\n  version: 1.0.0\n  effective from 2026-06-01\n  effective until 2026-01-01\n\nx equals 1",
            CompileOptions { source_path: None, search_paths: vec![] },
        )
        .expect_err("until before from fails");
        assert!(error.to_string().contains("is before"), "got: {error}");
    }

    #[test]
    fn rule_unknown_field_is_a_compile_error() {
        let error = compile_source_to_json(
            "Rule:\n  id: foo.bar\n  version: 1.0.0\n  autor: typo\n\nx equals 1",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect_err("typo'd field fails");
        assert!(
            error.to_string().contains("Unknown Rule field"),
            "got: {error}"
        );
    }

    #[test]
    fn rule_impossible_calendar_date_is_a_compile_error() {
        // Feb 31 is within 1..=31 but is not a real day.
        let error = compile_source_to_json(
            "Rule:\n  id: foo.bar\n  version: 1.0.0\n  effective from 2026-02-31\n\nx equals 1",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect_err("Feb 31 fails");
        assert!(
            error.to_string().contains("must be YYYY-MM-DD"),
            "got: {error}"
        );
        // A real leap day is accepted.
        compile_source_to_json(
            "Rule:\n  id: foo.bar\n  version: 1.0.0\n  effective from 2028-02-29\n\nx equals 1",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect("2028-02-29 is a valid leap day");
        // The same day in a non-leap year is rejected.
        compile_source_to_json(
            "Rule:\n  id: foo.bar\n  version: 1.0.0\n  effective from 2027-02-29\n\nx equals 1",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect_err("2027-02-29 is not a leap day");
    }

    #[test]
    fn rule_version_rejects_leading_zeros() {
        let error = compile_source_to_json(
            "Rule:\n  id: foo.bar\n  version: 01.02.03\n\nx equals 1",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect_err("leading-zero version fails");
        assert!(
            error.to_string().contains("must be semantic"),
            "got: {error}"
        );
    }

    #[test]
    fn rule_missing_id_is_a_compile_error_at_header_line() {
        let error = compile_source_to_json(
            "Rule:\n  version: 1.0.0\n\nx equals 1",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect_err("missing id fails");
        let msg = error.to_string();
        assert!(msg.contains("missing 'id'"), "got: {msg}");
        assert!(
            msg.contains("line 1"),
            "error should cite the Rule: header line, got: {msg}"
        );
    }

    #[test]
    fn manifest_vm_enforces_undeclared_effects() {
        // Program declares only read_file permission, then tries file_copy
        let source = r#"Permissions:
  Read files

Copy file from "/a" to "/b""#;

        let package = compile_ok(source);
        let err = run_package_err(package, json!({}));
        assert!(
            err.contains("Permission denied"),
            "expected permission denied, got: {err}"
        );
    }

    // --- DEVL-126: Set filler absorption and article stripping ---

    fn symbols(source: &str) -> Vec<String> {
        compile_ok(source)["symbol_table"]
            .as_array()
            .expect("symbol table")
            .iter()
            .map(|value| value.as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn set_absorbs_equal_to_filler_into_clean_name() {
        // Regression for DEVL-126: `set the discount equal to X` must assign
        // `discount`, not `the_discount_equal`.
        let syms = symbols("order_total equals 5000\nSet the discount equal to order_total times 0.1");
        assert!(syms.contains(&"discount".to_string()), "got: {syms:?}");
        assert!(
            !syms.iter().any(|s| s.contains("the_discount") || s.contains("equal")),
            "filler leaked into the name: {syms:?}"
        );
    }

    #[test]
    fn set_absorbs_equals_and_to_be_fillers() {
        let syms = symbols("x equals 3\nSet the total equals x\nSet the fee to be x");
        assert!(syms.contains(&"total".to_string()), "got: {syms:?}");
        assert!(syms.contains(&"fee".to_string()), "got: {syms:?}");
    }

    #[test]
    fn set_strips_leading_articles_a_an_the() {
        assert!(symbols("Set the amount to 1").contains(&"amount".to_string()));
        assert!(symbols("Set a total to 1").contains(&"total".to_string()));
        assert!(symbols("Set an invoice to 1").contains(&"invoice".to_string()));
    }

    #[test]
    fn set_value_containing_equals_still_binds_target_via_to() {
        // The earliest delimiter wins: `to` precedes `equals`, so the target is
        // `flag` and the value is the comparison `x equals 3`, not a mis-split.
        let syms = symbols("x equals 3\nSet flag to x equals 3");
        assert!(syms.contains(&"flag".to_string()), "got: {syms:?}");
    }

    #[test]
    fn underscore_escaped_name_passes_through_untouched() {
        // Escape hatch: explicit underscores make one token that never has its
        // article stripped.
        let syms = symbols("Set the_white_house to 42");
        assert!(syms.contains(&"the_white_house".to_string()), "got: {syms:?}");
    }

    #[test]
    fn single_letter_article_name_is_not_stripped_to_empty() {
        // A bare `a` used as a variable name must survive; stripping only
        // applies to multi-word names.
        let syms = symbols("a equals 5\nPrint a");
        assert!(syms.contains(&"a".to_string()), "got: {syms:?}");
    }

    // --- DEVL-127: bracket tokens are hard parse errors ---

    fn compile_err(source: &str) -> String {
        compile_source_to_json(
            source,
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect_err("expected a compile error")
        .to_string()
    }

    #[test]
    fn bracket_in_expression_is_a_hard_error() {
        let error = compile_err("line_items equals list of [1200, 450, 89]");
        assert!(error.contains("["), "got: {error}");
        assert!(error.contains("list of"), "message should suggest `list of`: {error}");
    }

    #[test]
    fn bracket_with_leading_article_is_a_hard_error() {
        // The DEVL-126 repro phrasing: `a list of [...]` must also fail loudly
        // rather than silently produce a null-filled list.
        let error = compile_err("line_items equals a list of [1200,450,89]");
        assert!(error.contains("["), "got: {error}");
    }

    #[test]
    fn brackets_inside_string_literals_are_allowed() {
        // Brackets are only rejected in expression position, not inside text.
        let syms = symbols("label equals \"[draft]\"");
        assert!(syms.contains(&"label".to_string()), "got: {syms:?}");
    }

    // --- DEVL-127: unbound-identifier lint warning ---

    fn lint(source: &str) -> Vec<LintWarning> {
        lint_source(
            source,
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect("source parses for lint")
    }

    #[test]
    fn lint_warns_on_identifier_never_bound() {
        let warnings = lint("known equals 10\ntotal equals known plus mistyped_var\nPrint total");
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert_eq!(warnings[0].line, 2);
        assert!(warnings[0].message.contains("mistyped_var"), "got: {warnings:?}");
    }

    #[test]
    fn lint_is_quiet_for_bound_identifiers() {
        // Assignments, loop variables, and Ask inputs are all bindings and must
        // not warn.
        let warnings = lint(
            "total equals 0\nFor each item in numbers:\n  Set total to total plus item\nAsk \"n?\" as answer\nPrint answer",
        );
        // `numbers` is never bound, so exactly that one should warn; item,
        // total, and answer must not. Assert the positive case first so the
        // negative assertions below are not vacuously true on an empty vector.
        let names: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(warnings.len(), 1, "expected exactly one warning: {names:?}");
        assert!(
            warnings[0].message.contains("numbers"),
            "expected the numbers warning: {names:?}"
        );
        assert!(
            !warnings.iter().any(|w| w.message.contains("'item'")
                || w.message.contains("'total'")
                || w.message.contains("'answer'")),
            "bound names warned: {names:?}"
        );
    }

    #[test]
    fn lint_warns_on_conditional_assignment_condition() {
        // DEVL-127 Fix 5: the condition of `X equals V if C` is expression-
        // bearing and must be walked. `misspelled_flag` is never bound.
        let warnings = lint("result equals 1 if misspelled_flag");
        assert_eq!(warnings.len(), 1, "got: {warnings:?}");
        assert!(
            warnings[0].message.contains("misspelled_flag"),
            "got: {warnings:?}"
        );
    }

    #[test]
    fn lint_warns_on_misspelled_respond_with_in_class_method() {
        // DEVL-127 Fix 4: `Respond with X` is lifted out of the method body into
        // return_value during class parsing, so the return value must be linted
        // separately or a typo there never warns.
        let warnings = lint(
            "Ops's Calculator:\n  total using amount:\n    Respond with misspelled_total",
        );
        assert!(
            warnings.iter().any(|w| w.message.contains("misspelled_total")),
            "expected a warning on the misspelled return value: {warnings:?}"
        );
    }

    #[test]
    fn lint_does_not_mask_typo_via_sibling_method_param() {
        // DEVL-127 Fix 6: method A's typo must warn even when method B has a
        // param of the same name. Only the current method's own params are
        // bound, plus sibling method names.
        let warnings = lint(
            "Ops's Calc:\n  first:\n    Respond with shared_value\n  second using shared_value:\n    Respond with shared_value",
        );
        assert!(
            warnings.iter().any(|w| w.message.contains("shared_value")),
            "method A's reference to shared_value should warn: {warnings:?}"
        );
    }

    #[test]
    fn class_method_import_shadowing_param_is_an_error() {
        // DEVL-127 Fix 3: an imported fragment that defines a name matching a
        // caller-supplied method param is a hard collision, not a silent
        // overwrite.
        let dir = std::env::temp_dir().join("devlish_test_param_shadow");
        std::fs::create_dir_all(&dir).unwrap();
        let helper_path = dir.join("frag.dvl");
        std::fs::write(&helper_path, "approved equals true").unwrap();

        let source = format!(
            "Ops's Approver:\n  decide using approved:\n    Import \"{}\"\n    Respond with approved",
            helper_path.to_string_lossy()
        );
        let error = compile_source_to_json(
            &source,
            CompileOptions {
                source_path: Some(dir.join("main.dvl").to_string_lossy().to_string()),
                search_paths: vec![],
            },
        )
        .expect_err("param shadow should fail");
        assert!(
            error.to_string().contains("Import name collision: approved"),
            "got: {error}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    // --- DEVL-123: class imports before the first method header ---

    #[test]
    fn class_indented_import_before_first_method_keeps_methods() {
        // A class-level import indented above the first method header must be
        // dropped (not turned into lines[0]) so method detection still works and
        // the class compiles with its methods intact.
        let dir = std::env::temp_dir().join("devlish_test_class_pre_import");
        std::fs::create_dir_all(&dir).unwrap();
        let helper_path = dir.join("shared.dvl");
        std::fs::write(&helper_path, "shared_rate equals 5").unwrap();

        let source = format!(
            "Ops's Pricer:\n  Import \"{}\"\n  price using base:\n    Respond with base",
            helper_path.to_string_lossy()
        );
        let package = compile_source_to_json(
            &source,
            CompileOptions {
                source_path: Some(dir.join("main.dvl").to_string_lossy().to_string()),
                search_paths: vec![],
            },
        )
        .expect("class with pre-method import compiles");
        assert!(
            package.contains("price"),
            "method 'price' should survive: {package}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn class_top_level_import_still_compiles() {
        // Regression for the preserved branch: a genuinely top-level (indent-0)
        // import in a class program is dropped and the class still compiles.
        let source =
            "Import \"nonexistent_ignored.dvl\"\nOps's Greeter:\n  greet:\n    Respond with \"hi\"";
        // The top-level import line is skipped before resolution, so a missing
        // path does not error here.
        let package = compile_source_to_json(
            source,
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect("class with top-level import compiles");
        assert!(package.contains("greet"), "method 'greet' should survive: {package}");
    }

    // --- DEVL-127 Fix 1: apostrophe must not disarm the bracket guard ---

    #[test]
    fn possessive_apostrophe_does_not_bypass_bracket_guard() {
        let error = compile_err("note equals buyer's total plus [1, 2]");
        assert!(error.contains("["), "possessive should not open a string: {error}");
    }

    #[test]
    fn single_quotes_are_not_string_delimiters() {
        // Single quotes are ordinary text, never string delimiters. Brackets
        // that used to hide inside a single-quoted "string" now fail loudly,
        // and string literals require double quotes.
        let error = compile_err("label equals '[draft]'");
        assert!(error.contains("["), "got: {error}");
        let result = compile_and_run_ok("label equals \"[draft]\"", json!({}));
        assert_eq!(
            result.get("context").and_then(|c| c.get("label")),
            Some(&json!("[draft]")),
            "got: {result:?}"
        );
    }

    #[test]
    fn possessive_folds_into_name_with_underscore_connector() {
        // `Set salesperson's commission to 5` binds salesperson_commission,
        // readable back as `salesperson commission` (same sanitized name).
        assert_eq!(sanitize_name("salesperson's commission"), "salesperson_commission");
        assert_eq!(sanitize_name("owners' equity"), "owners_equity");
        assert_eq!(sanitize_name("o'brien total"), "obrien_total");
        // Contractions: bare apostrophe drops (the s is mid-word, not a
        // possessive marker). Uppercase possessive folds the same way.
        assert_eq!(sanitize_name("don't stop"), "dont_stop");
        assert_eq!(sanitize_name("OWNER'S total"), "owner_total");
        let result = compile_and_run_ok(
            "Set salesperson's commission to 5\nSet r to salesperson commission",
            json!({}),
        );
        assert_eq!(
            result.get("context").and_then(|c| c.get("r")),
            Some(&json!(5)),
            "got: {result:?}"
        );
    }

    #[test]
    fn trailing_if_condition_works_after_possessive() {
        // Regression: split_trailing_if treated the possessive apostrophe as
        // a string opener and never found the trailing ` if `.
        let source = "Use the math module.\nflag equals true\nr equals math's pi if flag";
        let result = compile_and_run_ok(source, json!({}));
        assert_eq!(
            result.get("context").and_then(|c| c.get("r")),
            Some(&json!(std::f64::consts::PI)),
            "got: {result:?}"
        );
    }

    #[test]
    fn bracket_after_closed_double_quoted_string_is_rejected() {
        let error = compile_err("x equals \"done\" plus [1]");
        assert!(error.contains("["), "got: {error}");
    }

    #[test]
    fn escaped_quote_inside_string_with_brackets_is_allowed() {
        let syms = symbols("msg equals \"a \\\" [b]\"");
        assert!(syms.contains(&"msg".to_string()), "got: {syms:?}");
    }

    #[test]
    fn import_path_with_brackets_skips_bracket_guard() {
        // DEVL-127 Fix 15: an import path may carry brackets; the guard must not
        // reject the line before it is classified as an import.
        let err = compile_err("Import rules[2026].dvl\nx equals 1");
        assert!(
            err.contains("Import not found"),
            "bracket guard should not fire on an import line: {err}"
        );
    }

    // --- DEVL-126 Fix 7: article stripping keyed on capitalization ---

    #[test]
    fn capitalized_article_is_preserved_in_name() {
        assert!(symbols("Set exhibit A to 1").contains(&"exhibit_a".to_string()));
        assert!(symbols("Set The Hague to 1").contains(&"the_hague".to_string()));
    }

    #[test]
    fn lowercase_article_is_stripped_from_name() {
        assert!(symbols("Set the discount to 1").contains(&"discount".to_string()));
    }

    #[test]
    fn total_for_the_year_strips_to_total_for_year() {
        // Documented example: lowercase `the` is dropped from a multi-word name.
        let syms = symbols("Set total for the year to 1");
        assert!(syms.contains(&"total_for_year".to_string()), "got: {syms:?}");
    }

    // --- DEVL-126 Fix 12: Set delimiter and filler behavior ---

    #[test]
    fn set_with_no_delimiter_is_not_a_binding() {
        // `Set x` has no target/value delimiter, so it is never treated as a
        // SetField binding. It falls through to an unsupported-statement error
        // rather than silently binding `x`.
        let error = compile_err("Set x");
        assert!(
            error.to_lowercase().contains("set x") || error.contains("Unsupported"),
            "got: {error}"
        );
    }

    #[test]
    fn set_mixed_case_fillers_are_absorbed() {
        let syms = symbols("SET the total EQUAL TO 1");
        assert!(syms.contains(&"total".to_string()), "got: {syms:?}");
    }

    #[test]
    fn set_fee_to_be_binds_value_seven_not_be_seven() {
        // Tie-break: the `to` delimiter wins over the `to be` filler, and the
        // value is `7`, not `be 7`.
        let result = compile_and_run_ok("Set fee to be 7", json!({}));
        assert_eq!(
            result.get("context").and_then(|c| c.get("fee")),
            Some(&json!(7)),
            "got: {result:?}"
        );
    }

    #[test]
    fn all_article_multi_word_name_survives() {
        // If every word is an article the name is kept rather than emptied.
        let syms = symbols("Set the a an to 1");
        assert!(
            syms.iter().any(|s| !s.is_empty() && s != "1"),
            "an all-article name should survive as some non-empty symbol: {syms:?}"
        );
    }

    // --- DEVL-131: module namespaces + bundled stdlib ---

    #[test]
    fn use_math_module_gives_qualified_access() {
        let result = compile_and_run_ok("Use the math module.\nSet r to math's pi", json!({}));
        assert_eq!(
            result.get("context").and_then(|c| c.get("r")),
            Some(&json!(std::f64::consts::PI)),
            "got: {result:?}"
        );
    }

    #[test]
    fn use_selective_binds_unqualified_symbol() {
        let result =
            compile_and_run_ok("Use pi and tau from the math module\nSet r to tau", json!({}));
        assert_eq!(
            result.get("context").and_then(|c| c.get("r")),
            Some(&json!(std::f64::consts::TAU)),
            "got: {result:?}"
        );
    }

    #[test]
    fn whole_module_use_does_not_pollute_flat_namespace() {
        // A local `pi` coexists with the module's; qualified access still
        // reaches the module value.
        let source = "Use the math module.\npi equals 1\nSet local to pi\nSet stdlib to math's pi";
        let result = compile_and_run_ok(source, json!({}));
        let context = result.get("context").expect("context");
        assert_eq!(context.get("local"), Some(&json!(1)));
        assert_eq!(context.get("stdlib"), Some(&json!(std::f64::consts::PI)));
    }

    #[test]
    fn qualified_ref_without_use_is_a_compile_error() {
        let error = compile_err("Set r to math's pi");
        assert!(error.contains("Unknown module"), "got: {error}");
    }

    #[test]
    fn unknown_module_is_a_compile_error() {
        let error = compile_err("Use the nosuch module.");
        assert!(error.contains("Unknown module: nosuch"), "got: {error}");
    }

    #[test]
    fn unknown_selective_symbol_is_a_compile_error() {
        let error = compile_err("Use nope from the math module");
        assert!(error.contains("does not define 'nope'"), "got: {error}");
    }

    #[test]
    fn unknown_qualified_symbol_is_a_compile_error() {
        let error = compile_err("Use the math module.\nSet r to math's nope");
        assert!(error.contains("does not define 'nope'"), "got: {error}");
    }

    #[test]
    fn repeated_use_of_module_inlines_once_and_composes_with_selective() {
        // `Use the math module` + `Use tau from the math module` mirrors
        // Python's `import math` + `from math import tau`.
        let source =
            "Use the math module.\nUse tau from the math module\nSet a to tau\nSet b to math's e";
        let result = compile_and_run_ok(source, json!({}));
        let context = result.get("context").expect("context");
        assert_eq!(context.get("a"), Some(&json!(std::f64::consts::TAU)));
        assert_eq!(context.get("b"), Some(&json!(std::f64::consts::E)));
    }

    #[test]
    fn duplicate_selective_use_symbol_is_a_collision_error() {
        let error =
            compile_err("Use pi from the math module\nUse pi from the math module");
        assert!(error.contains("collision"), "got: {error}");
    }

    #[test]
    fn selective_use_collision_with_local_symbol_errors() {
        let error = compile_err("pi equals 1\nUse pi from the math module");
        assert!(error.contains("collision"), "got: {error}");
    }

    #[test]
    fn stdlib_use_recorded_in_package_metadata() {
        let json_text = compile_source_to_json(
            "Use the math module.\nSet r to math's pi",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect("compiles");
        let package: Value = serde_json::from_str(&json_text).expect("valid json");
        assert_eq!(
            package.pointer("/stdlib/version"),
            Some(&json!(STDLIB_VERSION))
        );
        assert_eq!(package.pointer("/stdlib/modules"), Some(&json!(["math"])));
        let files = package
            .get("source_files")
            .and_then(Value::as_array)
            .expect("source_files present");
        assert!(
            files
                .iter()
                .any(|f| f.get("path") == Some(&json!("stdlib:math.dvl"))),
            "got: {files:?}"
        );
    }

    /// Creates a unique module dir for this test (pid alone is shared by every
    /// test in one cargo-test process). Caller cleans up best-effort; a
    /// leaked dir from a panicking run cannot collide with later runs.
    fn unique_module_dir(test_name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .subsec_nanos();
        let dir = std::env::temp_dir().join(format!(
            "devlish_{test_name}_{}_{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create temp module dir");
        dir
    }

    #[test]
    fn file_module_resolves_via_search_path_with_trailing_apostrophe() {
        let dir = unique_module_dir("use_module");
        let module_path = dir.join("units.dvl");
        std::fs::write(&module_path, "meter equals 1\nkilometer equals 1000\n")
            .expect("write module");

        let json_text = compile_source_to_json(
            "Use the units module.\nSet r to units' kilometer",
            CompileOptions {
                source_path: None,
                search_paths: vec![dir.to_string_lossy().to_string()],
            },
        )
        .expect("compiles");
        let package: Value = serde_json::from_str(&json_text).expect("valid json");
        // A file-based (non-bundled) module must NOT claim stdlib provenance.
        assert!(package.get("stdlib").is_none(), "file module is not stdlib");
        let mut host = TestHost::new();
        let mut vm = Vm::new(package, json!({})).expect("vm init");
        let result = vm.run(&mut host).expect("runs");
        assert_eq!(
            result.get("context").and_then(|c| c.get("r")),
            Some(&json!(1000)),
            "got: {result:?}"
        );

        std::fs::remove_file(&module_path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn qualified_ref_composes_with_arithmetic_operators() {
        // Regression: the binary-operator splitter used to treat the
        // possessive apostrophe as a string opener, so everything right of
        // `math's` was "inside a quote" and never split (review of DEVL-131).
        let result = compile_and_run_ok("Use the math module.\nSet r to math's tau times 2", json!({}));
        assert_eq!(
            result.get("context").and_then(|c| c.get("r")),
            Some(&json!(std::f64::consts::TAU * 2.0)),
            "got: {result:?}"
        );
    }

    #[test]
    fn apostrophe_inside_string_literal_is_not_a_qualified_ref() {
        let result = compile_and_run_ok("Set msg to \"math's pi\"", json!({}));
        assert_eq!(
            result.get("context").and_then(|c| c.get("msg")),
            Some(&json!("math's pi")),
            "got: {result:?}"
        );
    }

    #[test]
    fn nested_use_inside_block_is_a_compile_error() {
        let error = compile_err("If flag:\n  Use pi from the math module\n  Set r to pi");
        assert!(error.contains("top level"), "got: {error}");
        // Whole-module form inside a loop body and a Try block error the same
        // way (recursion through every child_statement_blocks arm).
        let error = compile_err("While flag:\n  Use the math module.");
        assert!(error.contains("top level"), "got: {error}");
        let error = compile_err("Try:\n  Use the math module.\nOtherwise:\n  Print \"x\"");
        assert!(error.contains("top level"), "got: {error}");
    }

    #[test]
    fn unquote_handles_lone_and_empty_double_quotes() {
        // The len() >= 2 guard: a lone `"` must not slice-panic.
        assert_eq!(unquote("\""), "\"");
        assert_eq!(unquote("\"\""), "");
        assert_eq!(unquote("\"x\""), "x");
    }

    #[test]
    fn set_possessive_on_used_module_binds_local_name_not_module() {
        // Pinned semantics: a Set target ALWAYS folds to a local name, even
        // when the owner is a Used module. Module values are read-only;
        // `Set math's pi to 5` binds local `math_pi` and qualified reads of
        // `math's pi` still see the module constant.
        let source = "Use the math module.\nSet math's pi to 5\nSet local to math pi\nSet stdlib to math's pi";
        let result = compile_and_run_ok(source, json!({}));
        let context = result.get("context").expect("context");
        assert_eq!(context.get("math_pi"), Some(&json!(5)));
        assert_eq!(context.get("local"), Some(&json!(5)));
        assert_eq!(context.get("stdlib"), Some(&json!(std::f64::consts::PI)));
    }

    #[test]
    fn mangling_is_injective_for_double_underscore_names() {
        // ("a__b", "c") and ("a", "b__c") must resolve to different symbols.
        let dir = unique_module_dir("mangle_injective");
        std::fs::write(dir.join("a.dvl"), "b__c equals 222\n").expect("write a");
        std::fs::write(dir.join("a__b.dvl"), "c equals 111\n").expect("write a__b");

        let json_text = compile_source_to_json(
            "Use the a module.\nUse the a__b module.\nSet x to a's b__c\nSet y to a__b's c",
            CompileOptions {
                source_path: None,
                search_paths: vec![dir.to_string_lossy().to_string()],
            },
        )
        .expect("compiles");
        let package: Value = serde_json::from_str(&json_text).expect("valid json");
        let mut host = TestHost::new();
        let mut vm = Vm::new(package, json!({})).expect("vm init");
        let result = vm.run(&mut host).expect("runs");
        let context = result.get("context").expect("context");
        assert_eq!(context.get("x"), Some(&json!(222)), "a's b__c");
        assert_eq!(context.get("y"), Some(&json!(111)), "a__b's c");

        std::fs::remove_file(dir.join("a.dvl")).ok();
        std::fs::remove_file(dir.join("a__b.dvl")).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn whole_module_use_cycle_compiles_and_selective_cycle_errors() {
        let dir = unique_module_dir("use_cycle");
        std::fs::write(dir.join("ay.dvl"), "Use the bee module.\naval equals 1\n")
            .expect("write ay");
        std::fs::write(dir.join("bee.dvl"), "Use the ay module.\nbval equals 2\n")
            .expect("write bee");
        // Whole-module cycle: back-edge dropped; the entry module's exports
        // still evaluate through the VM.
        let json_text = compile_source_to_json(
            "Use the ay module.\nSet r to ay's aval",
            CompileOptions {
                source_path: None,
                search_paths: vec![dir.to_string_lossy().to_string()],
            },
        )
        .expect("whole-module cycle should compile");
        let package: Value = serde_json::from_str(&json_text).expect("valid json");
        let mut host = TestHost::new();
        let mut vm = Vm::new(package, json!({})).expect("vm init");
        let result = vm.run(&mut host).expect("runs");
        assert_eq!(
            result.get("context").and_then(|c| c.get("r")),
            Some(&json!(1)),
            "got: {result:?}"
        );

        // A module that qualified-references its cyclic partner across the
        // dropped back-edge cannot resolve it: hard error, not silent null.
        std::fs::write(
            dir.join("bee.dvl"),
            "Use the ay module.\nx equals ay's aval\nbval equals 2\n",
        )
        .expect("rewrite bee referencing back-edge");
        let error = compile_source_to_json(
            "Use the ay module.\nSet r to ay's aval",
            CompileOptions {
                source_path: None,
                search_paths: vec![dir.to_string_lossy().to_string()],
            },
        )
        .expect_err("back-edge qualified ref must error")
        .to_string();
        assert!(error.contains("Unknown module 'ay'"), "got: {error}");

        // Selective bind through a cycle back-edge cannot be honored: error.
        std::fs::write(
            dir.join("bee.dvl"),
            "Use aval from the ay module\nbval equals 2\n",
        )
        .expect("rewrite bee");
        let error = compile_source_to_json(
            "Use the ay module.\nSet r to ay's aval",
            CompileOptions {
                source_path: None,
                search_paths: vec![dir.to_string_lossy().to_string()],
            },
        )
        .expect_err("selective cycle must error")
        .to_string();
        assert!(error.contains("Circular Use"), "got: {error}");

        std::fs::remove_file(dir.join("ay.dvl")).ok();
        std::fs::remove_file(dir.join("bee.dvl")).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn use_statement_parse_errors_are_loud() {
        let error = compile_err("Use math module");
        assert!(error.contains("Malformed Use statement"), "got: {error}");
        let error = compile_err("Use from the math module");
        assert!(error.contains("Malformed Use statement"), "got: {error}");
        let error = compile_err("Use , from the math module");
        assert!(error.contains("no symbols"), "got: {error}");
    }

    #[test]
    fn selective_symbol_containing_from_the_splits_at_last_occurrence() {
        // `distance from the sun` is the symbol; `astronomy` is the module.
        let error = compile_err("Use distance from the sun from the astronomy module");
        assert!(error.contains("Unknown module: astronomy"), "got: {error}");
    }

    #[test]
    fn class_method_can_use_module_and_return_qualified_ref() {
        let source = "Circle's Area Calculator:\n  compute using radius:\n    Use the math module.\n    area equals math's pi times radius times radius\n    respond with area";
        let json_text = compile_source_to_json(
            source,
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect("class program with Use compiles");
        let package: Value = serde_json::from_str(&json_text).expect("valid json");
        assert!(package.get("methods").is_some(), "compiled as class program");
    }

    #[test]
    fn transitive_module_use_is_not_visible_without_direct_use() {
        // helper Uses math; the main program does NOT. `math's pi` must be a
        // compile error in main, while helper's own qualified refs work.
        let dir = unique_module_dir("transitive_use");
        std::fs::write(
            dir.join("helper.dvl"),
            "Use the math module.\ndouble_pi equals math's pi times 2\n",
        )
        .expect("write helper");

        let options = |dir: &std::path::Path| CompileOptions {
            source_path: None,
            search_paths: vec![dir.to_string_lossy().to_string()],
        };
        // Direct access through helper's transitive Use must fail.
        let error = compile_source_to_json(
            "Use the helper module.\nSet r to math's pi",
            options(&dir),
        )
        .expect_err("transitive qualified ref must error")
        .to_string();
        assert!(error.contains("Unknown module 'math'"), "got: {error}");

        // helper's exported value (computed FROM math) is reachable.
        let json_text = compile_source_to_json(
            "Use the helper module.\nSet r to helper's double_pi",
            options(&dir),
        )
        .expect("compiles");
        let package: Value = serde_json::from_str(&json_text).expect("valid json");
        let mut host = TestHost::new();
        let mut vm = Vm::new(package, json!({})).expect("vm init");
        let result = vm.run(&mut host).expect("runs");
        assert_eq!(
            result.get("context").and_then(|c| c.get("r")),
            Some(&json!(std::f64::consts::PI * 2.0)),
            "got: {result:?}"
        );

        std::fs::remove_file(dir.join("helper.dvl")).ok();
        std::fs::remove_dir(&dir).ok();
    }

    // `:` is illegal in Windows filenames; the spoof fixture cannot exist there.
    #[cfg(unix)]
    #[test]
    fn import_of_stdlib_prefixed_path_is_rejected() {
        let dir = unique_module_dir("stdlib_spoof");
        let spoof = dir.join("stdlib:math.dvl");
        std::fs::write(&spoof, "pi equals 99\n").expect("write spoof");

        let error = compile_source_to_json(
            "Import \"stdlib:math.dvl\"\nSet r to pi",
            CompileOptions {
                source_path: Some(dir.join("main.dvl").to_string_lossy().to_string()),
                search_paths: vec![dir.to_string_lossy().to_string()],
            },
        )
        .expect_err("stdlib-prefixed import path must be rejected")
        .to_string();
        assert!(error.contains("reserved"), "got: {error}");

        std::fs::remove_file(&spoof).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn lint_is_quiet_for_use_and_qualified_refs() {
        let warnings = lint_source(
            "Use the math module.\nUse tau from the math module\nSet r to math's pi plus tau\nPrint r",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect("lints");
        assert!(warnings.is_empty(), "got: {warnings:?}");
    }

    #[test]
    fn selective_use_invalid_module_name_is_a_compile_error() {
        // The module part of a selective Use must be a single identifier.
        let error = compile_err("Use pi from the two words module");
        assert!(
            error.contains("Invalid module name in Use statement"),
            "got: {error}"
        );
    }

    #[test]
    fn selective_use_collision_with_method_param_errors() {
        // A class method's params are reserved; a selective Use binding the
        // same name must fail loudly instead of silently shadowing.
        let source = "Circle's Area Calculator:\n  compute using pi:\n    Use pi from the math module\n    respond with pi";
        let error = compile_source_to_json(
            source,
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect_err("param collision must error")
        .to_string();
        assert!(error.contains("collision"), "got: {error}");
    }

    #[test]
    fn flat_import_makes_imported_files_use_visible() {
        // A flat Import is a textual include: a module the imported file Used
        // is visible for qualified refs in the importer.
        let dir = unique_module_dir("import_use_visible");
        std::fs::write(dir.join("helper.dvl"), "Use the math module.\nhelped equals 1\n")
            .expect("write helper");

        let json_text = compile_source_to_json(
            "Import \"helper.dvl\"\nSet r to math's pi",
            CompileOptions {
                source_path: Some(dir.join("main.dvl").to_string_lossy().to_string()),
                search_paths: vec![dir.to_string_lossy().to_string()],
            },
        )
        .expect("import-carried Use compiles");
        let package: Value = serde_json::from_str(&json_text).expect("valid json");
        let mut host = TestHost::new();
        let mut vm = Vm::new(package, json!({})).expect("vm init");
        let result = vm.run(&mut host).expect("runs");
        assert_eq!(
            result.get("context").and_then(|c| c.get("r")),
            Some(&json!(std::f64::consts::PI)),
            "got: {result:?}"
        );

        std::fs::remove_file(dir.join("helper.dvl")).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn class_method_return_value_resolves_qualified_ref() {
        // resolve_qualified_refs_in_expression: the method's return value may
        // itself be a qualified ref (success), and an unknown symbol there is
        // reported against the method (error).
        let options = || CompileOptions {
            source_path: None,
            search_paths: vec![],
        };
        let ok = "Circle's Constant Provider:\n  compute using radius:\n    Use the math module.\n    respond with math's pi";
        assert!(
            compile_source_to_json(ok, options()).is_ok(),
            "qualified ref in return value should compile"
        );

        let bad = "Circle's Constant Provider:\n  compute using radius:\n    Use the math module.\n    respond with math's nope";
        let error = compile_source_to_json(bad, options())
            .expect_err("unknown symbol in return value must error")
            .to_string();
        assert!(error.contains("does not define 'nope'"), "got: {error}");
    }

    #[test]
    fn qualified_ref_inside_branch_block_resolves() {
        // resolve_qualified_refs must recurse into child statement blocks.
        let source = "Use the math module.\nflag equals true\nSet r to 0\nIf flag:\n  Set r to math's pi";
        let result = compile_and_run_ok(source, json!({}));
        assert_eq!(
            result.get("context").and_then(|c| c.get("r")),
            Some(&json!(std::f64::consts::PI)),
            "got: {result:?}"
        );
    }

    #[test]
    fn single_quoted_import_path_is_no_longer_unquoted() {
        // Single quotes are no longer trimmed from Import paths (double
        // quotes only); the quoted text is taken verbatim and not found.
        let error = compile_err("Import 'nope.dvl'");
        assert!(error.contains("Import not found"), "got: {error}");
    }

    #[test]
    fn module_with_control_flow_renames_symbols_inside_blocks() {
        // rename_symbols_in_statement must recurse into loop bodies: the
        // module's counter is read AND written inside a While block, and both
        // sides must move to the mangled name together.
        let dir = unique_module_dir("rename_blocks");
        std::fs::write(
            dir.join("counter.dvl"),
            "count equals 0\nWhile count is less than 3:\n  Set count to count plus 1\n",
        )
        .expect("write counter");

        let json_text = compile_source_to_json(
            "Use the counter module.\nSet r to counter's count",
            CompileOptions {
                source_path: None,
                search_paths: vec![dir.to_string_lossy().to_string()],
            },
        )
        .expect("compiles");
        let package: Value = serde_json::from_str(&json_text).expect("valid json");
        let mut host = TestHost::new();
        let mut vm = Vm::new(package, json!({})).expect("vm init");
        let result = vm.run(&mut host).expect("runs");
        assert_eq!(
            result.get("context").and_then(|c| c.get("r")),
            Some(&json!(3)),
            "got: {result:?}"
        );

        std::fs::remove_file(dir.join("counter.dvl")).ok();
        std::fs::remove_dir(&dir).ok();
    }

    // DEVL-132: callback-style collection operations compiled as inline loops.

    fn context_of(result: &Value) -> &Value {
        result.get("context").expect("context")
    }

    #[test]
    fn e2e_map_with_expression_body() {
        let result = compile_and_run_ok(
            "xs equals list of 1, 2, 3\ndoubled equals map xs to item times 2",
            json!({}),
        );
        assert_eq!(context_of(&result).get("doubled"), Some(&json!([2, 4, 6])));
    }

    #[test]
    fn e2e_map_with_record_field_expression() {
        let result = compile_and_run_ok(
            "rows equals list of record with 3 as amount, record with 5 as amount\n\
             totals equals map rows to amount of item times 2",
            json!({}),
        );
        assert_eq!(context_of(&result).get("totals"), Some(&json!([6, 10])));
    }

    #[test]
    fn e2e_filter_with_expression_predicate() {
        let result = compile_and_run_ok(
            "xs equals list of 1, 2, 3, 4\nkept equals filter xs where item times 2 > 4",
            json!({}),
        );
        assert_eq!(context_of(&result).get("kept"), Some(&json!([3, 4])));
    }

    #[test]
    fn e2e_reject_with_compound_predicate() {
        let result = compile_and_run_ok(
            "xs equals list of 1, 2, 3, 4\nkept equals reject xs where item > 1 and item < 4",
            json!({}),
        );
        assert_eq!(context_of(&result).get("kept"), Some(&json!([1, 4])));
    }

    #[test]
    fn e2e_find_with_expression_predicate() {
        let result = compile_and_run_ok(
            "rows equals list of record with 3 as amount, record with 5 as amount\n\
             found equals find rows where amount of item > 4",
            json!({}),
        );
        assert_eq!(
            context_of(&result).get("found"),
            Some(&json!({"amount": 5}))
        );
    }

    #[test]
    fn e2e_find_returns_null_when_nothing_matches() {
        let result = compile_and_run_ok(
            "xs equals list of 1, 2\nfound equals find xs where item > 10",
            json!({}),
        );
        assert_eq!(context_of(&result).get("found"), Some(&Value::Null));
    }

    #[test]
    fn e2e_any_and_all_with_expression_predicates() {
        let result = compile_and_run_ok(
            "xs equals list of 1, 2, 3\n\
             has_big equals any of xs where item times 2 > 5\n\
             all_positive equals all of xs where item > 0\n\
             all_big equals all of xs where item > 2",
            json!({}),
        );
        let context = context_of(&result);
        assert_eq!(context.get("has_big"), Some(&json!(true)));
        assert_eq!(context.get("all_positive"), Some(&json!(true)));
        assert_eq!(context.get("all_big"), Some(&json!(false)));
    }

    #[test]
    fn e2e_bare_any_and_all_use_element_truthiness() {
        let result = compile_and_run_ok(
            "flags equals list of true, false\n\
             some_set equals any of flags\n\
             every_set equals all of flags",
            json!({}),
        );
        let context = context_of(&result);
        assert_eq!(context.get("some_set"), Some(&json!(true)));
        assert_eq!(context.get("every_set"), Some(&json!(false)));
    }

    #[test]
    fn e2e_general_reduce_sums_elements() {
        let result = compile_and_run_ok(
            "xs equals list of 1, 2, 3\n\
             total equals reduce xs starting at 0 with total and item to total plus item",
            json!({}),
        );
        assert_eq!(context_of(&result).get("total"), Some(&json!(6)));
    }

    #[test]
    fn e2e_general_reduce_with_custom_binding_names() {
        let result = compile_and_run_ok(
            "words equals list of \"a\", \"bb\"\n\
             joined equals reduce words starting at \"x\" with acc and word to acc plus word",
            json!({}),
        );
        assert_eq!(context_of(&result).get("joined"), Some(&json!("xabb")));
    }

    #[test]
    fn e2e_reduce_count_fast_path_still_used() {
        let result = compile_and_run_ok(
            "xs equals list of 1, 2, 3\n\
             n equals reduce xs starting at 0 with total and item to total plus 1",
            json!({}),
        );
        assert_eq!(context_of(&result).get("n"), Some(&json!(3)));
    }

    #[test]
    fn e2e_sort_by_expression_key() {
        let result = compile_and_run_ok(
            "rows equals list of record with 3 as amount, record with 5 as amount\n\
             descending equals sort rows by 0 minus amount of item",
            json!({}),
        );
        assert_eq!(
            context_of(&result).get("descending"),
            Some(&json!([{"amount": 5}, {"amount": 3}]))
        );
    }

    #[test]
    fn e2e_sort_by_plain_field_name_unchanged() {
        let result = compile_and_run_ok(
            "rows equals list of record with 5 as amount, record with 3 as amount\n\
             ascending equals sort rows by amount",
            json!({}),
        );
        assert_eq!(
            context_of(&result).get("ascending"),
            Some(&json!([{"amount": 3}, {"amount": 5}]))
        );
    }

    #[test]
    fn e2e_field_predicate_fast_path_unchanged() {
        let result = compile_and_run_ok(
            "rows equals list of record with 3 as amount, record with 5 as amount\n\
             big equals filter rows where amount >= 4",
            json!({}),
        );
        assert_eq!(
            context_of(&result).get("big"),
            Some(&json!([{"amount": 5}]))
        );
    }

    #[test]
    fn e2e_nested_map_inside_filter_predicate_list() {
        let result = compile_and_run_ok(
            "xs equals list of 1, 2, 3\n\
             tripled equals map xs to item times 3\n\
             kept equals filter tripled where item > 4",
            json!({}),
        );
        assert_eq!(context_of(&result).get("kept"), Some(&json!([6, 9])));
    }

    fn compile_and_run_class(source: &str) -> Value {
        let json = compile_source_to_json(
            source,
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect("class compiles");
        let package: Value = serde_json::from_str(&json).unwrap();
        let mut host = TestHost::new();
        let mut vm = Vm::new(package, json!({})).expect("VM init should succeed");
        vm.run(&mut host).expect("expected successful run")
    }

    #[test]
    fn e2e_method_call_inlines_sibling_method() {
        let result = compile_and_run_class(
            "Ops's Calc:\n\
             \x20 main:\n\
             \x20   respond with double using 21\n\
             \x20 double using n:\n\
             \x20   respond with n times 2",
        );
        assert_eq!(result["context"]["__return__"], json!(42), "got: {result:?}");
    }

    #[test]
    fn e2e_map_using_helper_method() {
        let result = compile_and_run_class(
            "Ops's Calc:\n\
             \x20 main:\n\
             \x20   xs equals list of 1, 2, 3\n\
             \x20   respond with map xs using double\n\
             \x20 double using n:\n\
             \x20   respond with n times 2",
        );
        assert_eq!(
            result["context"]["__return__"],
            json!([2, 4, 6]),
            "got: {result:?}"
        );
    }

    #[test]
    fn e2e_filter_using_helper_method() {
        let result = compile_and_run_class(
            "Ops's Calc:\n\
             \x20 main:\n\
             \x20   xs equals list of 1, 2, 3, 4\n\
             \x20   respond with filter xs using is big\n\
             \x20 is big using n:\n\
             \x20   big equals yes if n > 2\n\
             \x20   respond with big",
        );
        assert_eq!(
            result["context"]["__return__"],
            json!([3, 4]),
            "got: {result:?}"
        );
    }

    #[test]
    fn e2e_inlined_helper_locals_do_not_clobber_caller() {
        let result = compile_and_run_class(
            "Ops's Calc:\n\
             \x20 main:\n\
             \x20   temp equals 1\n\
             \x20   r equals double using 5\n\
             \x20   respond with temp plus r\n\
             \x20 double using n:\n\
             \x20   temp equals n times 2\n\
             \x20   respond with temp",
        );
        assert_eq!(result["context"]["__return__"], json!(11), "got: {result:?}");
    }

    #[test]
    fn recursive_method_calls_are_rejected_at_compile_time() {
        let error = compile_source_to_json(
            "Ops's Calc:\n\
             \x20 spiral using n:\n\
             \x20   respond with spiral using n",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect_err("recursive method fails");
        assert!(
            error.to_string().contains("recursion is not supported"),
            "got: {error}"
        );
    }

    #[test]
    fn mutually_recursive_method_calls_are_rejected() {
        let error = compile_source_to_json(
            "Ops's Calc:\n\
             \x20 ping using n:\n\
             \x20   respond with pong using n\n\
             \x20 pong using n:\n\
             \x20   respond with ping using n",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect_err("mutual recursion fails");
        assert!(
            error.to_string().contains("recursion is not supported"),
            "got: {error}"
        );
    }

    #[test]
    fn class_method_constants_and_jumps_are_rebased_after_concatenation() {
        // Methods compile with fresh compilers, so constant indices and jump
        // targets are method-relative. The second method must still resolve
        // its own constants through the shared pool and jump within itself.
        let json = compile_source_to_json(
            "Ops's Calc:\n\
             \x20 first:\n\
             \x20   respond with 1\n\
             \x20 second using n:\n\
             \x20   r equals 0\n\
             \x20   If n > 5:\n\
             \x20     r equals 42\n\
             \x20   respond with r",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect("class compiles");
        let package: Value = serde_json::from_str(&json).unwrap();
        let constants = package["constant_pool"].as_array().unwrap();
        let instructions = package["instructions"].as_array().unwrap();
        let methods = package["methods"].as_array().unwrap();
        let second_entry = methods[1]["entry_point"].as_u64().unwrap() as usize;
        assert!(second_entry > 0);

        let second = &instructions[second_entry..];
        let resolved: Vec<&Value> = second
            .iter()
            .filter(|i| i["op"] == json!("CONST"))
            .map(|i| &constants[i["const"].as_u64().unwrap() as usize])
            .collect();
        assert!(
            resolved.contains(&&json!(42)),
            "second method's CONSTs must reach 42 through the shared pool, resolved: {resolved:?}"
        );
        for instruction in second {
            let op = instruction["op"].as_str().unwrap();
            if op == "JUMP" || op == "JUMP_IF_FALSE" {
                let target = instruction["target"].as_u64().unwrap() as usize;
                assert!(
                    target >= second_entry && target <= instructions.len(),
                    "jump target {target} escapes the second method (entry {second_entry})"
                );
            }
        }
    }

    // DEVL-133: regex primitive.

    #[test]
    fn e2e_regex_test_in_condition() {
        let result = compile_and_run_ok(
            "code equals \"AB-123\"\n\
             ok equals yes if code matches the pattern \"^[A-Z]{2}-[0-9]+$\"\n\
             bad equals yes if code matches the pattern \"^[0-9]+$\"",
            json!({}),
        );
        let context = context_of(&result);
        assert_eq!(context.get("ok"), Some(&json!(true)));
        // A conditional assignment whose condition is false binds nothing.
        assert_eq!(context.get("bad"), None);
    }

    #[test]
    fn e2e_regex_first_match_returns_match_record() {
        let result = compile_and_run_ok(
            "email equals \"reach me at ann@example today\"\n\
             m equals first match of \"([a-z]+)@([a-z]+)\" in email\n\
             who equals text of m",
            json!({}),
        );
        let context = context_of(&result);
        assert_eq!(context["m"]["text"], json!("ann@example"));
        assert_eq!(context["m"]["start"], json!(12));
        assert_eq!(context["m"]["end"], json!(23));
        assert_eq!(context["m"]["groups"], json!(["ann", "example"]));
        assert_eq!(context.get("who"), Some(&json!("ann@example")));
    }

    #[test]
    fn e2e_regex_named_groups() {
        let result = compile_and_run_ok(
            "email equals \"ann@example\"\n\
             m equals first match of \"(?P<user>[a-z]+)@(?P<host>[a-z]+)\" in email\n\
             names equals named of m\n\
             user equals user of names",
            json!({}),
        );
        let context = context_of(&result);
        assert_eq!(context["m"]["named"], json!({"user": "ann", "host": "example"}));
        assert_eq!(context.get("user"), Some(&json!("ann")));
    }

    #[test]
    fn e2e_regex_first_match_returns_null_when_no_match() {
        let result = compile_and_run_ok(
            "m equals first match of \"z+\" in \"abc\"",
            json!({}),
        );
        assert_eq!(context_of(&result).get("m"), Some(&Value::Null));
    }

    #[test]
    fn e2e_regex_all_matches() {
        let result = compile_and_run_ok(
            "text equals \"order 12 then 34\"\n\
             nums equals all matches of \"[0-9]+\" in text",
            json!({}),
        );
        assert_eq!(context_of(&result).get("nums"), Some(&json!(["12", "34"])));
    }

    #[test]
    fn e2e_regex_replace_matches() {
        let result = compile_and_run_ok(
            "text equals \"order 12 then 34\"\n\
             clean equals replace matches of \"[0-9]+\" in text with \"#\"",
            json!({}),
        );
        assert_eq!(
            context_of(&result).get("clean"),
            Some(&json!("order # then #"))
        );
    }

    #[test]
    fn e2e_regex_split_by_pattern() {
        let result = compile_and_run_ok(
            "text equals \"a, b;c\"\n\
             parts equals split text by pattern \"[,;] *\"",
            json!({}),
        );
        assert_eq!(
            context_of(&result).get("parts"),
            Some(&json!(["a", "b", "c"]))
        );
    }

    #[test]
    fn e2e_regex_ignoring_case_flag() {
        let result = compile_and_run_ok(
            "hits equals all matches of \"abc\" in \"ABC abc\" ignoring case",
            json!({}),
        );
        assert_eq!(
            context_of(&result).get("hits"),
            Some(&json!(["ABC", "abc"]))
        );
    }

    #[test]
    fn e2e_literal_replace_and_split_are_unchanged() {
        let result = compile_and_run_ok(
            "text equals \"a.b\"\n\
             swapped equals replace \".\" in text with \"-\"\n\
             parts equals split text by \".\"",
            json!({}),
        );
        let context = context_of(&result);
        assert_eq!(context.get("swapped"), Some(&json!("a-b")));
        assert_eq!(context.get("parts"), Some(&json!(["a", "b"])));
    }

    #[test]
    fn invalid_literal_regex_is_a_compile_error() {
        let error = compile_source_to_json(
            "x equals all matches of \"[\" in \"abc\"",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect_err("invalid pattern fails to compile");
        assert!(
            error.to_string().contains("Invalid regular expression"),
            "got: {error}"
        );
    }

    #[test]
    fn invalid_dynamic_regex_fails_loudly_at_runtime() {
        let error = compile_and_run_err(
            "p equals \"[\"\nx equals all matches of p in \"abc\"",
            json!({}),
        );
        assert!(
            error.contains("Invalid regular expression"),
            "got: {error}"
        );
    }

    // DEVL-134: numeric tower (integer + Decimal + Fraction).

    fn decimal_json(text: &str) -> Value {
        json!({"__type": "decimal", "value": text})
    }

    #[test]
    fn e2e_decimal_literal_money_math_is_exact() {
        let result = compile_and_run_ok(
            "price equals decimal 19.99\ntotal equals price times 3",
            json!({}),
        );
        assert_eq!(
            context_of(&result).get("total"),
            Some(&decimal_json("59.97"))
        );
    }

    #[test]
    fn e2e_decimal_addition_avoids_float_drift() {
        let result = compile_and_run_ok(
            "a equals decimal 0.1\nb equals decimal 0.2\nc equals a plus b",
            json!({}),
        );
        assert_eq!(context_of(&result).get("c"), Some(&decimal_json("0.3")));
    }

    #[test]
    fn e2e_decimal_plus_integer_stays_exact() {
        let result = compile_and_run_ok(
            "a equals decimal 19.99\nb equals a plus 5",
            json!({}),
        );
        assert_eq!(context_of(&result).get("b"), Some(&decimal_json("24.99")));
    }

    #[test]
    fn e2e_decimal_float_mixing_is_a_loud_error() {
        let error = compile_and_run_err(
            "a equals decimal 1.5\nb equals a plus 0.5",
            json!({}),
        );
        assert!(
            error.contains("Cannot mix a decimal with a number"),
            "got: {error}"
        );
    }

    #[test]
    fn e2e_decimal_of_float_uses_display_digits() {
        let result = compile_and_run_ok("d equals decimal of 19.99", json!({}));
        assert_eq!(context_of(&result).get("d"), Some(&decimal_json("19.99")));
    }

    #[test]
    fn e2e_fraction_arithmetic_reduces_exactly() {
        let result = compile_and_run_ok(
            "f equals fraction 1 over 3\n\
             g equals f plus fraction 1 over 6\n\
             h equals f times 3\n\
             small equals yes if f < fraction 1 over 2",
            json!({}),
        );
        let context = context_of(&result);
        assert_eq!(
            context.get("g"),
            Some(&json!({"__type": "fraction", "numerator": 1, "denominator": 2}))
        );
        assert_eq!(
            context.get("h"),
            Some(&json!({"__type": "fraction", "numerator": 1, "denominator": 1}))
        );
        assert_eq!(context.get("small"), Some(&json!(true)));
    }

    #[test]
    fn e2e_round_to_decimal_places_default_half_even() {
        let result = compile_and_run_ok(
            "r equals round decimal 2.345 to 2 decimal places\n\
             r2 equals round decimal 2.345 to 2 decimal places rounding half up",
            json!({}),
        );
        let context = context_of(&result);
        assert_eq!(context.get("r"), Some(&decimal_json("2.34")));
        assert_eq!(context.get("r2"), Some(&decimal_json("2.35")));
    }

    #[test]
    fn e2e_round_applies_to_a_whole_arithmetic_expression() {
        // `round A times B to 2 decimal places` must round the product, not
        // split at `times` and misparse the right side.
        let result = compile_and_run_ok(
            "subtotal equals decimal 59.97\n\
             tax equals round subtotal times decimal 0.0825 to 2 decimal places",
            json!({}),
        );
        assert_eq!(context_of(&result).get("tax"), Some(&decimal_json("4.95")));
    }

    #[test]
    fn e2e_sum_and_average_of_decimals_are_exact() {
        let result = compile_and_run_ok(
            "xs equals list of decimal 0.1, decimal 0.2, decimal 0.3\n\
             s equals sum of xs\n\
             a equals average of xs",
            json!({}),
        );
        let context = context_of(&result);
        assert_eq!(context.get("s"), Some(&decimal_json("0.6")));
        assert_eq!(context.get("a"), Some(&decimal_json("0.2")));
    }

    #[test]
    fn e2e_decimal_compares_with_integers_by_quantity() {
        let result = compile_and_run_ok(
            "d equals decimal 5.0\n\
             same equals yes if d equals 5\n\
             bigger equals yes if d > 4",
            json!({}),
        );
        let context = context_of(&result);
        assert_eq!(context.get("same"), Some(&json!(true)));
        assert_eq!(context.get("bigger"), Some(&json!(true)));
    }

    #[test]
    fn e2e_numeric_value_converts_decimal_to_number() {
        let result = compile_and_run_ok(
            "n equals numeric value of decimal 2.5",
            json!({}),
        );
        assert_eq!(context_of(&result).get("n"), Some(&json!(2.5)));
    }

    #[test]
    fn e2e_decimal_concatenates_as_its_quantity() {
        let result = compile_and_run_ok(
            "msg equals \"total: \" plus decimal 19.99",
            json!({}),
        );
        assert_eq!(
            context_of(&result).get("msg"),
            Some(&json!("total: 19.99"))
        );
    }

    #[test]
    fn e2e_plain_number_sort_is_numeric() {
        let result = compile_and_run_ok(
            "sorted equals sort list of 10, 2",
            json!({}),
        );
        assert_eq!(context_of(&result).get("sorted"), Some(&json!([2, 10])));
    }

    #[test]
    fn invalid_decimal_literal_is_a_compile_error() {
        let error = compile_source_to_json(
            "x equals decimal \"abc\"",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect_err("bad decimal literal fails");
        assert!(error.to_string().contains("Invalid decimal"), "got: {error}");
    }

    #[test]
    fn zero_denominator_fraction_literal_is_a_compile_error() {
        let error = compile_source_to_json(
            "f equals fraction 1 over 0",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect_err("zero denominator fails");
        assert!(
            error.to_string().contains("denominator cannot be zero"),
            "got: {error}"
        );
    }

    #[test]
    fn e2e_dynamic_zero_denominator_fails_at_runtime() {
        let error = compile_and_run_err(
            "d equals 0\nf equals fraction 1 over d",
            json!({}),
        );
        assert!(
            error.contains("denominator cannot be zero"),
            "got: {error}"
        );
    }

    // DEVL-136: modulo, integer division, exponent.

    #[test]
    fn e2e_integer_modulo_idiv_pow_follow_python_semantics() {
        let result = compile_and_run_ok(
            "n equals -7\n\
             m equals n modulo 3\n\
             q equals n integer divided by 3\n\
             p equals 2 to the power of 10\n\
             s equals 3 squared\n\
             c equals 2 cubed",
            json!({}),
        );
        let context = context_of(&result);
        assert_eq!(context.get("m"), Some(&json!(2)));
        assert_eq!(context.get("q"), Some(&json!(-3)));
        assert_eq!(context.get("p"), Some(&json!(1024)));
        assert_eq!(context.get("s"), Some(&json!(9)));
        assert_eq!(context.get("c"), Some(&json!(8)));
    }

    #[test]
    fn e2e_symbol_operator_forms() {
        let result = compile_and_run_ok(
            "a equals 7 % 3\nb equals 7 // 2\nc equals 2 ** 8\nd equals 3 ^ 2",
            json!({}),
        );
        let context = context_of(&result);
        assert_eq!(context.get("a"), Some(&json!(1)));
        assert_eq!(context.get("b"), Some(&json!(3)));
        assert_eq!(context.get("c"), Some(&json!(256)));
        assert_eq!(context.get("d"), Some(&json!(9)));
    }

    #[test]
    fn e2e_power_binds_tighter_than_times() {
        let result = compile_and_run_ok(
            "v equals 2 plus 3 times 2 to the power of 2",
            json!({}),
        );
        assert_eq!(context_of(&result).get("v"), Some(&json!(14)));
    }

    #[test]
    fn e2e_float_modulo_and_floor_division() {
        let result = compile_and_run_ok(
            "f equals 7.5 modulo 2\nfd equals 7.5 // 2",
            json!({}),
        );
        let context = context_of(&result);
        assert_eq!(context.get("f"), Some(&json!(1.5)));
        assert_eq!(context.get("fd"), Some(&json!(3)));
    }

    #[test]
    fn e2e_decimal_modulo_idiv_pow() {
        let result = compile_and_run_ok(
            "m equals decimal 7.5 modulo decimal 2\n\
             q equals decimal -7 integer divided by decimal 2\n\
             growth equals decimal 1.05 to the power of 3",
            json!({}),
        );
        let context = context_of(&result);
        assert_eq!(context.get("m"), Some(&decimal_json("1.5")));
        // Python Decimal // truncates toward zero: -7 // 2 is -3, not -4.
        assert_eq!(context.get("q"), Some(&decimal_json("-3")));
        assert_eq!(context.get("growth"), Some(&decimal_json("1.157625")));
    }

    #[test]
    fn e2e_fraction_modulo_idiv_pow() {
        let result = compile_and_run_ok(
            "m equals fraction 7 over 2 modulo fraction 1 over 3\n\
             q equals fraction 7 over 2 integer divided by fraction 1 over 3\n\
             p equals fraction 2 over 3 to the power of 2\n\
             inv equals fraction 2 over 3 to the power of -1",
            json!({}),
        );
        let context = context_of(&result);
        assert_eq!(
            context.get("m"),
            Some(&json!({"__type": "fraction", "numerator": 1, "denominator": 6}))
        );
        assert_eq!(context.get("q"), Some(&json!(10)));
        assert_eq!(
            context.get("p"),
            Some(&json!({"__type": "fraction", "numerator": 4, "denominator": 9}))
        );
        assert_eq!(
            context.get("inv"),
            Some(&json!({"__type": "fraction", "numerator": 3, "denominator": 2}))
        );
    }

    #[test]
    fn e2e_modulo_by_zero_fails_loudly() {
        let error = compile_and_run_err("x equals 5 modulo 0", json!({}));
        assert!(error.contains("Modulo by zero"), "got: {error}");
    }

    #[test]
    fn e2e_fractional_decimal_exponent_fails_loudly() {
        // A float exponent hits the mixing rule; a decimal fractional
        // exponent hits the whole-number requirement. Both are loud.
        let mixing = compile_and_run_err(
            "x equals decimal 2 to the power of 0.5",
            json!({}),
        );
        assert!(mixing.contains("Cannot mix a decimal"), "got: {mixing}");
        let fractional = compile_and_run_err(
            "x equals decimal 2 to the power of decimal 0.5",
            json!({}),
        );
        assert!(fractional.contains("whole numbers"), "got: {fractional}");
    }

    #[test]
    fn lint_does_not_flag_comprehension_bindings() {
        let warnings = lint_source(
            "xs equals list of 1, 2\n\
             doubled equals map xs to item times 2\n\
             total equals reduce xs starting at 0 with acc and n to acc plus n",
            CompileOptions {
                source_path: None,
                search_paths: vec![],
            },
        )
        .expect("lints");
        assert!(
            warnings.is_empty(),
            "expected no warnings, got: {warnings:?}"
        );
    }
}
