use serde_json::{json, Map, Number, Value};
use std::collections::HashMap;

/// Trait for host-provided effects. The VM calls these for I/O operations.
/// WASM hosts implement via JS callbacks. Native hosts implement via filesystem.
pub trait HostEffects {
    fn emit_event(&mut self, event: &Value);
    fn write_file(&mut self, request: &Value) -> Result<(), String>;
    fn read_file(&mut self, request: &Value) -> Result<Value, String> {
        Err(format!(
            "read_file not implemented by this host (requested: {})",
            request
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        ))
    }
    fn call_service(&mut self, request: &Value) -> Result<Value, String> {
        Err(format!(
            "call_service not implemented by this host (service: {}, action: {})",
            request
                .get("service")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            request
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        ))
    }
    fn http_request(
        &mut self,
        method: &str,
        url: &str,
        _body: &Value,
        _headers: &Value,
    ) -> Result<Value, String> {
        Err(format!(
            "http_request not implemented by this host ({method} {url})"
        ))
    }
    fn respond(&mut self, _value: &Value) -> Result<(), String> {
        Err("respond not implemented by this host".to_string())
    }
    fn http_download(&mut self, _url: &str, _path: &str) -> Result<(), String> {
        Err("http_download not implemented by this host".to_string())
    }
    fn read_xlsx_rows(&mut self, _path: &str, _sheet: Option<&str>) -> Result<Value, String> {
        Err("read_xlsx_rows not implemented by this host".to_string())
    }
    fn file_copy(&mut self, _source: &str, _destination: &str) -> Result<(), String> {
        Err("file_copy not implemented by this host".to_string())
    }
    fn file_move(&mut self, _source: &str, _destination: &str) -> Result<(), String> {
        Err("file_move not implemented by this host".to_string())
    }
    fn file_mkdir(&mut self, _path: &str) -> Result<(), String> {
        Err("file_mkdir not implemented by this host".to_string())
    }
    fn file_delete(&mut self, _path: &str) -> Result<(), String> {
        Err("file_delete not implemented by this host".to_string())
    }
    fn file_exists(&mut self, _path: &str) -> Result<bool, String> {
        Err("file_exists not implemented by this host".to_string())
    }
    fn file_stat(&mut self, _path: &str) -> Result<Value, String> {
        Err("file_stat not implemented by this host".to_string())
    }
    fn file_list(&mut self, _path: &str) -> Result<Value, String> {
        Err("file_list not implemented by this host".to_string())
    }
    fn file_glob(&mut self, _pattern: &str, _directory: &str) -> Result<Value, String> {
        Err("file_glob not implemented by this host".to_string())
    }
    fn resolve_credential(&self, _key: &str) -> Option<String> {
        None
    }
    /// Called once at run completion for governed rules (a `Rule:` manifest
    /// section). The record binds the output to the rule that produced it.
    /// Hosts that persist audit trails override this; the default drops it.
    /// An Err fails the run: a governed run whose audit record cannot be
    /// persisted must not report success.
    fn audit_record(&mut self, _record: &Value) -> Result<(), String> {
        Ok(())
    }
}

/// Error type for VM operations.
#[derive(Debug)]
pub struct VmError {
    pub message: String,
    pub events: Vec<Value>,
}

pub struct Vm {
    constants: Vec<Value>,
    instructions: Vec<Map<String, Value>>,
    source_map: Vec<Map<String, Value>>,
    context: Map<String, Value>,
    results: Map<String, Value>,
    registers: HashMap<String, Value>,
    events: Vec<Value>,
    pc: usize,
    checkpoint: Option<Value>,
    try_stack: Vec<TryFrame>,
    manifest: Option<Value>,
    emit_events: bool,
    instruction_count: u64,
    instruction_limit: u64,
    audit_seed: Option<AuditSeed>,
}

/// Provenance facts fixed at load time for a governed rule, held until the
/// run completes and the audit record is emitted. Absent for ungoverned
/// programs, so they pay no hashing cost.
#[derive(Debug, Clone)]
struct AuditSeed {
    rule_id: String,
    rule_version: String,
    artifact_sha256: String,
    input_sha256: String,
}

#[derive(Debug, Clone)]
struct TryFrame {
    handler: usize,
}

impl Vm {
    pub fn new(package: Value, input: Value) -> Result<Self, VmError> {
        if package.get("format").and_then(Value::as_str) != Some("devlish-bytecode") {
            return Err(VmError {
                message: "Not a Devlish bytecode package".to_string(),
                events: Vec::new(),
            });
        }
        if package.get("format_version").and_then(Value::as_u64) != Some(0) {
            return Err(VmError {
                message: format!(
                    "Unsupported bytecode format_version: {}",
                    package.get("format_version").unwrap_or(&Value::Null)
                ),
                events: Vec::new(),
            });
        }

        let constants = package
            .get("constant_pool")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| VmError {
                message: "Missing constant_pool".to_string(),
                events: Vec::new(),
            })?;
        let instructions = package
            .get("instructions")
            .and_then(Value::as_array)
            .ok_or_else(|| VmError {
                message: "Missing instructions".to_string(),
                events: Vec::new(),
            })?
            .iter()
            .map(|item| {
                item.as_object().cloned().ok_or_else(|| VmError {
                    message: "Instruction must be an object".to_string(),
                    events: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        validate_control_flow(&instructions)?;

        let source_map = package
            .get("source_map")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(|v| v.as_object().cloned()).collect())
            .unwrap_or_default();

        let context = input.as_object().cloned().unwrap_or_default();
        let mut results = Map::new();
        results.insert("events".to_string(), Value::Array(Vec::new()));

        let manifest = package.get("manifest").cloned();

        // Governed rules (a Rule: manifest section) get an audit seed. The
        // artifact hash is the sorted-keys pretty serialization of the parsed
        // package -- the exact form DEVL-113 evidence bundles hash -- so an
        // audit record and an evidence report for the same artifact agree.
        // (Note: this is NOT the raw bytes `devlish compile` writes; the
        // compiler emits fields in declaration order. Canonicalizing here
        // makes the hash stable under reformatting of the bytecode file.)
        let audit_seed = manifest
            .as_ref()
            .and_then(|m| m.get("rule"))
            .and_then(|rule| {
                let id = rule.get("id").and_then(Value::as_str)?;
                let version = rule.get("version").and_then(Value::as_str)?;
                Some(AuditSeed {
                    rule_id: id.to_string(),
                    rule_version: version.to_string(),
                    artifact_sha256: sha256_hex(
                        serde_json::to_string_pretty(&package)
                            .unwrap_or_default()
                            .as_bytes(),
                    ),
                    input_sha256: sha256_hex(&serde_json::to_vec(&input).unwrap_or_default()),
                })
            });

        Ok(Self {
            constants,
            instructions,
            source_map,
            context,
            results,
            registers: HashMap::new(),
            events: Vec::new(),
            pc: 0,
            checkpoint: None,
            try_stack: Vec::new(),
            manifest,
            emit_events: true,
            instruction_count: 0,
            instruction_limit: 10_000_000,
            audit_seed,
        })
    }

    /// Sets the maximum number of instructions the VM will execute
    /// before returning an error. Default is 10 million.
    pub fn set_instruction_limit(&mut self, limit: u64) {
        self.instruction_limit = limit;
    }

    /// Number of instructions executed so far. After a completed run this is
    /// the same count recorded in the run's audit record, so a replay can
    /// assert it retraced the original execution exactly.
    pub fn executed_instructions(&self) -> u64 {
        self.instruction_count
    }

    /// Controls whether the VM emits events during execution.
    /// When disabled, push_event becomes a no-op, avoiding memory
    /// exhaustion from per-instruction JSON allocations.
    pub fn set_emit_events(&mut self, enabled: bool) {
        self.emit_events = enabled;
    }

    pub fn run(&mut self, host: &mut dyn HostEffects) -> Result<Value, VmError> {
        let outcome = self.run_inner(host);
        if let Some(seed) = self.audit_seed.take() {
            // The output hash covers the VM's own result envelope (or, on
            // error, the canonical failure envelope) -- NOT any wrapper a
            // runner prints or adds around it. See docs/AUDIT.md.
            let (success, output) = match &outcome {
                Ok(value) => (true, value.clone()),
                Err(error) => (false, json!({ "success": false, "error": error.message })),
            };
            // A CHECKPOINT pause returns success but is not a completed
            // evaluation; mark it so a compliance reader can tell the two
            // apart (the resumed run emits its own record).
            let paused = output
                .get("is_checkpoint")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let mut record = json!({
                "artifact_sha256": seed.artifact_sha256,
                "input_sha256": seed.input_sha256,
                "instruction_count": self.instruction_count,
                "output_sha256": sha256_hex(&serde_json::to_vec(&output).unwrap_or_default()),
                "rule_id": seed.rule_id,
                "rule_version": seed.rule_version,
                "success": success,
            });
            if paused {
                record["paused"] = json!(true);
            }
            if let Err(message) = host.audit_record(&record) {
                return Err(VmError {
                    message: format!("audit record write failed: {message}"),
                    events: Vec::new(),
                });
            }
        }
        outcome
    }

    fn run_inner(&mut self, host: &mut dyn HostEffects) -> Result<Value, VmError> {
        self.push_event(host, "run_started", Map::new());

        while self.pc < self.instructions.len() {
            self.instruction_count += 1;
            if self.instruction_count > self.instruction_limit {
                return Err(self.error(format!(
                    "Instruction limit exceeded ({} instructions)",
                    self.instruction_limit
                )));
            }

            let address = self.pc;
            let instruction = self.instructions[address].clone();
            self.pc += 1;

            let op = string_field(&instruction, "op")?;
            self.push_event(
                host,
                "instruction_started",
                map_from_pairs(vec![("address", json!(address)), ("op", json!(op.clone()))]),
            );
            if let Err(error) = self.execute(host, &instruction, &op) {
                if self.recover_from_error(host, &error.message) {
                    continue;
                }
                return Err(error);
            }
            self.push_event(
                host,
                "instruction_finished",
                map_from_pairs(vec![("address", json!(address)), ("op", json!(op))]),
            );
        }

        if let Some(checkpoint_result) = self.checkpoint.take() {
            return Ok(checkpoint_result);
        }

        self.push_event(host, "run_finished", Map::new());
        Ok(json!({
            "success": true,
            "context": self.context,
            "results": self.results
        }))
    }

    fn execute(
        &mut self,
        host: &mut dyn HostEffects,
        instruction: &Map<String, Value>,
        op: &str,
    ) -> Result<(), VmError> {
        match op {
            "CONST" => {
                let dest = string_field(instruction, "dest")?;
                let index = usize_field(instruction, "const")?;
                let value =
                    self.constants.get(index).cloned().ok_or_else(|| {
                        self.error(format!("Constant index out of range: {index}"))
                    })?;
                self.registers.insert(dest, value);
            }
            "LOAD" => {
                let dest = string_field(instruction, "dest")?;
                let symbol = string_field(instruction, "symbol")?;
                let value = self.context.get(&symbol).cloned().unwrap_or(Value::Null);
                self.registers.insert(dest, value);
            }
            "STORE" => {
                let symbol = string_field(instruction, "symbol")?;
                let value = self.register_value(&string_field(instruction, "value")?)?;
                self.context.insert(symbol.clone(), value.clone());
                self.results.insert(symbol.clone(), value.clone());
                self.push_event(
                    host,
                    "variable_assigned",
                    map_from_pairs(vec![("symbol", json!(symbol)), ("value", value)]),
                );
            }
            "ADD" | "SUB" | "MUL" | "DIV" | "MOD" | "IDIV" | "POW" | "AND" | "OR" => {
                self.execute_binary(instruction, op)?;
            }
            "EQ" | "NEQ" | "GT" | "GTE" | "LT" | "LTE" => {
                self.execute_comparison(instruction, op)?;
            }
            "JUMP_IF_FALSE" => {
                let condition = self.register_value(&string_field(instruction, "condition")?)?;
                if !truthy(&condition) {
                    self.pc = usize_field(instruction, "target")?;
                }
            }
            "JUMP" => {
                self.pc = usize_field(instruction, "target")?;
            }
            "ASK" => self.execute_ask(host, instruction)?,
            "TRY_BEGIN" => {
                let handler = usize_field(instruction, "handler")?;
                self.try_stack.push(TryFrame { handler });
            }
            "TRY_END" => {
                self.try_stack.pop();
            }
            "PRINT" => {
                let value = self.register_value(&string_field(instruction, "value")?)?;
                self.push_result_array("outputs", value.clone());
                self.push_event(
                    host,
                    "output_emitted",
                    map_from_pairs(vec![("value", value)]),
                );
            }
            "EXPORT" => self.execute_export(host, instruction)?,
            "READ_FILE" => self.execute_file_read(host, instruction)?,
            "VALIDATE" => self.execute_validation(host, instruction)?,
            "XLSX_READ_CELL" => self.execute_xlsx_read_cell(host, instruction)?,
            "PDF_READ_TEXT" => self.execute_pdf_text_read(host, instruction)?,
            "DOCX_READ_TEXT" => self.execute_docx_text_read(host, instruction)?,
            "ASSERT" => self.execute_assertion(host, instruction)?,
            "EXPORT_ASSERTIONS" => self.execute_assertion_export(host, instruction)?,
            "RESPOND" => {
                let value = self.register_value(&string_field(instruction, "value")?)?;
                host.respond(&value)
                    .map_err(|err| self.error(format!("Respond failed: {err}")))?;
                self.push_event(host, "run_finished", Map::new());
                let mut result = Map::new();
                result.insert("success".to_string(), json!(true));
                result.insert("responded".to_string(), json!(true));
                result.insert("response".to_string(), value);
                self.checkpoint = Some(Value::Object(result));
                self.pc = self.instructions.len();
            }
            "RETURN" => {
                self.pc = self.instructions.len();
            }
            "NOT" => {
                let operand = self.register_value(&string_field(instruction, "operand")?)?;
                self.registers.insert(
                    string_field(instruction, "dest")?,
                    Value::Bool(!truthy(&operand)),
                );
            }
            "FAIL" => {
                let message = self.register_value(&string_field(instruction, "message")?)?;
                let msg_str = match &message {
                    Value::Object(_) | Value::Array(_) => {
                        serde_json::to_string(&message).unwrap_or_else(|_| message.to_string())
                    }
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                return Err(self.error(msg_str));
            }
            "LIST_BUILD" => {
                let items = instruction
                    .get("items")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let values: Vec<Value> = items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|reg| self.register_value(reg))
                    .collect::<Result<Vec<_>, _>>()?;
                self.registers
                    .insert(string_field(instruction, "dest")?, Value::Array(values));
            }
            "LIST_LEN" => {
                let source = self.register_value(&string_field(instruction, "source")?)?;
                let len = source.as_array().map(|a| a.len()).unwrap_or(0);
                self.registers
                    .insert(string_field(instruction, "dest")?, json!(len));
            }
            "LIST_GET" => {
                let source = self.register_value(&string_field(instruction, "source")?)?;
                let index = self.register_value(&string_field(instruction, "index")?)?;
                let idx = number_as_f64(&index) as usize;
                let value = source
                    .as_array()
                    .and_then(|a| a.get(idx))
                    .cloned()
                    .unwrap_or(Value::Null);
                self.registers
                    .insert(string_field(instruction, "dest")?, value);
            }
            "LIST_CONTAINS" => {
                let list = self.register_value(&string_field(instruction, "list")?)?;
                let value = self.register_value(&string_field(instruction, "value")?)?;
                let found = list.as_array().map(|a| a.contains(&value)).unwrap_or(false);
                self.registers
                    .insert(string_field(instruction, "dest")?, Value::Bool(found));
            }
            "LIST_APPEND" => {
                let target = string_field(instruction, "target")?;
                let value = self.register_value(&string_field(instruction, "value")?)?;
                let list = self
                    .context
                    .entry(target.clone())
                    .or_insert_with(|| Value::Array(Vec::new()));
                if let Value::Array(items) = list {
                    items.push(value);
                }
                let current = self.context.get(&target).cloned().unwrap_or(Value::Null);
                self.results.insert(target, current);
            }
            "LIST_POP" => {
                let source = string_field(instruction, "source")?;
                let dest = string_field(instruction, "dest")?;
                let value = if let Some(Value::Array(items)) = self.context.get_mut(&source) {
                    items.pop().unwrap_or(Value::Null)
                } else {
                    Value::Null
                };
                self.context.insert(dest.clone(), value.clone());
                self.results.insert(dest, value);
            }
            "RECORD_BUILD" => {
                let keys = instruction
                    .get("keys")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let value_regs = instruction
                    .get("values")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let mut record = Map::new();
                for (key, reg) in keys.iter().zip(value_regs.iter()) {
                    if let (Some(k), Some(r)) = (key.as_str(), reg.as_str()) {
                        let val = self.register_value(r)?;
                        record.insert(k.to_string(), val);
                    }
                }
                self.registers
                    .insert(string_field(instruction, "dest")?, Value::Object(record));
            }
            "FIELD_GET" => {
                let record = self.register_value(&string_field(instruction, "record")?)?;
                let field = string_field(instruction, "field")?;
                let value = record
                    .as_object()
                    .and_then(|obj| obj.get(&field))
                    .cloned()
                    .unwrap_or(Value::Null);
                self.registers
                    .insert(string_field(instruction, "dest")?, value);
            }
            "FIELD_SET" => {
                let record_name = string_field(instruction, "record")?;
                let field = string_field(instruction, "field")?;
                let value = self.register_value(&string_field(instruction, "value")?)?;
                let record = self
                    .context
                    .entry(record_name.clone())
                    .or_insert_with(|| json!({}));
                if let Value::Object(obj) = record {
                    obj.insert(field, value);
                }
                let current = self
                    .context
                    .get(&record_name)
                    .cloned()
                    .unwrap_or(Value::Null);
                self.results.insert(record_name, current);
            }
            "FIELD_SET_PATH" => {
                let root = string_field(instruction, "root")?;
                let path: Vec<String> = instruction
                    .get("path")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|value| value.as_str().map(ToString::to_string))
                    .collect();
                let value = self.register_value(&string_field(instruction, "value")?)?;
                let root_value = self
                    .context
                    .entry(root.clone())
                    .or_insert_with(|| json!({}));
                set_path_value(root_value, &path, value)
                    .map_err(|err| self.error(format!("Set field failed for {root}: {err}")))?;
                let current = self.context.get(&root).cloned().unwrap_or(Value::Null);
                self.results.insert(root, current);
            }
            "STR_CONTAINS" => {
                let left = self.register_value(&string_field(instruction, "left")?)?;
                let right = self.register_value(&string_field(instruction, "right")?)?;
                let result = left
                    .as_str()
                    .unwrap_or_default()
                    .contains(right.as_str().unwrap_or_default());
                self.registers
                    .insert(string_field(instruction, "dest")?, Value::Bool(result));
            }
            "STR_STARTS_WITH" => {
                let left = self.register_value(&string_field(instruction, "left")?)?;
                let right = self.register_value(&string_field(instruction, "right")?)?;
                let result = left
                    .as_str()
                    .unwrap_or_default()
                    .starts_with(right.as_str().unwrap_or_default());
                self.registers
                    .insert(string_field(instruction, "dest")?, Value::Bool(result));
            }
            "STR_ENDS_WITH" => {
                let left = self.register_value(&string_field(instruction, "left")?)?;
                let right = self.register_value(&string_field(instruction, "right")?)?;
                let result = left
                    .as_str()
                    .unwrap_or_default()
                    .ends_with(right.as_str().unwrap_or_default());
                self.registers
                    .insert(string_field(instruction, "dest")?, Value::Bool(result));
            }
            "CALL_BUILTIN" => {
                let name = string_field(instruction, "name")?;
                let arg_regs = instruction
                    .get("args")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let args: Vec<Value> = arg_regs
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|reg| self.register_value(reg))
                    .collect::<Result<Vec<_>, _>>()?;
                let result = execute_builtin(&name, &args)?;
                self.registers
                    .insert(string_field(instruction, "dest")?, result);
            }
            "SERVICE_CALL" => {
                let service = string_field(instruction, "service")?;
                let action = string_field(instruction, "action")?;
                let args_reg = instruction
                    .get("args")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let args_val = if args_reg.is_empty() {
                    Value::Object(Map::new())
                } else {
                    self.register_value(args_reg)?
                };
                let dest = string_field(instruction, "dest")?;
                let request = json!({
                    "service": service,
                    "action": action,
                    "arguments": args_val,
                });
                self.push_event(
                    host,
                    "effect_requested",
                    map_from_pairs(vec![
                        ("kind", json!("service_call")),
                        ("service", json!(service)),
                        ("action", json!(action)),
                    ]),
                );
                let result = host.call_service(&request).map_err(|err| {
                    self.error(format!("Service call {service}.{action} failed: {err}"))
                })?;
                self.registers.insert(dest.clone(), result.clone());
                self.context.insert(dest.clone(), result.clone());
                self.results.insert(dest.clone(), result);
                self.push_event(
                    host,
                    "effect_completed",
                    map_from_pairs(vec![
                        ("kind", json!("service_call")),
                        ("service", json!(service)),
                        ("action", json!(action)),
                    ]),
                );
            }
            "HTTP_REQUEST" => {
                let method = string_field(instruction, "method")?;
                let url_val = self.register_value(&string_field(instruction, "url")?)?;
                let url = url_val.as_str().unwrap_or_default().to_string();
                self.check_manifest_permission("http_request", Some(&url))?;
                let body_reg = instruction
                    .get("body")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let body_val = if body_reg.is_empty() {
                    Value::Null
                } else {
                    self.register_value(body_reg)?
                };
                let dest = string_field(instruction, "dest")?;
                let empty_headers = Value::Object(Map::new());
                self.push_event(
                    host,
                    "effect_requested",
                    map_from_pairs(vec![
                        ("kind", json!("http_request")),
                        ("method", json!(method)),
                        ("url", json!(url)),
                    ]),
                );
                let result = host
                    .http_request(&method, &url, &body_val, &empty_headers)
                    .map_err(|err| self.error(format!("HTTP {method} {url} failed: {err}")))?;
                self.registers.insert(dest.clone(), result.clone());
                self.context.insert(dest.clone(), result.clone());
                self.results.insert(dest.clone(), result);
                self.push_event(
                    host,
                    "effect_completed",
                    map_from_pairs(vec![
                        ("kind", json!("http_request")),
                        ("method", json!(method)),
                        ("url", json!(url)),
                    ]),
                );
            }
            "HTTP_DOWNLOAD" => {
                let url_val = self.register_value(&string_field(instruction, "url")?)?;
                let path_val = self.register_value(&string_field(instruction, "path")?)?;
                let url = url_val.as_str().unwrap_or_default();
                let path = path_val.as_str().unwrap_or_default();
                self.check_manifest_permission("http_request", Some(url))?;
                self.push_event(
                    host,
                    "effect_requested",
                    map_from_pairs(vec![
                        ("kind", json!("http_download")),
                        ("url", json!(url)),
                        ("path", json!(path)),
                    ]),
                );
                host.http_download(url, path)
                    .map_err(|err| self.error(format!("Download {url} to {path} failed: {err}")))?;
                self.push_event(
                    host,
                    "effect_completed",
                    map_from_pairs(vec![("kind", json!("http_download"))]),
                );
            }
            "XLSX_READ_ROWS" => {
                let path_val = self.register_value(&string_field(instruction, "path")?)?;
                let path = path_val.as_str().unwrap_or_default();
                let sheet = instruction.get("sheet").and_then(Value::as_str);
                let dest = string_field(instruction, "dest")?;
                self.push_event(
                    host,
                    "effect_requested",
                    map_from_pairs(vec![
                        ("kind", json!("xlsx_read_rows")),
                        ("path", json!(path)),
                    ]),
                );
                let result = host.read_xlsx_rows(path, sheet).map_err(|err| {
                    self.error(format!("Read XLSX rows from {path} failed: {err}"))
                })?;
                self.registers.insert(dest.clone(), result.clone());
                self.context.insert(dest.clone(), result.clone());
                self.results.insert(dest.clone(), result);
                self.push_event(
                    host,
                    "effect_completed",
                    map_from_pairs(vec![("kind", json!("xlsx_read_rows"))]),
                );
            }
            "FILE_COPY" => {
                let src_val = self.register_value(&string_field(instruction, "source")?)?;
                let dst_val = self.register_value(&string_field(instruction, "destination")?)?;
                let source = src_val.as_str().unwrap_or_default();
                let destination = dst_val.as_str().unwrap_or_default();
                self.check_manifest_permission("file_copy", Some(destination))?;
                self.push_event(
                    host,
                    "effect_requested",
                    map_from_pairs(vec![
                        ("kind", json!("file_copy")),
                        ("source", json!(source)),
                        ("destination", json!(destination)),
                    ]),
                );
                host.file_copy(source, destination).map_err(|err| {
                    self.error(format!("Copy {source} to {destination} failed: {err}"))
                })?;
                self.push_event(
                    host,
                    "effect_completed",
                    map_from_pairs(vec![("kind", json!("file_copy"))]),
                );
            }
            "FILE_MOVE" => {
                let src_val = self.register_value(&string_field(instruction, "source")?)?;
                let dst_val = self.register_value(&string_field(instruction, "destination")?)?;
                let source = src_val.as_str().unwrap_or_default();
                let destination = dst_val.as_str().unwrap_or_default();
                self.check_manifest_permission("file_move", Some(destination))?;
                self.push_event(
                    host,
                    "effect_requested",
                    map_from_pairs(vec![
                        ("kind", json!("file_move")),
                        ("source", json!(source)),
                        ("destination", json!(destination)),
                    ]),
                );
                host.file_move(source, destination).map_err(|err| {
                    self.error(format!("Move {source} to {destination} failed: {err}"))
                })?;
                self.push_event(
                    host,
                    "effect_completed",
                    map_from_pairs(vec![("kind", json!("file_move"))]),
                );
            }
            "FILE_MKDIR" => {
                let path_val = self.register_value(&string_field(instruction, "path")?)?;
                let path = path_val.as_str().unwrap_or_default();
                self.check_manifest_permission("file_mkdir", Some(path))?;
                self.push_event(
                    host,
                    "effect_requested",
                    map_from_pairs(vec![("kind", json!("file_mkdir")), ("path", json!(path))]),
                );
                host.file_mkdir(path)
                    .map_err(|err| self.error(format!("Create directory {path} failed: {err}")))?;
                self.push_event(
                    host,
                    "effect_completed",
                    map_from_pairs(vec![("kind", json!("file_mkdir"))]),
                );
            }
            "FILE_DELETE" => {
                let path_val = self.register_value(&string_field(instruction, "path")?)?;
                let path = path_val.as_str().unwrap_or_default();
                self.check_manifest_permission("file_delete", Some(path))?;
                self.push_event(
                    host,
                    "effect_requested",
                    map_from_pairs(vec![("kind", json!("file_delete")), ("path", json!(path))]),
                );
                host.file_delete(path)
                    .map_err(|err| self.error(format!("Delete {path} failed: {err}")))?;
                self.push_event(
                    host,
                    "effect_completed",
                    map_from_pairs(vec![("kind", json!("file_delete"))]),
                );
            }
            "FILE_EXISTS" => {
                let path_val = self.register_value(&string_field(instruction, "path")?)?;
                let path = path_val.as_str().unwrap_or_default();
                let dest = string_field(instruction, "dest")?;
                self.push_event(
                    host,
                    "effect_requested",
                    map_from_pairs(vec![("kind", json!("file_exists")), ("path", json!(path))]),
                );
                let exists = host
                    .file_exists(path)
                    .map_err(|err| self.error(format!("Check exists {path} failed: {err}")))?;
                let result = Value::Bool(exists);
                self.registers.insert(dest.clone(), result.clone());
                self.context.insert(dest.clone(), result.clone());
                self.results.insert(dest.clone(), result);
                self.push_event(
                    host,
                    "effect_completed",
                    map_from_pairs(vec![("kind", json!("file_exists"))]),
                );
            }
            "FILE_STAT" => {
                let path_val = self.register_value(&string_field(instruction, "path")?)?;
                let path = path_val.as_str().unwrap_or_default();
                let dest = string_field(instruction, "dest")?;
                self.push_event(
                    host,
                    "effect_requested",
                    map_from_pairs(vec![("kind", json!("file_stat")), ("path", json!(path))]),
                );
                let result = host
                    .file_stat(path)
                    .map_err(|err| self.error(format!("Stat {path} failed: {err}")))?;
                self.registers.insert(dest.clone(), result.clone());
                self.context.insert(dest.clone(), result.clone());
                self.results.insert(dest.clone(), result);
                self.push_event(
                    host,
                    "effect_completed",
                    map_from_pairs(vec![("kind", json!("file_stat"))]),
                );
            }
            "FILE_LIST" => {
                let path_val = self.register_value(&string_field(instruction, "path")?)?;
                let path = path_val.as_str().unwrap_or_default();
                let dest = string_field(instruction, "dest")?;
                self.push_event(
                    host,
                    "effect_requested",
                    map_from_pairs(vec![("kind", json!("file_list")), ("path", json!(path))]),
                );
                let result = host
                    .file_list(path)
                    .map_err(|err| self.error(format!("List {path} failed: {err}")))?;
                self.registers.insert(dest.clone(), result.clone());
                self.context.insert(dest.clone(), result.clone());
                self.results.insert(dest.clone(), result);
                self.push_event(
                    host,
                    "effect_completed",
                    map_from_pairs(vec![("kind", json!("file_list"))]),
                );
            }
            "FILE_GLOB" => {
                let pattern_val = self.register_value(&string_field(instruction, "pattern")?)?;
                let dir_val = self.register_value(&string_field(instruction, "directory")?)?;
                let pattern = pattern_val.as_str().unwrap_or_default();
                let directory = dir_val.as_str().unwrap_or_default();
                let dest = string_field(instruction, "dest")?;
                self.push_event(
                    host,
                    "effect_requested",
                    map_from_pairs(vec![
                        ("kind", json!("file_glob")),
                        ("pattern", json!(pattern)),
                        ("directory", json!(directory)),
                    ]),
                );
                let result = host.file_glob(pattern, directory).map_err(|err| {
                    self.error(format!("Glob {pattern} in {directory} failed: {err}"))
                })?;
                self.registers.insert(dest.clone(), result.clone());
                self.context.insert(dest.clone(), result.clone());
                self.results.insert(dest.clone(), result);
                self.push_event(
                    host,
                    "effect_completed",
                    map_from_pairs(vec![("kind", json!("file_glob"))]),
                );
            }
            "LOAD_FILE" => {
                let path = self.register_value(&string_field(instruction, "path")?)?;
                let alias = string_field(instruction, "alias")?;
                let path_str = path.as_str().unwrap_or_default().to_string();
                let request = json!({ "path": path_str });
                self.push_event(
                    host,
                    "effect_requested",
                    map_from_pairs(vec![
                        ("kind", json!("read_file")),
                        ("path", json!(path_str)),
                    ]),
                );
                let content = host
                    .read_file(&request)
                    .map_err(|err| self.error(format!("Load file failed for {path_str}: {err}")))?;
                self.context.insert(alias.clone(), content.clone());
                self.results.insert(alias.clone(), content.clone());
                self.registers.insert(alias.clone(), content);
                self.push_event(
                    host,
                    "effect_completed",
                    map_from_pairs(vec![
                        ("kind", json!("read_file")),
                        ("path", json!(path_str)),
                        ("alias", json!(alias)),
                    ]),
                );
            }
            "EXTRACT" => {
                let source = string_field(instruction, "source")?;
                let field = string_field(instruction, "field")?;
                let dest = string_field(instruction, "dest")?;
                let source_val = self.context.get(&source).cloned().unwrap_or(Value::Null);
                let extracted = source_val
                    .as_object()
                    .and_then(|obj| obj.get(&field))
                    .cloned()
                    .unwrap_or(Value::Null);
                self.registers.insert(dest.clone(), extracted.clone());
                self.context.insert(dest.clone(), extracted.clone());
                self.results.insert(dest, extracted);
            }
            "REQUIRE_DOC" => {
                let target = string_field(instruction, "target")?;
                let verb = instruction
                    .get("verb")
                    .and_then(Value::as_str)
                    .unwrap_or("require")
                    .to_string();
                let exists = self
                    .context
                    .get(&target)
                    .map(|v| !v.is_null())
                    .unwrap_or(false);
                if !exists {
                    return Err(
                        self.error(format!("Document requirement not met: {verb} {target}"))
                    );
                }
            }
            "ROUTE" => {
                let source_val = self.register_value(&string_field(instruction, "source")?)?;
                let dest_val = self.register_value(&string_field(instruction, "dest")?)?;
                let dest_str = dest_val.as_str().unwrap_or_default().to_string();
                let request = json!({
                    "source": source_val,
                    "destination": dest_str,
                });
                self.push_event(
                    host,
                    "effect_requested",
                    map_from_pairs(vec![
                        ("kind", json!("route")),
                        ("destination", json!(dest_str)),
                    ]),
                );
                host.write_file(&request)
                    .map_err(|err| self.error(format!("Route to {dest_str} failed: {err}")))?;
                self.push_event(
                    host,
                    "effect_completed",
                    map_from_pairs(vec![
                        ("kind", json!("route")),
                        ("destination", json!(dest_str)),
                    ]),
                );
            }
            "CHECKPOINT" => {
                let prompt = self.register_value(&string_field(instruction, "prompt")?)?;
                let prompt_str = prompt.as_str().unwrap_or_default().to_string();
                let context_key = instruction
                    .get("context_key")
                    .and_then(Value::as_str)
                    .unwrap_or("checkpoint");
                let checkpoint_context = Value::Object(self.context.clone());
                self.results
                    .insert(context_key.to_string(), checkpoint_context.clone());
                self.push_event(
                    host,
                    "checkpoint",
                    map_from_pairs(vec![
                        ("kind", json!("needs_agent")),
                        ("prompt", json!(prompt_str)),
                        ("context_key", json!(context_key)),
                    ]),
                );
                self.push_event(host, "run_finished", Map::new());
                let mut result = Map::new();
                result.insert("success".to_string(), json!(true));
                result.insert("is_checkpoint".to_string(), json!(true));
                result.insert("prompt".to_string(), json!(prompt_str));
                result.insert("context".to_string(), Value::Object(self.context.clone()));
                result.insert("results".to_string(), Value::Object(self.results.clone()));
                result.insert(context_key.to_string(), checkpoint_context);
                // Set checkpoint and force pc past end to exit run loop
                self.checkpoint = Some(Value::Object(result));
                self.pc = self.instructions.len();
            }
            "NOP" => {
                let note = instruction
                    .get("note")
                    .and_then(Value::as_str)
                    .unwrap_or("no-op instruction");
                let address = self.pc - 1;
                let source_hint = self
                    .source_text_at(address)
                    .map(|text| format!(" (source: {text})"))
                    .unwrap_or_default();
                return Err(self.error(format!(
                    "Statement not yet implemented: {note}{source_hint}"
                )));
            }
            _ => return Err(self.error(format!("Unsupported bytecode opcode: {op}"))),
        }

        Ok(())
    }

    fn execute_binary(
        &mut self,
        instruction: &Map<String, Value>,
        op: &str,
    ) -> Result<(), VmError> {
        let left = self.register_value(&string_field(instruction, "left")?)?;
        let right = self.register_value(&string_field(instruction, "right")?)?;
        let value = match op {
            "ADD" => {
                // String concatenation when either operand is a string
                if left.is_string() || right.is_string() {
                    let render = |value: &Value| match value {
                        Value::String(s) => s.clone(),
                        Value::Null => String::new(),
                        // Tagged numerics read as their quantity ("19.99"),
                        // not their JSON encoding.
                        other if tagged_numeric_kind(other).is_some() => value_as_str(other),
                        other => serde_json::to_string(other).unwrap_or_default(),
                    };
                    Value::String(format!("{}{}", render(&left), render(&right)))
                } else if let (Some(l), Some(r)) = (as_numeric(&left), as_numeric(&right)) {
                    numeric_binary("ADD", l, r)?
                } else {
                    number_value(number_as_f64(&left) + number_as_f64(&right))?
                }
            }
            "SUB" | "MUL" | "DIV" | "MOD" | "IDIV" | "POW" => {
                if let (Some(l), Some(r)) = (as_numeric(&left), as_numeric(&right)) {
                    numeric_binary(op, l, r)?
                } else {
                    let (a, b) = (number_as_f64(&left), number_as_f64(&right));
                    match op {
                        "SUB" => number_value(a - b)?,
                        "MUL" => number_value(a * b)?,
                        "DIV" => number_value(a / b)?,
                        "MOD" => number_value(a - b * (a / b).floor())?,
                        "IDIV" => number_value((a / b).floor())?,
                        _ => number_value(a.powf(b))?,
                    }
                }
            }
            "AND" => Value::Bool(truthy(&left) && truthy(&right)),
            "OR" => Value::Bool(truthy(&left) || truthy(&right)),
            _ => unreachable!(),
        };
        self.registers
            .insert(string_field(instruction, "dest")?, value);
        Ok(())
    }

    fn execute_comparison(
        &mut self,
        instruction: &Map<String, Value>,
        op: &str,
    ) -> Result<(), VmError> {
        let left = self.register_value(&string_field(instruction, "left")?)?;
        let right = self.register_value(&string_field(instruction, "right")?)?;
        // Tagged numerics (decimal/fraction) compare by quantity, so
        // decimal "5" equals the integer 5. Plain values keep their existing
        // semantics (EQ/NEQ structural, ordering via f64).
        let tagged = tagged_numeric_kind(&left).is_some() || tagged_numeric_kind(&right).is_some();
        let ordering = match (as_numeric(&left), as_numeric(&right)) {
            (Some(l), Some(r)) => Some(numeric_compare(l, r)),
            _ => None,
        };
        let value = match op {
            "EQ" => match ordering {
                Some(ordering) if tagged => ordering == std::cmp::Ordering::Equal,
                _ => left == right,
            },
            "NEQ" => match ordering {
                Some(ordering) if tagged => ordering != std::cmp::Ordering::Equal,
                _ => left != right,
            },
            "GT" | "GTE" | "LT" | "LTE" => {
                let ordering = ordering.unwrap_or_else(|| {
                    number_as_f64(&left)
                        .partial_cmp(&number_as_f64(&right))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                match op {
                    "GT" => ordering == std::cmp::Ordering::Greater,
                    "GTE" => ordering != std::cmp::Ordering::Less,
                    "LT" => ordering == std::cmp::Ordering::Less,
                    _ => ordering != std::cmp::Ordering::Greater,
                }
            }
            _ => unreachable!(),
        };
        self.registers
            .insert(string_field(instruction, "dest")?, Value::Bool(value));
        Ok(())
    }

    fn execute_ask(
        &mut self,
        host: &mut dyn HostEffects,
        instruction: &Map<String, Value>,
    ) -> Result<(), VmError> {
        let target = string_field(instruction, "target")?;
        let prompt = instruction
            .get("prompt")
            .and_then(Value::as_str)
            .and_then(|register| self.registers.get(register))
            .cloned()
            .unwrap_or(Value::Null);

        self.push_event(
            host,
            "effect_requested",
            map_from_pairs(vec![
                ("kind", json!("input")),
                ("target", json!(target.clone())),
                ("prompt", prompt.clone()),
            ]),
        );

        let value = self
            .context
            .get(&target)
            .cloned()
            .or_else(|| {
                self.context
                    .get("__input__")
                    .and_then(Value::as_object)
                    .and_then(|input| input.get(&target))
                    .cloned()
            })
            .ok_or_else(|| self.error(format!("Missing input for {target}")))?;

        self.context.insert(target.clone(), value.clone());
        self.results.insert(target.clone(), value.clone());
        self.push_result_array(
            "inputs",
            json!({
                "target": target,
                "prompt": prompt,
                "value": value
            }),
        );
        self.push_event(
            host,
            "effect_completed",
            map_from_pairs(vec![("kind", json!("input"))]),
        );
        Ok(())
    }

    fn execute_export(
        &mut self,
        host: &mut dyn HostEffects,
        instruction: &Map<String, Value>,
    ) -> Result<(), VmError> {
        let value = self.register_value(&string_field(instruction, "value")?)?;
        let path = self
            .register_value(&string_field(instruction, "path")?)?
            .as_str()
            .unwrap_or_default()
            .to_string();
        if path.trim().is_empty() {
            return Err(self.error("File path cannot be empty".to_string()));
        }

        let mode = instruction
            .get("mode")
            .and_then(Value::as_str)
            .unwrap_or("export")
            .to_string();
        let content = file_content(&value, &mode);
        let request = json!({
            "path": path,
            "content": content,
            "mode": mode
        });

        self.push_event(
            host,
            "effect_requested",
            map_from_pairs(vec![
                ("kind", json!("file_write")),
                ("path", request["path"].clone()),
                ("mode", request["mode"].clone()),
            ]),
        );
        host.write_file(&request).map_err(|err| {
            self.error(format!(
                "Host write_file failed for {}: {err}",
                request["path"]
            ))
        })?;

        let bytes = request["content"]
            .as_str()
            .unwrap_or_default()
            .as_bytes()
            .len();
        self.push_result_array(
            "files_written",
            json!({
                "path": request["path"],
                "bytes": bytes,
                "mode": request["mode"]
            }),
        );
        self.push_event(
            host,
            "effect_completed",
            map_from_pairs(vec![
                ("kind", json!("file_write")),
                ("path", request["path"].clone()),
                ("bytes", json!(bytes)),
            ]),
        );
        Ok(())
    }

    fn execute_file_read(
        &mut self,
        host: &mut dyn HostEffects,
        instruction: &Map<String, Value>,
    ) -> Result<(), VmError> {
        let target = string_field(instruction, "target")?;
        let format = string_field(instruction, "format")?;
        let path = self
            .register_value(&string_field(instruction, "path")?)?
            .as_str()
            .unwrap_or_default()
            .to_string();
        if path.trim().is_empty() {
            return Err(self.error("File path cannot be empty".to_string()));
        }

        let request = json!({ "path": path, "format": format });
        self.push_event(
            host,
            "effect_requested",
            map_from_pairs(vec![
                ("kind", json!("file_read")),
                ("path", request["path"].clone()),
                ("format", request["format"].clone()),
            ]),
        );
        let raw = host.read_file(&request).map_err(|err| {
            self.error(format!(
                "Host read_file failed for {}: {err}",
                request["path"]
            ))
        })?;
        let parsed = parse_file_content(&raw, &format).map_err(|err| {
            self.error(format!(
                "Read {format} failed for {}: {err}",
                request["path"]
            ))
        })?;

        self.context.insert(target.clone(), parsed.clone());
        self.results.insert(target.clone(), parsed.clone());
        self.push_result_array(
            "files_read",
            json!({
                "path": request["path"],
                "format": request["format"],
                "target": target,
            }),
        );
        self.push_event(
            host,
            "effect_completed",
            map_from_pairs(vec![
                ("kind", json!("file_read")),
                ("path", request["path"].clone()),
                ("format", request["format"].clone()),
            ]),
        );
        Ok(())
    }

    fn execute_validation(
        &mut self,
        host: &mut dyn HostEffects,
        instruction: &Map<String, Value>,
    ) -> Result<(), VmError> {
        let target = string_field(instruction, "target")?;
        let rule = string_field(instruction, "rule")?;
        let actual = self.register_value(&string_field(instruction, "actual")?)?;
        let expected = match instruction.get("expected").and_then(Value::as_str) {
            Some(register) => self.register_value(register)?,
            None => Value::Null,
        };
        let passed = validate_value(&actual, &rule, &expected);
        let message = validation_message(&target, &rule, &expected, &actual, passed);
        let validation = json!({
            "target": target,
            "rule": rule,
            "expected": if expected == Value::Null { Value::Null } else { typed_value(&expected) },
            "actual": typed_value(&actual),
            "passed": passed,
            "message": message,
        });
        self.push_result_array("validations", validation);
        self.push_event(
            host,
            "validation_recorded",
            map_from_pairs(vec![
                ("target", json!(target)),
                ("rule", json!(rule)),
                ("passed", json!(passed)),
            ]),
        );
        if !passed {
            return Err(self.error(message));
        }
        Ok(())
    }

    fn execute_xlsx_read_cell(
        &mut self,
        host: &mut dyn HostEffects,
        instruction: &Map<String, Value>,
    ) -> Result<(), VmError> {
        let target = string_field(instruction, "target")?;
        let sheet = string_field(instruction, "sheet")?;
        let cell = string_field(instruction, "cell")?;
        let reference = format!("{sheet}!{cell}");
        self.push_event(
            host,
            "effect_requested",
            map_from_pairs(vec![
                ("kind", json!("xlsx_read_cell")),
                ("target", json!(target.clone())),
                ("sheet", json!(sheet.clone())),
                ("cell", json!(cell.clone())),
            ]),
        );

        let value = self
            .context
            .get("__xlsx_cells__")
            .and_then(Value::as_object)
            .and_then(|cells| cells.get(&reference))
            .cloned()
            .ok_or_else(|| self.error(format!("Missing preloaded XLSX cell: {reference}")))?;

        self.context.insert(target.clone(), value.clone());
        self.results.insert(target.clone(), value.clone());
        self.push_result_array(
            "xlsx_cells",
            json!({
                "target": target,
                "sheet": sheet,
                "cell": cell,
                "value": value
            }),
        );
        self.push_event(
            host,
            "effect_completed",
            map_from_pairs(vec![("kind", json!("xlsx_read_cell"))]),
        );
        Ok(())
    }

    fn execute_pdf_text_read(
        &mut self,
        host: &mut dyn HostEffects,
        instruction: &Map<String, Value>,
    ) -> Result<(), VmError> {
        let target = string_field(instruction, "target")?;
        let path = string_field(instruction, "path")?;
        self.push_event(
            host,
            "effect_requested",
            map_from_pairs(vec![
                ("kind", json!("pdf_read_text")),
                ("target", json!(target.clone())),
                ("path", json!(path.clone())),
            ]),
        );

        let value = self
            .context
            .get("__pdf_texts__")
            .and_then(Value::as_object)
            .and_then(|texts| texts.get(&path))
            .cloned()
            .ok_or_else(|| self.error(format!("Missing preloaded PDF text: {path}")))?;

        self.context.insert(target.clone(), value.clone());
        self.results.insert(target.clone(), value.clone());
        self.push_result_array(
            "pdf_texts",
            json!({
                "target": target,
                "path": path,
                "value": value
            }),
        );
        self.push_event(
            host,
            "effect_completed",
            map_from_pairs(vec![("kind", json!("pdf_read_text"))]),
        );
        Ok(())
    }

    fn execute_docx_text_read(
        &mut self,
        host: &mut dyn HostEffects,
        instruction: &Map<String, Value>,
    ) -> Result<(), VmError> {
        let target = string_field(instruction, "target")?;
        let path = string_field(instruction, "path")?;
        self.push_event(
            host,
            "effect_requested",
            map_from_pairs(vec![
                ("kind", json!("docx_read_text")),
                ("target", json!(target.clone())),
                ("path", json!(path.clone())),
            ]),
        );

        let value = self
            .context
            .get("__docx_texts__")
            .and_then(Value::as_object)
            .and_then(|texts| texts.get(&path))
            .cloned()
            .ok_or_else(|| self.error(format!("Missing preloaded DOCX text: {path}")))?;

        self.context.insert(target.clone(), value.clone());
        self.results.insert(target.clone(), value.clone());
        self.push_result_array(
            "docx_texts",
            json!({
                "target": target,
                "path": path,
                "value": value
            }),
        );
        self.push_event(
            host,
            "effect_completed",
            map_from_pairs(vec![("kind", json!("docx_read_text"))]),
        );
        Ok(())
    }

    fn execute_assertion(
        &mut self,
        host: &mut dyn HostEffects,
        instruction: &Map<String, Value>,
    ) -> Result<(), VmError> {
        let assertion_id = string_field(instruction, "assertion_id")?;
        let target = string_field(instruction, "target")?;
        let operator = string_field(instruction, "operator")?;
        let actual = self.context.get(&target).cloned().unwrap_or(Value::Null);
        let expected = match instruction.get("expected").and_then(Value::as_str) {
            Some(register) => self.register_value(register)?,
            None => Value::Null,
        };
        let assertion = evaluate_assertion(&assertion_id, &target, &operator, actual, expected)?;
        let passed = assertion
            .get("passed")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.push_result_array("assertions", assertion);
        self.push_event(
            host,
            "assertion_recorded",
            map_from_pairs(vec![
                ("assertion_id", json!(assertion_id)),
                ("passed", json!(passed)),
            ]),
        );
        Ok(())
    }

    fn execute_assertion_export(
        &mut self,
        host: &mut dyn HostEffects,
        instruction: &Map<String, Value>,
    ) -> Result<(), VmError> {
        let path = self
            .register_value(&string_field(instruction, "path")?)?
            .as_str()
            .unwrap_or_default()
            .to_string();
        if path.trim().is_empty() {
            return Err(self.error("File path cannot be empty".to_string()));
        }

        let assertions = self
            .results
            .get("assertions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let success = assertions
            .iter()
            .all(|assertion| assertion.get("passed").and_then(Value::as_bool) == Some(true));
        let report = json!({
            "schema_version": self.context
                .get("assertion_report_schema_version")
                .and_then(Value::as_str)
                .unwrap_or("devlish-xlsx-report-v0"),
            "benchmark": self.context.get("benchmark").and_then(Value::as_str).unwrap_or("xlsx_expected_cells"),
            "success": success,
            "assertions": assertions
        });
        let content = serde_json::to_string_pretty(&report).unwrap_or_default();
        let request = json!({
            "path": path,
            "content": content,
            "mode": "assertions"
        });

        self.push_event(
            host,
            "effect_requested",
            map_from_pairs(vec![
                ("kind", json!("file_write")),
                ("path", request["path"].clone()),
                ("mode", request["mode"].clone()),
            ]),
        );
        host.write_file(&request).map_err(|err| {
            self.error(format!(
                "Host write_file failed for {}: {err}",
                request["path"]
            ))
        })?;

        let bytes = request["content"]
            .as_str()
            .unwrap_or_default()
            .as_bytes()
            .len();
        self.results.insert("assertion_report".to_string(), report);
        self.push_result_array(
            "files_written",
            json!({
                "path": request["path"],
                "bytes": bytes,
                "mode": request["mode"]
            }),
        );
        self.push_event(
            host,
            "effect_completed",
            map_from_pairs(vec![
                ("kind", json!("file_write")),
                ("path", request["path"].clone()),
                ("bytes", json!(bytes)),
            ]),
        );
        Ok(())
    }

    fn register_value(&self, register: &str) -> Result<Value, VmError> {
        self.registers
            .get(register)
            .cloned()
            .ok_or_else(|| self.error(format!("Register not set: {register}")))
    }

    fn push_result_array(&mut self, key: &str, value: Value) {
        let entry = self
            .results
            .entry(key.to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Value::Array(items) = entry {
            items.push(value);
        }
    }

    fn push_event(
        &mut self,
        host: &mut dyn HostEffects,
        event_type: &str,
        mut attributes: Map<String, Value>,
    ) {
        if !self.emit_events {
            return;
        }
        attributes.insert("type".to_string(), json!(event_type));
        attributes.insert("pc".to_string(), json!(self.pc));
        let event = Value::Object(attributes);
        self.events.push(event.clone());
        self.push_result_array("events", event.clone());
        host.emit_event(&event);
    }

    fn source_text_at(&self, address: usize) -> Option<String> {
        self.source_map
            .get(address)
            .and_then(|entry| entry.get("source_text"))
            .and_then(Value::as_str)
            .map(ToString::to_string)
    }

    fn error(&self, message: String) -> VmError {
        VmError {
            message,
            events: self.events.clone(),
        }
    }

    fn check_manifest_permission(&self, kind: &str, detail: Option<&str>) -> Result<(), VmError> {
        let Some(manifest) = &self.manifest else {
            return Ok(()); // No manifest means no enforcement
        };
        let permissions = manifest.get("permissions").and_then(Value::as_array);
        let Some(permissions) = permissions else {
            return Ok(()); // No permissions array means no enforcement
        };
        if permissions.is_empty() {
            return Ok(()); // Empty permissions array means no enforcement
        }

        for perm in permissions {
            let perm_kind = perm.get("kind").and_then(Value::as_str).unwrap_or("");
            if perm_kind == kind {
                if let Some(path) = detail {
                    if let Some(scope) = perm.get("scope").and_then(Value::as_str) {
                        if path.starts_with(scope) {
                            return Ok(());
                        }
                    } else {
                        return Ok(()); // Permission with no scope allows all
                    }
                } else {
                    return Ok(()); // No detail to check
                }
            }
            // "filesystem" permission covers all file_* operations
            if perm_kind == "filesystem"
                && (kind.starts_with("file_") || kind == "read_file" || kind == "write_file")
            {
                if let Some(path) = detail {
                    if let Some(scope) = perm.get("scope").and_then(Value::as_str) {
                        if path.starts_with(scope) {
                            return Ok(());
                        }
                    } else {
                        return Ok(());
                    }
                } else {
                    return Ok(());
                }
            }
        }

        let detail_msg = detail.map(|d| format!(" (path: {d})")).unwrap_or_default();
        Err(self.error(format!(
            "Permission denied: {kind}{detail_msg} is not declared in the program manifest"
        )))
    }

    fn recover_from_error(&mut self, host: &mut dyn HostEffects, message: &str) -> bool {
        let Some(frame) = self.try_stack.pop() else {
            return false;
        };
        let error_value = json!({ "message": message });
        self.context
            .insert("last_error".to_string(), error_value.clone());
        self.results
            .insert("last_error".to_string(), error_value.clone());
        self.push_event(
            host,
            "error_recovered",
            map_from_pairs(vec![
                ("handler", json!(frame.handler)),
                ("message", json!(message)),
            ]),
        );
        self.pc = frame.handler;
        true
    }
}

fn string_field(instruction: &Map<String, Value>, key: &str) -> Result<String, VmError> {
    instruction
        .get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .ok_or_else(|| VmError {
            message: format!("Missing string field: {key}"),
            events: Vec::new(),
        })
}

/// Validates control-flow targets before execution. A JUMP or handler target
/// past the instruction array would otherwise make the run loop exit as an
/// apparent success, which untrusted bytecode could exploit to skip logic.
/// A target equal to the instruction count is allowed: that is the normal
/// jump-to-end the compiler emits for If/While exits. The same applies to
/// TRY_BEGIN handlers — an `Attempt:` with no `Otherwise` recovery as the
/// final statement patches its handler to the instruction count, and
/// recovering to end-of-program is the defined swallow-and-finish semantics.
fn validate_control_flow(instructions: &[Map<String, Value>]) -> Result<(), VmError> {
    let count = instructions.len();
    for (address, instruction) in instructions.iter().enumerate() {
        let Some(op) = instruction.get("op").and_then(Value::as_str) else {
            return Err(VmError {
                message: format!(
                    "Invalid bytecode: instruction {address} has no string 'op' field"
                ),
                events: Vec::new(),
            });
        };
        let field = match op {
            "JUMP" | "JUMP_IF_FALSE" => "target",
            "TRY_BEGIN" => "handler",
            _ => continue,
        };
        let target = instruction
            .get(field)
            .and_then(Value::as_u64)
            .ok_or_else(|| VmError {
                message: format!(
                    "Invalid bytecode: instruction {address} ({op}) is missing numeric '{field}'"
                ),
                events: Vec::new(),
            })?;
        if target as usize > count {
            return Err(VmError {
                message: format!(
                    "Invalid bytecode: instruction {address} ({op}) {field} {target} is out of range (program has {count} instructions)"
                ),
                events: Vec::new(),
            });
        }
    }
    Ok(())
}

fn usize_field(instruction: &Map<String, Value>, key: &str) -> Result<usize, VmError> {
    instruction
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .ok_or_else(|| VmError {
            message: format!("Missing numeric field: {key}"),
            events: Vec::new(),
        })
}

pub fn number_as_f64(value: &Value) -> f64 {
    match value {
        Value::Number(number) => number.as_f64().unwrap_or(0.0),
        Value::String(text) => text.parse::<f64>().unwrap_or(0.0),
        Value::Bool(true) => 1.0,
        Value::Object(_) => match as_numeric(value) {
            Some(numeric) => numeric.to_f64(),
            None => 0.0,
        },
        _ => 0.0,
    }
}

// ---------------------------------------------------------------------------
// Numeric tower (DEVL-134): distinct integer, exact base-10 Decimal, and
// Fraction values on top of the JSON data model. Decimals and fractions are
// tagged records ({"__type": "decimal", "value": "19.99"} /
// {"__type": "fraction", "numerator": 1, "denominator": 3}) so they flow
// unchanged through artifacts, journals, checkpoints, and the WASM boundary.
// Mixing rules follow Python: integers combine exactly with everything;
// decimal<->float, fraction<->float, and decimal<->fraction arithmetic are
// loud errors (convert explicitly). Comparisons are lenient across kinds.
// ---------------------------------------------------------------------------

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

const TYPE_TAG: &str = "__type";

/// Kind tag of a tagged numeric record, if the value is one.
fn tagged_numeric_kind(value: &Value) -> Option<&str> {
    match value.get(TYPE_TAG).and_then(Value::as_str) {
        Some(kind @ ("decimal" | "fraction")) => Some(kind),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Numeric {
    Int(i64),
    Float(f64),
    Dec(Decimal),
    Frac(i64, i64),
}

impl Numeric {
    fn kind(&self) -> &'static str {
        match self {
            Numeric::Int(_) => "integer",
            Numeric::Float(_) => "number",
            Numeric::Dec(_) => "decimal",
            Numeric::Frac(..) => "fraction",
        }
    }

    fn to_f64(self) -> f64 {
        match self {
            Numeric::Int(i) => i as f64,
            Numeric::Float(f) => f,
            Numeric::Dec(d) => d.to_f64().unwrap_or(0.0),
            Numeric::Frac(n, d) => n as f64 / d as f64,
        }
    }
}

/// Builds the canonical tagged Value for a Decimal (trailing zeros kept as
/// written is NOT canonical; normalize so equal quantities are structurally
/// equal).
pub fn decimal_json(decimal: Decimal) -> Value {
    json!({ TYPE_TAG: "decimal", "value": decimal.normalize().to_string() })
}

/// Parses a decimal from source text. Shared with the compiler so a literal
/// like `decimal 19.99` is validated (and made exact) at compile time.
pub fn parse_decimal(text: &str) -> Result<Value, String> {
    let trimmed = text.trim();
    Decimal::from_str_exact(trimmed)
        .map(decimal_json)
        .map_err(|error| format!("Invalid decimal \"{trimmed}\": {error}"))
}

/// Builds the canonical tagged Value for a fraction: reduced, denominator
/// positive, denominator never zero. Shared with the compiler for literal
/// validation.
pub fn fraction_json(numerator: i64, denominator: i64) -> Result<Value, String> {
    if denominator == 0 {
        return Err("Fraction denominator cannot be zero".to_string());
    }
    fn gcd(a: i64, b: i64) -> i64 {
        if b == 0 { a.abs() } else { gcd(b, a % b) }
    }
    let divisor = gcd(numerator, denominator).max(1);
    let sign = if denominator < 0 { -1 } else { 1 };
    Ok(json!({
        TYPE_TAG: "fraction",
        "numerator": sign * (numerator / divisor),
        "denominator": (denominator / divisor).abs(),
    }))
}

pub fn as_numeric(value: &Value) -> Option<Numeric> {
    match value {
        Value::Number(number) => {
            if let Some(i) = number.as_i64() {
                Some(Numeric::Int(i))
            } else {
                number.as_f64().map(Numeric::Float)
            }
        }
        Value::Object(fields) => match fields.get(TYPE_TAG).and_then(Value::as_str) {
            Some("decimal") => fields
                .get("value")
                .and_then(Value::as_str)
                .and_then(|text| Decimal::from_str_exact(text).ok())
                .map(Numeric::Dec),
            Some("fraction") => {
                let numerator = fields.get("numerator").and_then(Value::as_i64)?;
                let denominator = fields.get("denominator").and_then(Value::as_i64)?;
                (denominator != 0).then_some(Numeric::Frac(numerator, denominator))
            }
            _ => None,
        },
        _ => None,
    }
}

fn numeric_error(message: String) -> VmError {
    VmError {
        message,
        events: Vec::new(),
    }
}

/// Promotes two numerics to a common exact kind, or errors on lossy mixes.
fn promote(left: Numeric, right: Numeric) -> Result<(Numeric, Numeric), VmError> {
    use Numeric::*;
    let promoted = match (left, right) {
        (Int(_), Int(_)) | (Float(_), Float(_)) | (Dec(_), Dec(_)) | (Frac(..), Frac(..)) => {
            (left, right)
        }
        (Int(i), Float(_)) => (Float(i as f64), right),
        (Float(_), Int(i)) => (left, Float(i as f64)),
        (Int(i), Dec(_)) => (Dec(Decimal::from(i)), right),
        (Dec(_), Int(i)) => (left, Dec(Decimal::from(i))),
        (Int(i), Frac(..)) => (Frac(i, 1), right),
        (Frac(..), Int(i)) => (left, Frac(i, 1)),
        _ => {
            return Err(numeric_error(format!(
                "Cannot mix a {} with a {} in arithmetic. Convert explicitly \
                 (e.g. \"decimal of x\" or \"numeric value of x\") so precision \
                 loss is a decision, not an accident.",
                left.kind(),
                right.kind()
            )))
        }
    };
    Ok(promoted)
}

/// Floor division on i128 (rounds toward negative infinity, like Python //).
fn floor_div_i128(a: i128, b: i128) -> i128 {
    let quotient = a / b;
    let remainder = a % b;
    if remainder != 0 && (remainder < 0) != (b < 0) {
        quotient - 1
    } else {
        quotient
    }
}

fn i128_to_i64(value: i128) -> Result<i64, VmError> {
    i64::try_from(value)
        .map_err(|_| numeric_error("Integer arithmetic overflowed 64-bit integers".to_string()))
}

/// Reduces an i128 rational and builds the tagged fraction, erroring if the
/// reduced parts do not fit in 64 bits. Products of two i64s always fit in
/// i128, so intermediate math cannot overflow.
fn frac_value_i128(numerator: i128, denominator: i128) -> Result<Value, VmError> {
    fn gcd(a: i128, b: i128) -> i128 {
        if b == 0 { a.abs() } else { gcd(b, a % b) }
    }
    if denominator == 0 {
        return Err(numeric_error("Fraction denominator cannot be zero".to_string()));
    }
    let divisor = gcd(numerator, denominator).max(1);
    let numerator = i128_to_i64(numerator / divisor)?;
    let denominator = i128_to_i64(denominator / divisor)?;
    fraction_json(numerator, denominator).map_err(numeric_error)
}

/// Integer power of a Decimal by repeated squaring with checked multiplies,
/// so overflow is a loud error, not a panic. Negative exponents invert.
fn decimal_int_pow(base: Decimal, exponent: i64) -> Result<Decimal, VmError> {
    let overflow =
        || numeric_error("Decimal arithmetic overflowed the exact range".to_string());
    let (mut base, mut exponent, invert) = if exponent < 0 {
        if base.is_zero() {
            return Err(numeric_error(
                "Zero cannot be raised to a negative power".to_string(),
            ));
        }
        (base, exponent.unsigned_abs(), true)
    } else {
        (base, exponent as u64, false)
    };
    let mut result = Decimal::ONE;
    while exponent > 0 {
        if exponent & 1 == 1 {
            result = result.checked_mul(base).ok_or_else(overflow)?;
        }
        exponent >>= 1;
        if exponent > 0 {
            base = base.checked_mul(base).ok_or_else(overflow)?;
        }
    }
    if invert {
        Decimal::ONE.checked_div(result).ok_or_else(overflow)
    } else {
        Ok(result)
    }
}

/// Exact arithmetic across the numeric tower. `op` is the bytecode name.
pub fn numeric_binary(op: &str, left: Numeric, right: Numeric) -> Result<Value, VmError> {
    use Numeric::*;
    let (left, right) = promote(left, right)?;
    match (left, right) {
        (Int(a), Int(b)) => {
            let result = match op {
                "ADD" => a.checked_add(b),
                "SUB" => a.checked_sub(b),
                "MUL" => a.checked_mul(b),
                // Plain division stays float; `integer divided by` is IDIV.
                "DIV" => return number_value(a as f64 / b as f64),
                // Python semantics: result sign follows the divisor and
                // division floors (DEVL-136). i128 intermediates dodge the
                // i64::MIN edge cases.
                "MOD" => {
                    if b == 0 {
                        return Err(numeric_error("Modulo by zero".to_string()));
                    }
                    let (a, b) = (a as i128, b as i128);
                    i128_to_i64(((a % b) + b) % b).map(Some)?
                }
                "IDIV" => {
                    if b == 0 {
                        return Err(numeric_error("Integer division by zero".to_string()));
                    }
                    i128_to_i64(floor_div_i128(a as i128, b as i128)).map(Some)?
                }
                "POW" => {
                    if b < 0 {
                        return number_value((a as f64).powf(b as f64));
                    }
                    let exponent = u32::try_from(b).ok();
                    exponent.and_then(|exponent| a.checked_pow(exponent))
                }
                _ => unreachable!(),
            };
            result.map(|value| Value::Number(Number::from(value))).ok_or_else(|| {
                numeric_error("Integer arithmetic overflowed 64-bit integers".to_string())
            })
        }
        (Float(a), Float(b)) => {
            if b == 0.0 && matches!(op, "MOD" | "IDIV") {
                return Err(numeric_error(if op == "MOD" {
                    "Modulo by zero".to_string()
                } else {
                    "Integer division by zero".to_string()
                }));
            }
            let result = match op {
                "ADD" => a + b,
                "SUB" => a - b,
                "MUL" => a * b,
                "DIV" => a / b,
                // Python float semantics: % follows the divisor's sign,
                // // floors.
                "MOD" => a - b * (a / b).floor(),
                "IDIV" => (a / b).floor(),
                "POW" => a.powf(b),
                _ => unreachable!(),
            };
            number_value(result)
        }
        (Dec(a), Dec(b)) => {
            if b.is_zero() && matches!(op, "DIV" | "MOD" | "IDIV") {
                return Err(numeric_error(match op {
                    "MOD" => "Modulo by zero".to_string(),
                    "IDIV" => "Integer division by zero".to_string(),
                    _ => "Division of a decimal by zero".to_string(),
                }));
            }
            let result = match op {
                "ADD" => a.checked_add(b),
                "SUB" => a.checked_sub(b),
                "MUL" => a.checked_mul(b),
                "DIV" => a.checked_div(b),
                // Python Decimal semantics: % keeps the dividend's sign and
                // // truncates the true quotient toward zero.
                "MOD" => a.checked_rem(b),
                "IDIV" => a.checked_div(b).map(|quotient| quotient.trunc()),
                "POW" => {
                    if !b.fract().is_zero() {
                        return Err(numeric_error(
                            "Decimal exponents must be whole numbers; convert with \
                             \"numeric value of x\" for fractional powers"
                                .to_string(),
                        ));
                    }
                    let exponent = b.to_i64().ok_or_else(|| {
                        numeric_error("Decimal exponent is too large".to_string())
                    })?;
                    decimal_int_pow(a, exponent).map(Some)?
                }
                _ => unreachable!(),
            };
            result.map(decimal_json).ok_or_else(|| {
                numeric_error("Decimal arithmetic overflowed the exact range".to_string())
            })
        }
        (Frac(a, b), Frac(c, d)) => {
            let (a, b, c, d) = (a as i128, b as i128, c as i128, d as i128);
            if c == 0 && matches!(op, "DIV" | "MOD" | "IDIV") {
                return Err(numeric_error(match op {
                    "MOD" => "Modulo by zero".to_string(),
                    "IDIV" => "Integer division by zero".to_string(),
                    _ => "Division of a fraction by zero".to_string(),
                }));
            }
            match op {
                "ADD" => frac_value_i128(a * d + c * b, b * d),
                "SUB" => frac_value_i128(a * d - c * b, b * d),
                "MUL" => frac_value_i128(a * c, b * d),
                "DIV" => frac_value_i128(a * d, b * c),
                // Python Fraction semantics: floor division and floor-mod.
                "IDIV" => {
                    i128_to_i64(floor_div_i128(a * d, b * c)).map(|q| json!(q))
                }
                "MOD" => {
                    let quotient = floor_div_i128(a * d, b * c);
                    frac_value_i128(a * d - quotient * c * b, b * d)
                }
                "POW" => {
                    if d != 1 {
                        return Err(numeric_error(
                            "Fraction exponents must be whole numbers".to_string(),
                        ));
                    }
                    let exponent = c;
                    let (base_n, base_d, exponent) = if exponent < 0 {
                        if a == 0 {
                            return Err(numeric_error(
                                "Zero cannot be raised to a negative power".to_string(),
                            ));
                        }
                        (b, a, -exponent)
                    } else {
                        (a, b, exponent)
                    };
                    let exponent = u32::try_from(exponent).map_err(|_| {
                        numeric_error("Fraction exponent is too large".to_string())
                    })?;
                    let numerator = base_n.checked_pow(exponent);
                    let denominator = base_d.checked_pow(exponent);
                    match (numerator, denominator) {
                        (Some(n), Some(d)) => frac_value_i128(n, d),
                        _ => Err(numeric_error(
                            "Fraction arithmetic overflowed 64-bit integers".to_string(),
                        )),
                    }
                }
                _ => unreachable!(),
            }
        }
        _ => unreachable!("promote returns matching kinds"),
    }
}

/// All items as numerics, but only when at least one is a tagged exact type —
/// plain number lists keep the legacy f64 aggregation paths.
fn exact_numeric_items(items: &[Value]) -> Option<Vec<Numeric>> {
    if !items.iter().any(|item| tagged_numeric_kind(item).is_some()) {
        return None;
    }
    items.iter().map(as_numeric).collect()
}

fn exact_sum(numerics: &[Numeric]) -> Result<Value, VmError> {
    let mut total = Numeric::Int(0);
    let mut total_value = Value::Number(Number::from(0));
    for numeric in numerics {
        total_value = numeric_binary("ADD", total, *numeric)?;
        total = as_numeric(&total_value).expect("numeric_binary returns numerics");
    }
    Ok(total_value)
}

fn to_decimal(value: &Value) -> Result<Value, VmError> {
    let error = |message: String| VmError {
        message,
        events: Vec::new(),
    };
    match as_numeric(value) {
        Some(Numeric::Dec(d)) => Ok(decimal_json(d)),
        Some(Numeric::Int(i)) => Ok(decimal_json(Decimal::from(i))),
        // Through the shortest display form, so `decimal of 19.99` gives
        // exactly 19.99, not the f64's nearest binary neighbor.
        Some(Numeric::Float(f)) => parse_decimal(&format!("{f}")).map_err(error),
        Some(Numeric::Frac(n, d)) => Decimal::from(n)
            .checked_div(Decimal::from(d))
            .map(decimal_json)
            .ok_or_else(|| error(format!("Cannot represent {n}/{d} as a decimal"))),
        None => match value {
            Value::String(text) => parse_decimal(text).map_err(error),
            other => Err(error(format!("Cannot convert {other} to a decimal"))),
        },
    }
}

fn rounding_strategy(mode: &str) -> Option<rust_decimal::RoundingStrategy> {
    use rust_decimal::RoundingStrategy::*;
    match mode.trim().to_lowercase().replace('_', " ").as_str() {
        "half even" | "bankers" => Some(MidpointNearestEven),
        "half up" => Some(MidpointAwayFromZero),
        "half down" => Some(MidpointTowardZero),
        "up" => Some(AwayFromZero),
        "down" | "truncate" => Some(ToZero),
        "ceiling" => Some(ToPositiveInfinity),
        "floor" => Some(ToNegativeInfinity),
        _ => None,
    }
}

/// Ordering across all numeric kinds. Exact within a promotable family,
/// lenient (via f64) across decimal/float/fraction so comparisons and sorting
/// never error.
pub fn numeric_compare(left: Numeric, right: Numeric) -> std::cmp::Ordering {
    use Numeric::*;
    match (left, right) {
        (Int(a), Int(b)) => a.cmp(&b),
        (Dec(a), Dec(b)) => a.cmp(&b),
        (Int(i), Dec(d)) => Decimal::from(i).cmp(&d),
        (Dec(d), Int(i)) => d.cmp(&Decimal::from(i)),
        (Frac(a, b), Frac(c, d)) => match (a.checked_mul(d), c.checked_mul(b)) {
            (Some(x), Some(y)) => x.cmp(&y),
            _ => left
                .to_f64()
                .partial_cmp(&right.to_f64())
                .unwrap_or(std::cmp::Ordering::Equal),
        },
        (Int(i), Frac(c, d)) => numeric_compare(Frac(i, 1), Frac(c, d)),
        (Frac(a, b), Int(i)) => numeric_compare(Frac(a, b), Frac(i, 1)),
        _ => left
            .to_f64()
            .partial_cmp(&right.to_f64())
            .unwrap_or(std::cmp::Ordering::Equal),
    }
}

pub fn number_value(value: f64) -> Result<Value, VmError> {
    if value.fract() == 0.0 {
        Ok(Value::Number(Number::from(value as i64)))
    } else {
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| VmError {
                message: "Invalid numeric result".to_string(),
                events: Vec::new(),
            })
    }
}

pub fn truthy(value: &Value) -> bool {
    !matches!(value, Value::Null | Value::Bool(false))
}

pub fn file_content(value: &Value, mode: &str) -> String {
    if mode == "csv" {
        return csv_content(value);
    }
    if let Value::String(text) = value {
        return text.clone();
    }
    if mode == "export" && matches!(value, Value::Array(_) | Value::Object(_)) {
        return serde_json::to_string_pretty(value).unwrap_or_default();
    }

    match value {
        Value::Null => String::new(),
        Value::Bool(boolean) => boolean.to_string(),
        Value::Number(number) => number.to_string(),
        _ => value.to_string(),
    }
}

fn parse_file_content(raw: &Value, format: &str) -> Result<Value, String> {
    match format {
        "json" => match raw {
            Value::String(text) => {
                serde_json::from_str(text).map_err(|error| format!("invalid JSON: {error}"))
            }
            other => Ok(other.clone()),
        },
        "csv" => {
            let text = raw
                .as_str()
                .ok_or_else(|| "CSV input must be text".to_string())?;
            parse_csv(text)
        }
        "text" => Ok(raw.clone()),
        other => Err(format!("unsupported file format: {other}")),
    }
}

fn csv_content(value: &Value) -> String {
    match value {
        Value::Array(items) => csv_rows(items),
        Value::Object(_) => csv_rows(std::slice::from_ref(value)),
        Value::Null => String::new(),
        other => {
            let rows = vec![json!({ "value": other })];
            csv_rows(&rows)
        }
    }
}

fn csv_rows(rows: &[Value]) -> String {
    if rows.is_empty() {
        return String::new();
    }

    let mut headers = Vec::<String>::new();
    for row in rows {
        if let Some(object) = row.as_object() {
            for key in object.keys() {
                if !headers.contains(key) {
                    headers.push(key.clone());
                }
            }
        }
    }
    if headers.is_empty() {
        headers.push("value".to_string());
    }

    let mut lines = Vec::new();
    lines.push(
        headers
            .iter()
            .map(|header| csv_escape(header))
            .collect::<Vec<_>>()
            .join(","),
    );
    for row in rows {
        let values = headers
            .iter()
            .map(|header| {
                let value = row
                    .as_object()
                    .and_then(|object| object.get(header))
                    .unwrap_or(row);
                csv_escape(&csv_value(value))
            })
            .collect::<Vec<_>>()
            .join(",");
        lines.push(values);
    }
    lines.join("\n")
}

fn csv_value(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Bool(boolean) => boolean.to_string(),
        Value::Number(number) => number.to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn parse_csv(text: &str) -> Result<Value, String> {
    let mut lines = text.lines().filter(|line| !line.trim().is_empty());
    let Some(header_line) = lines.next() else {
        return Ok(Value::Array(Vec::new()));
    };
    let headers = parse_csv_line(header_line)?;
    let mut rows = Vec::new();
    for line in lines {
        let values = parse_csv_line(line)?;
        let mut row = Map::new();
        for (index, header) in headers.iter().enumerate() {
            row.insert(
                sanitize_csv_header(header),
                values
                    .get(index)
                    .cloned()
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            );
        }
        rows.push(Value::Object(row));
    }
    Ok(Value::Array(rows))
}

fn parse_csv_line(line: &str) -> Result<Vec<String>, String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;

    while let Some(ch) = chars.next() {
        match ch {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                fields.push(current.clone());
                current.clear();
            }
            other => current.push(other),
        }
    }

    if in_quotes {
        return Err("unterminated quoted CSV field".to_string());
    }
    fields.push(current);
    Ok(fields)
}

fn sanitize_csv_header(value: &str) -> String {
    let mut out = String::new();
    let mut previous_underscore = false;
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch.to_ascii_lowercase());
            previous_underscore = false;
        } else if !previous_underscore {
            out.push('_');
            previous_underscore = true;
        }
    }
    let trimmed = out.trim_matches('_');
    if trimmed.is_empty() {
        "field".to_string()
    } else {
        trimmed.to_string()
    }
}

fn set_path_value(root: &mut Value, path: &[String], value: Value) -> Result<(), String> {
    if path.is_empty() {
        *root = value;
        return Ok(());
    }

    if root.is_null() {
        *root = json!({});
    }
    let mut current = root;
    for field in &path[..path.len() - 1] {
        let object = current
            .as_object_mut()
            .ok_or_else(|| format!("{field} cannot be set on a non-record value"))?;
        current = object.entry(field.clone()).or_insert_with(|| json!({}));
        if current.is_null() {
            *current = json!({});
        }
    }

    let last = path.last().expect("path is not empty");
    let object = current
        .as_object_mut()
        .ok_or_else(|| format!("{last} cannot be set on a non-record value"))?;
    object.insert(last.clone(), value);
    Ok(())
}

fn evaluate_assertion(
    assertion_id: &str,
    target: &str,
    operator: &str,
    actual: Value,
    expected: Value,
) -> Result<Value, VmError> {
    let passed = match operator {
        "equals" => actual_value(&actual) == actual_value(&expected),
        "contains" => actual_value(&actual)
            .as_str()
            .unwrap_or_default()
            .contains(actual_value(&expected).as_str().unwrap_or_default()),
        "present" => present_cell(&actual),
        "not_spreadsheet_error" => !spreadsheet_error(&actual),
        _ => {
            return Err(VmError {
                message: format!("Unsupported assertion operator: {operator}"),
                events: Vec::new(),
            })
        }
    };
    Ok(json!({
        "id": assertion_id,
        "target": target,
        "operator": operator,
        "expected": if expected == Value::Null { Value::Null } else { typed_value(&expected) },
        "actual": typed_value(&actual),
        "passed": passed,
        "message": assertion_message(assertion_id, target, operator, &expected, &actual, passed)
    }))
}

fn validate_value(actual: &Value, rule: &str, expected: &Value) -> bool {
    match rule {
        "minimum" => number_as_f64(actual) >= number_as_f64(expected),
        "maximum" => number_as_f64(actual) <= number_as_f64(expected),
        "equals" => actual_value(actual) == actual_value(expected),
        "contains" => contains_value(actual, expected),
        "matches" => wildcard_match(&value_as_str(actual), &value_as_str(expected)),
        "present" => present_cell(actual),
        "missing" => !present_cell(actual),
        "one_of" => as_array(expected).contains(actual),
        _ => false,
    }
}

fn validation_message(
    target: &str,
    rule: &str,
    expected: &Value,
    actual: &Value,
    passed: bool,
) -> String {
    if passed {
        return format!("Validation passed: {target} {rule}");
    }
    match rule {
        "minimum" => format!("Validation failed: {target} below minimum {expected}"),
        "maximum" => format!("Validation failed: {target} above maximum {expected}"),
        "equals" => {
            format!("Validation failed: expected {target} to equal {expected}, got {actual}")
        }
        "contains" => format!("Validation failed: expected {target} to contain {expected}"),
        "matches" => format!("Validation failed: expected {target} to match {expected}"),
        "present" => format!("Validation failed: expected {target} to be present"),
        "missing" => format!("Validation failed: expected {target} to be missing"),
        "one_of" => format!("Validation failed: expected {target} to be one of {expected}"),
        _ => format!("Validation failed: {target} {rule}"),
    }
}

fn contains_value(actual: &Value, expected: &Value) -> bool {
    if let Some(items) = actual.as_array() {
        return items.contains(expected);
    }
    value_as_str(actual).contains(&value_as_str(expected))
}

fn wildcard_match(actual: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return actual == pattern;
    }
    let mut position = 0usize;
    for (index, part) in parts
        .iter()
        .enumerate()
        .filter(|(_, part)| !part.is_empty())
    {
        if index == 0 && !pattern.starts_with('*') {
            if !actual[position..].starts_with(part) {
                return false;
            }
            position += part.len();
            continue;
        }
        let Some(found) = actual[position..].find(part) else {
            return false;
        };
        position += found + part.len();
    }
    if !pattern.ends_with('*') {
        let last = parts.last().copied().unwrap_or_default();
        return actual.ends_with(last);
    }
    true
}

fn actual_value(value: &Value) -> Value {
    value
        .as_object()
        .and_then(|object| object.get("value"))
        .cloned()
        .unwrap_or_else(|| value.clone())
}

fn typed_value(value: &Value) -> Value {
    if value
        .as_object()
        .and_then(|object| object.get("kind"))
        .is_some()
    {
        return value.clone();
    }

    let kind = match value {
        Value::Null => "blank",
        Value::String(_) => "string",
        Value::Number(_) => "number",
        Value::Bool(_) => "boolean",
        _ => "object",
    };
    json!({
        "kind": kind,
        "value": value
    })
}

fn present_cell(value: &Value) -> bool {
    if value.is_null() {
        return false;
    }
    if let Some(kind) = value
        .as_object()
        .and_then(|object| object.get("kind"))
        .and_then(Value::as_str)
    {
        return kind != "missing" && kind != "blank";
    }
    value.as_str() != Some("")
}

fn spreadsheet_error(value: &Value) -> bool {
    value
        .as_object()
        .and_then(|object| object.get("kind"))
        .and_then(Value::as_str)
        == Some("error")
}

fn assertion_message(
    assertion_id: &str,
    target: &str,
    operator: &str,
    expected: &Value,
    actual: &Value,
    passed: bool,
) -> String {
    if passed {
        return format!("Assertion {assertion_id} passed");
    }
    match operator {
        "equals" => format!(
            "Expected {target} to equal {expected}, got {}",
            actual_value(actual)
        ),
        "contains" => format!("Expected {target} to contain {}", actual_value(expected)),
        "present" => format!("Expected {target} to be present"),
        "not_spreadsheet_error" => {
            format!("Expected {target} to not be a spreadsheet error")
        }
        _ => format!("Assertion {assertion_id} failed"),
    }
}

pub fn map_from_pairs(pairs: Vec<(&str, Value)>) -> Map<String, Value> {
    pairs
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

fn execute_builtin(name: &str, args: &[Value]) -> Result<Value, VmError> {
    let result = match name {
        "count" => json!(as_array(&args[0]).len()),
        "first" => as_array(&args[0]).first().cloned().unwrap_or(Value::Null),
        "last" => as_array(&args[0]).last().cloned().unwrap_or(Value::Null),
        "unique" => {
            let mut seen = Vec::new();
            for item in as_array(&args[0]) {
                if !seen.contains(&item) {
                    seen.push(item.clone());
                }
            }
            Value::Array(seen)
        }
        "flatten" => {
            let mut flat = Vec::new();
            for item in as_array(&args[0]) {
                if let Value::Array(inner) = item {
                    flat.extend(inner.iter().cloned());
                } else {
                    flat.push(item.clone());
                }
            }
            Value::Array(flat)
        }
        "minimum" => {
            let items = as_array(&args[0]);
            items
                .iter()
                .min_by(|a, b| {
                    number_as_f64(a)
                        .partial_cmp(&number_as_f64(b))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .cloned()
                .unwrap_or(Value::Null)
        }
        "maximum" => {
            let items = as_array(&args[0]);
            items
                .iter()
                .max_by(|a, b| {
                    number_as_f64(a)
                        .partial_cmp(&number_as_f64(b))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .cloned()
                .unwrap_or(Value::Null)
        }
        "sum" => match exact_numeric_items(&as_array(&args[0])) {
            Some(numerics) => exact_sum(&numerics)?,
            None => {
                let total: f64 = as_array(&args[0]).iter().map(number_as_f64).sum();
                number_value(total).unwrap_or(Value::Null)
            }
        },
        "average" => {
            let items = as_array(&args[0]);
            if items.is_empty() {
                json!(0)
            } else if let Some(numerics) = exact_numeric_items(&items) {
                let total = exact_sum(&numerics)?;
                let total = as_numeric(&total).expect("exact sum is numeric");
                numeric_binary("DIV", total, Numeric::Int(items.len() as i64))?
            } else {
                let total: f64 = items.iter().map(number_as_f64).sum();
                number_value(total / items.len() as f64).unwrap_or(Value::Null)
            }
        }
        "reverse" => Value::Array(as_array(&args[0]).iter().rev().cloned().collect()),
        "sort" => {
            // Numbers (plain or tagged decimal/fraction) sort numerically;
            // anything else keeps the stringified ordering.
            let compare = |a: &Value, b: &Value| match (as_numeric(a), as_numeric(b)) {
                (Some(left), Some(right)) => numeric_compare(left, right),
                _ => a.to_string().cmp(&b.to_string()),
            };
            let mut items = as_array(&args[0]).clone();
            if let Some(field) = args.get(1).map(value_as_str) {
                items.sort_by(|a, b| compare(&field_value(a, &field), &field_value(b, &field)));
            } else {
                items.sort_by(compare);
            }
            Value::Array(items)
        }
        // Sorts args[0] by the parallel per-element key list in args[1]
        // (computed by the compiler's inlined key loop, DEVL-132). Numeric
        // keys compare numerically; anything else falls back to the same
        // stringified ordering `sort` uses. Stable, so equal keys keep
        // source order.
        "sort_by_keys" => {
            let items = as_array(&args[0]).clone();
            let keys = as_array(args.get(1).unwrap_or(&Value::Null)).clone();
            let mut pairs: Vec<(Value, Value)> = items
                .into_iter()
                .enumerate()
                .map(|(i, item)| (keys.get(i).cloned().unwrap_or(Value::Null), item))
                .collect();
            let all_numeric = pairs.iter().all(|(key, _)| key.is_number());
            if all_numeric {
                pairs.sort_by(|a, b| {
                    number_as_f64(&a.0)
                        .partial_cmp(&number_as_f64(&b.0))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            } else {
                pairs.sort_by(|a, b| a.0.to_string().cmp(&b.0.to_string()));
            }
            Value::Array(pairs.into_iter().map(|(_, item)| item).collect())
        }
        // Regex primitive (DEVL-133). Pure and deterministic: no effect
        // journaling needed. Invalid patterns fail loudly. The optional
        // trailing flags argument is a string of `i` (ignore case),
        // `m` (multiline), `s` (dot matches newline).
        "regex_test" => {
            let regex = build_regex(args.get(1), args.get(2))?;
            Value::Bool(regex.is_match(&value_as_str(&args[0])))
        }
        "regex_match" => {
            let regex = build_regex(args.get(1), args.get(2))?;
            let text = value_as_str(&args[0]);
            match regex.captures(&text) {
                Some(captures) => match_record(&regex, &captures, &text),
                None => Value::Null,
            }
        }
        "regex_find_all" => {
            let regex = build_regex(args.get(1), args.get(2))?;
            let text = value_as_str(&args[0]);
            Value::Array(
                regex
                    .find_iter(&text)
                    .map(|m| Value::String(m.as_str().to_string()))
                    .collect(),
            )
        }
        "regex_replace" => {
            let regex = build_regex(args.get(1), args.get(3))?;
            let text = value_as_str(&args[0]);
            let replacement = value_as_str(args.get(2).unwrap_or(&Value::Null));
            Value::String(regex.replace_all(&text, replacement.as_str()).to_string())
        }
        "regex_split" => {
            let regex = build_regex(args.get(1), args.get(2))?;
            let text = value_as_str(&args[0]);
            Value::Array(
                regex
                    .split(&text)
                    .map(|part| Value::String(part.to_string()))
                    .collect(),
            )
        }
        "find_where" => find_where(args),
        "filter_where" => filter_where(args, false),
        "reject_where" => filter_where(args, true),
        "any_where" => Value::Bool(
            filter_where(args, false)
                .as_array()
                .is_some_and(|items| !items.is_empty()),
        ),
        "all_where" => {
            let items = as_array(&args[0]);
            let matched = filter_where(args, false)
                .as_array()
                .map(|matched| matched.len())
                .unwrap_or(0);
            Value::Bool(!items.is_empty() && matched == items.len())
        }
        "partition_where" => partition_where(args),
        "group_by" => group_by(args),
        "index_by" => index_by(args),
        "take" => {
            let items = as_array(&args[0]);
            let count = number_as_f64(&args[1]).max(0.0) as usize;
            Value::Array(items.into_iter().take(count).collect())
        }
        "drop" => {
            let items = as_array(&args[0]);
            let count = number_as_f64(&args[1]).max(0.0) as usize;
            Value::Array(items.into_iter().skip(count).collect())
        }
        "zip" => {
            let left = as_array(&args[0]);
            let right = as_array(&args[1]);
            Value::Array(
                left.into_iter()
                    .zip(right)
                    .map(|(left, right)| json!({ "left": left, "right": right }))
                    .collect(),
            )
        }
        "chunk" => {
            let items = as_array(&args[0]);
            let size = number_as_f64(&args[1]).max(1.0) as usize;
            Value::Array(
                items
                    .chunks(size)
                    .map(|chunk| Value::Array(chunk.to_vec()))
                    .collect(),
            )
        }
        "union" => Value::Array(unique_values(
            as_array(&args[0])
                .into_iter()
                .chain(as_array(&args[1]))
                .collect(),
        )),
        "intersection" => {
            let left = as_array(&args[0]);
            let right = as_array(&args[1]);
            Value::Array(unique_values(
                left.into_iter()
                    .filter(|item| right.contains(item))
                    .collect(),
            ))
        }
        "difference" => {
            let right = as_array(&args[1]);
            Value::Array(unique_values(
                as_array(&args[0])
                    .into_iter()
                    .filter(|item| !right.contains(item))
                    .collect(),
            ))
        }
        "map_transform" => map_transform(args),
        "pluck" => {
            let field = value_as_str(&args[1]);
            Value::Array(
                as_array(&args[0])
                    .iter()
                    .map(|item| field_value(item, &field))
                    .collect(),
            )
        }
        "reduce_count" => {
            let total = number_as_f64(&args[1]) + as_array(&args[0]).len() as f64;
            number_value(total).unwrap_or(Value::Null)
        }
        "uppercase" => Value::String(value_as_str(&args[0]).to_uppercase()),
        "lowercase" => Value::String(value_as_str(&args[0]).to_lowercase()),
        "trim" => Value::String(value_as_str(&args[0]).trim().to_string()),
        "normalize_whitespace" => Value::String(normalize_whitespace(&value_as_str(&args[0]))),
        "slugify" => Value::String(slugify(&value_as_str(&args[0]))),
        "title_case" => Value::String(title_case(&value_as_str(&args[0]))),
        "sentence_case" => Value::String(sentence_case(&value_as_str(&args[0]))),
        "words" => Value::Array(
            value_as_str(&args[0])
                .split_whitespace()
                .map(|word| Value::String(word.to_string()))
                .collect(),
        ),
        "contains_text" => Value::Bool(value_as_str(&args[0]).contains(&value_as_str(&args[1]))),
        "starts_with_text" => {
            Value::Bool(value_as_str(&args[0]).starts_with(&value_as_str(&args[1])))
        }
        "ends_with_text" => Value::Bool(value_as_str(&args[0]).ends_with(&value_as_str(&args[1]))),
        "date_parse" => Value::String(
            parse_iso_date(&value_as_str(&args[0]))
                .map(format_iso_date)
                .unwrap_or_default(),
        ),
        "date_add_days" => {
            let date = parse_iso_date(&value_as_str(&args[0]));
            let days = number_as_f64(&args[1]) as i64;
            Value::String(
                date.map(|(year, month, day)| {
                    format_iso_date(civil_from_days(days_from_civil(year, month, day) + days))
                })
                .unwrap_or_default(),
            )
        }
        "days_between" => {
            let start = parse_iso_date(&value_as_str(&args[0]));
            let end = parse_iso_date(&value_as_str(&args[1]));
            match (start, end) {
                (Some(start), Some(end)) => number_value(
                    (days_from_civil(end.0, end.1, end.2)
                        - days_from_civil(start.0, start.1, start.2)) as f64,
                )
                .unwrap_or(Value::Null),
                _ => Value::Null,
            }
        }
        "business_days_between" => {
            let start = parse_iso_date(&value_as_str(&args[0]));
            let end = parse_iso_date(&value_as_str(&args[1]));
            match (start, end) {
                (Some(start), Some(end)) => json!(business_days_between(start, end)),
                _ => Value::Null,
            }
        }
        "length" => {
            let v = &args[0];
            if let Some(arr) = v.as_array() {
                json!(arr.len())
            } else if let Some(s) = v.as_str() {
                json!(s.len())
            } else {
                json!(v.to_string().len())
            }
        }
        "round" => match as_numeric(&args[0]) {
            Some(Numeric::Dec(d)) => {
                let rounded = d.round();
                rounded.to_i64().map(|i| json!(i)).ok_or_else(|| VmError {
                    message: format!("Decimal {d} is too large to round to an integer"),
                    events: Vec::new(),
                })?
            }
            Some(Numeric::Frac(n, d)) => {
                number_value((n as f64 / d as f64).round()).unwrap_or(Value::Null)
            }
            _ => number_value(number_as_f64(&args[0]).round()).unwrap_or(Value::Null),
        },
        "abs" | "absolute_value" => match as_numeric(&args[0]) {
            Some(Numeric::Dec(d)) => decimal_json(d.abs()),
            Some(Numeric::Frac(n, d)) => {
                fraction_json(n.abs(), d).map_err(|message| VmError {
                    message,
                    events: Vec::new(),
                })?
            }
            _ => number_value(number_as_f64(&args[0]).abs()).unwrap_or(Value::Null),
        },
        // Numeric tower conversions and rounding (DEVL-134).
        "to_decimal" => to_decimal(&args[0])?,
        "to_fraction" => {
            let (Some(Numeric::Int(numerator)), Some(Numeric::Int(denominator))) = (
                as_numeric(&args[0]),
                as_numeric(args.get(1).unwrap_or(&Value::Null)),
            ) else {
                return Err(VmError {
                    message: "Fractions are built from two whole numbers".to_string(),
                    events: Vec::new(),
                });
            };
            fraction_json(numerator, denominator).map_err(|message| VmError {
                message,
                events: Vec::new(),
            })?
        }
        "decimal_round" => {
            let decimal_value_arg = to_decimal(&args[0])?;
            let Some(Numeric::Dec(decimal)) = as_numeric(&decimal_value_arg) else {
                unreachable!("to_decimal returns a decimal");
            };
            let places = as_numeric(args.get(1).unwrap_or(&Value::Null));
            let Some(Numeric::Int(places)) = places else {
                return Err(VmError {
                    message: "Rounding needs a whole number of decimal places".to_string(),
                    events: Vec::new(),
                });
            };
            if !(0..=28).contains(&places) {
                return Err(VmError {
                    message: format!("Decimal places must be between 0 and 28, got {places}"),
                    events: Vec::new(),
                });
            }
            let mode = match args.get(2) {
                Some(Value::String(mode)) => mode.clone(),
                _ => "half even".to_string(),
            };
            let strategy = rounding_strategy(&mode).ok_or_else(|| VmError {
                message: format!(
                    "Unknown rounding mode \"{mode}\" (expected half even, half up, \
                     half down, up, down, ceiling, or floor)"
                ),
                events: Vec::new(),
            })?;
            decimal_json(decimal.round_dp_with_strategy(places as u32, strategy))
        }
        "to_number" => match as_numeric(&args[0]) {
            Some(numeric) => number_value(numeric.to_f64())?,
            None => match &args[0] {
                Value::String(text) => match text.trim().parse::<f64>() {
                    Ok(parsed) => number_value(parsed)?,
                    Err(_) => {
                        return Err(VmError {
                            message: format!("\"{text}\" is not a number"),
                            events: Vec::new(),
                        })
                    }
                },
                other => {
                    return Err(VmError {
                        message: format!("Cannot convert {other} to a number"),
                        events: Vec::new(),
                    })
                }
            },
        },
        "replace" => {
            let source = value_as_str(&args[0]);
            let from = value_as_str(&args[1]);
            let to = value_as_str(&args[2]);
            Value::String(source.replace(&from, &to))
        }
        "split" => {
            let source = value_as_str(&args[0]);
            let delimiter = value_as_str(&args[1]);
            Value::Array(
                source
                    .split(&delimiter)
                    .map(|s| Value::String(s.to_string()))
                    .collect(),
            )
        }
        "join" => {
            let items = as_array(&args[0]);
            let separator = value_as_str(&args[1]);
            let joined: Vec<String> = items.iter().map(|v| value_as_str(v)).collect();
            Value::String(joined.join(&separator))
        }
        "item" => {
            let items = as_array(&args[0]);
            let index = number_as_f64(&args[1]) as usize;
            let zero_index = if index > 0 { index - 1 } else { 0 };
            items.get(zero_index).cloned().unwrap_or(Value::Null)
        }
        "slice" => {
            let items = as_array(&args[0]);
            let start = (number_as_f64(&args[1]) as usize).saturating_sub(1);
            let end = number_as_f64(&args[2]) as usize;
            Value::Array(items.get(start..end).unwrap_or_default().to_vec())
        }
        "keys" => {
            if let Some(obj) = args[0].as_object() {
                Value::Array(obj.keys().map(|k| Value::String(k.clone())).collect())
            } else {
                Value::Array(Vec::new())
            }
        }
        "values" => {
            if let Some(obj) = args[0].as_object() {
                Value::Array(obj.values().cloned().collect())
            } else {
                Value::Array(Vec::new())
            }
        }
        "entries" => {
            if let Some(obj) = args[0].as_object() {
                Value::Array(
                    obj.iter()
                        .map(|(k, v)| json!({"key": k, "value": v}))
                        .collect(),
                )
            } else {
                Value::Array(Vec::new())
            }
        }
        "has_fields" => Value::Bool(has_fields(
            args.first().unwrap_or(&Value::Null),
            args.get(1).unwrap_or(&Value::Null),
        )),
        "matches_shape" => Value::Bool(match args.get(1) {
            Some(shape) => matches_shape(args.first().unwrap_or(&Value::Null), shape),
            None => false,
        }),
        "type_of" => {
            let t = match &args[0] {
                Value::Null => "nil",
                Value::String(_) => "string",
                Value::Number(_) => "number",
                Value::Bool(_) => "boolean",
                Value::Array(_) => "list",
                Value::Object(_) => tagged_numeric_kind(&args[0]).unwrap_or("record"),
            };
            Value::String(t.to_string())
        }
        _ => {
            return Err(VmError {
                message: format!("Unknown builtin function: {name}"),
                events: Vec::new(),
            })
        }
    };
    Ok(result)
}

fn as_array(value: &Value) -> Vec<Value> {
    value.as_array().cloned().unwrap_or_default()
}

fn value_as_str(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Object(fields) => match tagged_numeric_kind(value) {
            Some("decimal") => fields
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            Some("fraction") => format!(
                "{}/{}",
                fields.get("numerator").and_then(Value::as_i64).unwrap_or(0),
                fields.get("denominator").and_then(Value::as_i64).unwrap_or(1),
            ),
            _ => value.to_string(),
        },
        other => other.to_string(),
    }
}

/// Compiles a regex from the pattern and optional flags arguments. Exposed so
/// the compiler can validate literal patterns at compile time with identical
/// semantics (DEVL-133).
pub fn compile_regex(pattern: &str, flags: &str) -> Result<regex::Regex, String> {
    let mut inline = String::new();
    for flag in flags.chars() {
        match flag {
            'i' | 'm' | 's' => inline.push(flag),
            _ => return Err(format!("Unknown regex flag '{flag}' (expected i, m, or s)")),
        }
    }
    let source = if inline.is_empty() {
        pattern.to_string()
    } else {
        format!("(?{inline}){pattern}")
    };
    regex::Regex::new(&source)
        .map_err(|error| format!("Invalid regular expression \"{pattern}\": {error}"))
}

fn build_regex(pattern: Option<&Value>, flags: Option<&Value>) -> Result<regex::Regex, VmError> {
    let pattern = value_as_str(pattern.unwrap_or(&Value::Null));
    let flags = match flags {
        Some(Value::String(flags)) => flags.clone(),
        _ => String::new(),
    };
    compile_regex(&pattern, &flags).map_err(|message| VmError {
        message,
        events: Vec::new(),
    })
}

/// One match as a record: the matched text, character offsets, positional
/// capture groups, and named capture groups (unmatched groups are null).
fn match_record(regex: &regex::Regex, captures: &regex::Captures, text: &str) -> Value {
    let whole = captures.get(0).expect("group 0 always present");
    let char_offset = |byte: usize| text[..byte].chars().count();
    let group_value = |group: Option<regex::Match>| match group {
        Some(group) => Value::String(group.as_str().to_string()),
        None => Value::Null,
    };
    let groups: Vec<Value> = captures
        .iter()
        .skip(1)
        .map(|group| group_value(group))
        .collect();
    let mut named = Map::new();
    for name in regex.capture_names().flatten() {
        named.insert(name.to_string(), group_value(captures.name(name)));
    }
    json!({
        "text": whole.as_str(),
        "start": char_offset(whole.start()),
        "end": char_offset(whole.end()),
        "groups": groups,
        "named": Value::Object(named),
    })
}

fn field_value(item: &Value, field: &str) -> Value {
    if field == "item" || field == "value" && !item.is_object() {
        return item.clone();
    }
    item.as_object()
        .and_then(|object| object.get(field))
        .cloned()
        .unwrap_or(Value::Null)
}

fn predicate_matches(item: &Value, field: &str, operator: &str, expected: &Value) -> bool {
    let actual = field_value(item, field);
    match operator {
        "eq" => actual == *expected,
        "neq" => actual != *expected,
        "gt" => number_as_f64(&actual) > number_as_f64(expected),
        "gte" => number_as_f64(&actual) >= number_as_f64(expected),
        "lt" => number_as_f64(&actual) < number_as_f64(expected),
        "lte" => number_as_f64(&actual) <= number_as_f64(expected),
        "contains" => contains_value(&actual, expected),
        _ => false,
    }
}

fn find_where(args: &[Value]) -> Value {
    let items = as_array(&args[0]);
    let field = value_as_str(&args[1]);
    let operator = value_as_str(&args[2]);
    let expected = args.get(3).unwrap_or(&Value::Null);
    items
        .into_iter()
        .find(|item| predicate_matches(item, &field, &operator, expected))
        .unwrap_or(Value::Null)
}

fn filter_where(args: &[Value], reject: bool) -> Value {
    let items = as_array(&args[0]);
    let field = value_as_str(&args[1]);
    let operator = value_as_str(&args[2]);
    let expected = args.get(3).unwrap_or(&Value::Null);
    Value::Array(
        items
            .into_iter()
            .filter(|item| predicate_matches(item, &field, &operator, expected) != reject)
            .collect(),
    )
}

fn partition_where(args: &[Value]) -> Value {
    let matched = filter_where(args, false);
    let rejected = filter_where(args, true);
    json!({
        "matched": matched,
        "rejected": rejected,
    })
}

fn group_by(args: &[Value]) -> Value {
    let field = value_as_str(&args[1]);
    let mut groups = Map::new();
    for item in as_array(&args[0]) {
        let key = value_as_str(&field_value(&item, &field));
        let entry = groups
            .entry(key)
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Value::Array(items) = entry {
            items.push(item);
        }
    }
    Value::Object(groups)
}

fn index_by(args: &[Value]) -> Value {
    let field = value_as_str(&args[1]);
    let mut index = Map::new();
    for item in as_array(&args[0]) {
        let key = value_as_str(&field_value(&item, &field));
        index.insert(key, item);
    }
    Value::Object(index)
}

fn unique_values(values: Vec<Value>) -> Vec<Value> {
    let mut unique = Vec::new();
    for value in values {
        if !unique.contains(&value) {
            unique.push(value);
        }
    }
    unique
}

fn map_transform(args: &[Value]) -> Value {
    let operation = value_as_str(&args[1]);
    Value::Array(
        as_array(&args[0])
            .iter()
            .map(|item| match operation.as_str() {
                "trim" => Value::String(value_as_str(item).trim().to_string()),
                "uppercase" => Value::String(value_as_str(item).to_uppercase()),
                "lowercase" => Value::String(value_as_str(item).to_lowercase()),
                "normalize_whitespace" => Value::String(normalize_whitespace(&value_as_str(item))),
                _ => item.clone(),
            })
            .collect(),
    )
}

fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn slugify(text: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for ch in text.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            previous_dash = false;
        } else if !previous_dash {
            slug.push('-');
            previous_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

fn title_case(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    let upper: String = first.to_uppercase().collect();
                    format!("{upper}{}", chars.as_str().to_lowercase())
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn sentence_case(text: &str) -> String {
    let normalized = normalize_whitespace(text).to_lowercase();
    let mut chars = normalized.chars();
    match chars.next() {
        Some(first) => {
            let upper: String = first.to_uppercase().collect();
            format!("{upper}{}", chars.as_str())
        }
        None => String::new(),
    }
}

fn parse_iso_date(text: &str) -> Option<(i32, u32, u32)> {
    let parts: Vec<&str> = text.trim().split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let year = parts[0].parse::<i32>().ok()?;
    let month = parts[1].parse::<u32>().ok()?;
    let day = parts[2].parse::<u32>().ok()?;
    if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
        return None;
    }
    Some((year, month, day))
}

fn format_iso_date((year, month, day): (i32, u32, u32)) -> String {
    format!("{year:04}-{month:02}-{day:02}")
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - i32::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = (year - era * 400) as i64;
    let month = month as i64;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era as i64 * 146097 + doe - 719468
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let days = days + 719468;
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    (
        (year + i64::from(month <= 2)) as i32,
        month as u32,
        day as u32,
    )
}

fn business_days_between(start: (i32, u32, u32), end: (i32, u32, u32)) -> i64 {
    let start_days = days_from_civil(start.0, start.1, start.2);
    let end_days = days_from_civil(end.0, end.1, end.2);
    let step = if end_days >= start_days { 1 } else { -1 };
    let mut count = 0;
    let mut day = start_days;
    while day != end_days {
        day += step;
        let weekday = (day + 4).rem_euclid(7);
        if weekday < 5 {
            count += step;
        }
    }
    count
}

fn has_fields(value: &Value, fields_value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    let fields = as_array(fields_value);
    if fields.is_empty() {
        return false;
    }
    fields.iter().all(|field| {
        let field_name = value_as_str(field);
        object
            .get(&field_name)
            .is_some_and(|value| !value.is_null())
    })
}

fn matches_shape(value: &Value, shape: &Value) -> bool {
    if let Some(shape_object) = shape.as_object() {
        let Some(value_object) = value.as_object() else {
            return false;
        };
        return shape_object.iter().all(|(field, expected_shape)| {
            value_object.get(field).is_some_and(|field_value| {
                !field_value.is_null() && matches_shape(field_value, expected_shape)
            })
        });
    }

    if let Some(items) = shape.as_array() {
        let Some(values) = value.as_array() else {
            return false;
        };
        return items
            .first()
            .map(|item_shape| values.iter().all(|item| matches_shape(item, item_shape)))
            .unwrap_or(true);
    }

    let expected = shape
        .as_str()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match expected.as_str() {
        "any" | "value" => !value.is_null(),
        "text" | "string" => value.is_string(),
        "number" | "numeric" => value.is_number(),
        "boolean" | "bool" => value.is_boolean(),
        "list" | "array" => value.is_array(),
        "record" | "object" => value.is_object(),
        "missing" | "nil" | "null" => value.is_null(),
        _ => false,
    }
}

/// SHA-256 of `input` as lowercase hex. Lives in the VM crate so audit
/// records hash inputs/outputs identically on every runtime; the compiler
/// and CLI reuse it (re-exported from devlish_core) for `source_hash` and
/// evidence bundles.
pub fn sha256_hex(input: &[u8]) -> String {
    sha256(input)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join("")
}

fn sha256(input: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (input.len() as u64) * 8;
    let mut message = input.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in message.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (index, word) in w.iter_mut().take(16).enumerate() {
            let offset = index * 4;
            *word = u32::from_be_bytes([
                chunk[offset],
                chunk[offset + 1],
                chunk[offset + 2],
                chunk[offset + 3],
            ]);
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7)
                ^ w[index - 15].rotate_right(18)
                ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17)
                ^ w[index - 2].rotate_right(19)
                ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];
        let mut f = h[5];
        let mut g = h[6];
        let mut hh = h[7];

        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (index, value) in h.iter().enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&value.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestHost {
        events: Vec<Value>,
        audit: Vec<Value>,
    }

    impl TestHost {
        fn new() -> Self {
            Self {
                events: Vec::new(),
                audit: Vec::new(),
            }
        }
    }

    impl HostEffects for TestHost {
        fn emit_event(&mut self, event: &Value) {
            self.events.push(event.clone());
        }
        fn write_file(&mut self, _request: &Value) -> Result<(), String> {
            Ok(())
        }
        fn audit_record(&mut self, record: &Value) -> Result<(), String> {
            self.audit.push(record.clone());
            Ok(())
        }
    }

    #[test]
    fn nop_with_note_returns_error() {
        let package = json!({
            "format": "devlish-bytecode",
            "format_version": 0,
            "constant_pool": [],
            "instructions": [
                {"op": "NOP", "note": "not yet supported in bytecode target"}
            ],
            "source_map": [
                {"address": 0, "line": 1, "source_text": "Call DealStar fetch_report with deal_id equals \"DS-100\""}
            ]
        });
        let mut host = TestHost::new();
        let mut vm = Vm::new(package, json!({})).unwrap();
        let result = vm.run(&mut host);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("not yet supported"),
            "expected 'not yet supported' in error, got: {}",
            err.message
        );
        assert!(
            err.message.contains("Call DealStar"),
            "expected source text in error, got: {}",
            err.message
        );
    }

    #[test]
    fn nop_without_source_map_still_errors() {
        let package = json!({
            "format": "devlish-bytecode",
            "format_version": 0,
            "constant_pool": [],
            "instructions": [
                {"op": "NOP", "note": "not yet supported in bytecode target"}
            ]
        });
        let mut host = TestHost::new();
        let mut vm = Vm::new(package, json!({})).unwrap();
        let result = vm.run(&mut host);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("not yet supported"));
    }

    #[test]
    fn jump_target_out_of_range_is_rejected_before_execution() {
        let package = json!({
            "format": "devlish-bytecode",
            "format_version": 0,
            "constant_pool": [],
            "instructions": [
                {"op": "JUMP", "target": 99}
            ]
        });
        let Err(err) = Vm::new(package, json!({})) else {
            panic!("out-of-range jump should be rejected");
        };
        assert!(
            err.message.contains("target 99 is out of range"),
            "got: {}",
            err.message
        );
    }

    #[test]
    fn try_handler_out_of_range_is_rejected_before_execution() {
        let package = json!({
            "format": "devlish-bytecode",
            "format_version": 0,
            "constant_pool": [],
            "instructions": [
                {"op": "TRY_BEGIN", "handler": 7},
                {"op": "TRY_END"}
            ]
        });
        let Err(err) = Vm::new(package, json!({})) else {
            panic!("out-of-range handler should be rejected");
        };
        assert!(
            err.message.contains("handler 7 is out of range"),
            "got: {}",
            err.message
        );
    }

    #[test]
    fn jump_to_end_of_program_is_valid() {
        let package = json!({
            "format": "devlish-bytecode",
            "format_version": 0,
            "constant_pool": [1],
            "instructions": [
                {"op": "CONST", "dest": "r0", "const": 0},
                {"op": "JUMP", "target": 2}
            ]
        });
        let mut host = TestHost::new();
        let mut vm = match Vm::new(package, json!({})) {
            Ok(vm) => vm,
            Err(err) => panic!("jump to end should be valid: {}", err.message),
        };
        vm.run(&mut host).expect("runs to completion");
    }

    #[test]
    fn jump_with_missing_target_is_rejected_before_execution() {
        let package = json!({
            "format": "devlish-bytecode",
            "format_version": 0,
            "constant_pool": [],
            "instructions": [
                {"op": "JUMP_IF_FALSE", "condition": "r0"}
            ]
        });
        let Err(err) = Vm::new(package, json!({})) else {
            panic!("missing target should be rejected");
        };
        assert!(
            err.message.contains("missing numeric 'target'"),
            "got: {}",
            err.message
        );
    }

    #[test]
    fn jump_if_false_target_out_of_range_is_rejected_before_execution() {
        let package = json!({
            "format": "devlish-bytecode",
            "format_version": 0,
            "constant_pool": [],
            "instructions": [
                {"op": "JUMP_IF_FALSE", "condition": "r0", "target": 42}
            ]
        });
        let Err(err) = Vm::new(package, json!({})) else {
            panic!("out-of-range JUMP_IF_FALSE target should be rejected");
        };
        assert!(
            err.message.contains("target 42 is out of range"),
            "got: {}",
            err.message
        );
    }

    #[test]
    fn instruction_without_op_field_is_rejected_before_execution() {
        let package = json!({
            "format": "devlish-bytecode",
            "format_version": 0,
            "constant_pool": [],
            "instructions": [
                {"target": 0}
            ]
        });
        let Err(err) = Vm::new(package, json!({})) else {
            panic!("instruction without op should be rejected");
        };
        assert!(
            err.message.contains("has no string 'op' field"),
            "got: {}",
            err.message
        );
    }

    #[test]
    fn instruction_limit_stops_infinite_loop() {
        let package = json!({
            "format": "devlish-bytecode",
            "format_version": 0,
            "constant_pool": [42],
            "instructions": [
                {"op": "CONST", "dest": "x", "const": 0},
                {"op": "JUMP", "target": 0}
            ]
        });
        let mut host = TestHost::new();
        let mut vm = Vm::new(package, json!({})).unwrap();
        vm.set_emit_events(false);
        vm.set_instruction_limit(100);
        let result = vm.run(&mut host);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message.contains("Instruction limit exceeded"),
            "expected limit error, got: {}",
            err.message
        );
    }

    #[test]
    fn emit_events_disabled_suppresses_host_events() {
        let package = json!({
            "format": "devlish-bytecode",
            "format_version": 0,
            "constant_pool": [],
            "instructions": [
                {"op": "NOP", "note": "not yet supported in bytecode target"}
            ],
            "source_map": [
                {"address": 0, "line": 1, "source_text": "some statement"}
            ]
        });
        let mut host = TestHost::new();
        let mut vm = Vm::new(package, json!({})).unwrap();
        vm.set_emit_events(false);
        let result = vm.run(&mut host);
        // Still errors (NOP is unsupported), but no events emitted to host
        assert!(result.is_err());
        assert!(
            host.events.is_empty(),
            "expected no events emitted to host, got: {}",
            host.events.len()
        );
        // Error events vec should also be empty
        let err = result.unwrap_err();
        assert!(
            err.events.is_empty(),
            "expected no internal events, got: {}",
            err.events.len()
        );
    }

    fn governed_package() -> Value {
        json!({
            "format": "devlish-bytecode",
            "format_version": 0,
            "constant_pool": [42],
            "instructions": [
                {"op": "CONST", "dest": "r0", "const": 0},
                {"op": "STORE", "symbol": "answer", "value": "r0"}
            ],
            "manifest": {
                "rule": {"id": "pricing.tier", "version": "1.0.0"}
            }
        })
    }

    #[test]
    fn governed_run_emits_one_audit_record_with_canonical_hashes() {
        let package = governed_package();
        let input = json!({"amount": 100});
        let expected_artifact =
            sha256_hex(serde_json::to_string_pretty(&package).unwrap().as_bytes());
        let expected_input = sha256_hex(&serde_json::to_vec(&input).unwrap());

        let mut host = TestHost::new();
        let mut vm = Vm::new(package, input).unwrap();
        vm.set_emit_events(false);
        let result = vm.run(&mut host).expect("run succeeds");

        assert_eq!(host.audit.len(), 1);
        let record = &host.audit[0];
        assert_eq!(record["rule_id"], json!("pricing.tier"));
        assert_eq!(record["rule_version"], json!("1.0.0"));
        assert_eq!(record["success"], json!(true));
        assert_eq!(record["artifact_sha256"], json!(expected_artifact));
        assert_eq!(record["input_sha256"], json!(expected_input));
        assert_eq!(
            record["output_sha256"],
            json!(sha256_hex(&serde_json::to_vec(&result).unwrap()))
        );
        assert_eq!(record["instruction_count"], json!(2));
    }

    #[test]
    fn ungoverned_run_emits_no_audit_record() {
        let package = json!({
            "format": "devlish-bytecode",
            "format_version": 0,
            "constant_pool": [42],
            "instructions": [
                {"op": "CONST", "dest": "r0", "const": 0},
                {"op": "STORE", "symbol": "answer", "value": "r0"}
            ]
        });
        let mut host = TestHost::new();
        let mut vm = Vm::new(package, json!({})).unwrap();
        vm.run(&mut host).expect("run succeeds");
        assert!(host.audit.is_empty());
    }

    #[test]
    fn failed_governed_run_emits_failure_record() {
        let package = json!({
            "format": "devlish-bytecode",
            "format_version": 0,
            "constant_pool": [],
            "instructions": [
                {"op": "NOP", "note": "not yet supported in bytecode target"}
            ],
            "manifest": {
                "rule": {"id": "pricing.tier", "version": "1.0.0"}
            }
        });
        let mut host = TestHost::new();
        let mut vm = Vm::new(package, json!({})).unwrap();
        let error = vm.run(&mut host).expect_err("run fails");

        assert_eq!(host.audit.len(), 1);
        let record = &host.audit[0];
        assert_eq!(record["success"], json!(false));
        let failure = json!({"success": false, "error": error.message});
        assert_eq!(
            record["output_sha256"],
            json!(sha256_hex(&serde_json::to_vec(&failure).unwrap()))
        );
    }

    #[test]
    fn audit_write_failure_fails_the_run() {
        struct FailingAuditHost;
        impl HostEffects for FailingAuditHost {
            fn emit_event(&mut self, _event: &Value) {}
            fn write_file(&mut self, _request: &Value) -> Result<(), String> {
                Ok(())
            }
            fn audit_record(&mut self, _record: &Value) -> Result<(), String> {
                Err("disk full".to_string())
            }
        }
        let mut host = FailingAuditHost;
        let mut vm = Vm::new(governed_package(), json!({})).unwrap();
        let error = vm.run(&mut host).expect_err("audit failure fails the run");
        assert!(
            error
                .message
                .contains("audit record write failed: disk full"),
            "got: {}",
            error.message
        );
    }

    // -- Determinism (DEVL-122): governed execution must be a pure function
    // of (bytecode, input, effect responses). ----------------------------

    #[test]
    fn identical_runs_produce_byte_identical_output() {
        let package = json!({
            "format": "devlish-bytecode",
            "format_version": 0,
            "constant_pool": [0.1, 0.2, 3, {"b": 1, "a": 2}],
            "instructions": [
                {"op": "CONST", "dest": "r0", "const": 0},
                {"op": "CONST", "dest": "r1", "const": 1},
                {"op": "ADD", "dest": "r2", "left": "r0", "right": "r1"},
                {"op": "STORE", "symbol": "sum", "value": "r2"},
                {"op": "CONST", "dest": "r3", "const": 3},
                {"op": "STORE", "symbol": "shape", "value": "r3"}
            ],
            "manifest": {"rule": {"id": "det.check", "version": "1.0.0"}}
        });
        let input = json!({"zeta": 1, "alpha": 2});

        let mut outputs = Vec::new();
        for _ in 0..2 {
            let mut host = TestHost::new();
            let mut vm = Vm::new(package.clone(), input.clone()).unwrap();
            vm.set_emit_events(false);
            let result = vm.run(&mut host).expect("run succeeds");
            outputs.push(serde_json::to_vec(&result).unwrap());
        }
        assert_eq!(
            outputs[0], outputs[1],
            "identical runs must serialize identically"
        );
    }

    #[test]
    fn canonical_serialization_sorts_map_keys() {
        // serde_json maps are BTreeMaps here (no preserve_order feature), so
        // insertion order never leaks into serialized bytes.
        let mut forward = Map::new();
        forward.insert("alpha".to_string(), json!(1));
        forward.insert("zeta".to_string(), json!(2));
        let mut backward = Map::new();
        backward.insert("zeta".to_string(), json!(2));
        backward.insert("alpha".to_string(), json!(1));
        assert_eq!(
            serde_json::to_vec(&Value::Object(forward)).unwrap(),
            serde_json::to_vec(&Value::Object(backward)).unwrap()
        );
    }

    #[test]
    fn float_semantics_are_defined_and_stable() {
        // Whole-valued results normalize to integers; fractional results keep
        // IEEE-754 f64 shortest-roundtrip formatting. Both are part of the
        // audit contract: output hashes depend on these exact bytes.
        assert_eq!(number_value(720.0).unwrap(), json!(720));
        assert_eq!(number_value(-0.0).unwrap(), json!(0));
        let sum = number_value(0.1 + 0.2).unwrap();
        assert_eq!(serde_json::to_string(&sum).unwrap(), "0.30000000000000004");

        // Whole-valued floats beyond i64 range saturate (Rust float-cast
        // semantics) -- deterministic on run and replay, but documented.
        assert_eq!(number_value(1e300).unwrap(), json!(i64::MAX));

        // NaN and infinity can never enter a result envelope: they are
        // hard errors, not host-dependent formatting.
        assert!(number_value(f64::NAN).is_err());
        assert!(number_value(f64::INFINITY).is_err());
    }

    #[test]
    fn sha256_matches_known_test_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
