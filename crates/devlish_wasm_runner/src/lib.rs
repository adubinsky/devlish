use devlish_vm::{HostEffects, Vm, VmError};
use serde_json::{json, Value};
use std::sync::{Mutex, PoisonError};

#[link(wasm_import_module = "devlish_host")]
extern "C" {
    fn emit_event(ptr: *const u8, len: usize);
    fn write_file(ptr: *const u8, len: usize) -> i32;
}

static LAST_RESULT: Mutex<Vec<u8>> = Mutex::new(Vec::new());
static INSTRUCTION_LIMIT: Mutex<u64> = Mutex::new(10_000_000);

#[no_mangle]
pub extern "C" fn devlish_alloc(len: usize) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(len);
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    ptr
}

#[no_mangle]
pub unsafe extern "C" fn devlish_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        drop(Vec::from_raw_parts(ptr, 0, len));
    }
}

#[no_mangle]
pub unsafe extern "C" fn devlish_run(
    bytecode_ptr: *const u8,
    bytecode_len: usize,
    input_ptr: *const u8,
    input_len: usize,
) -> i32 {
    let result = run_from_raw_parts(bytecode_ptr, bytecode_len, input_ptr, input_len);
    let status = if result.get("success").and_then(Value::as_bool) == Some(true) {
        0
    } else {
        1
    };
    *LAST_RESULT.lock().unwrap_or_else(PoisonError::into_inner) =
        serde_json::to_vec(&result).unwrap_or_else(|_| br#"{"success":false}"#.to_vec());
    status
}

#[no_mangle]
pub extern "C" fn devlish_result_ptr() -> *const u8 {
    LAST_RESULT
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ptr()
}

#[no_mangle]
pub extern "C" fn devlish_result_len() -> usize {
    LAST_RESULT
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .len()
}

#[no_mangle]
pub extern "C" fn devlish_set_instruction_limit(limit: u64) {
    *INSTRUCTION_LIMIT
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = limit;
}

struct WasmHost {
    /// Audit records emitted by governed runs. WASM has no clock or stable
    /// side channel, so records ride back to the embedder attached to the
    /// run result under `AUDIT_TRANSPORT_KEY`; the JS runtime stamps the
    /// timestamp and hands them to `onAuditRecord`.
    audit: Vec<Value>,
}

/// Tags an audit record with this runtime's identity. WASM has no clock;
/// the JS runtime stamps the timestamp when it delivers the record.
fn tag_wasm_runtime(record: &Value) -> Result<Value, String> {
    let mut full = record
        .as_object()
        .cloned()
        .ok_or_else(|| "audit record must be an object".to_string())?;
    full.insert(
        "runtime".to_string(),
        json!({ "kind": "wasm", "version": env!("CARGO_PKG_VERSION") }),
    );
    Ok(Value::Object(full))
}

/// Reserved envelope key that carries audit records out of the sandbox.
/// Namespaced so no program-controlled key (e.g. a CHECKPOINT context_key)
/// can collide with -- or forge -- the transport.
const AUDIT_TRANSPORT_KEY: &str = "__devlish_audit__";

/// Attaches collected audit records to the result envelope. This happens
/// after the VM hashed its result, and the JS runtime strips the key back
/// off before the embedder sees it, so output_sha256 stays accurate.
fn attach_audit(result: &mut Value, audit: Vec<Value>) {
    if let Some(envelope) = result.as_object_mut() {
        // Anything already under the reserved key came from the program,
        // not this runner: drop it so forged records never reach the
        // embedder's onAuditRecord.
        envelope.remove(AUDIT_TRANSPORT_KEY);
        if !audit.is_empty() {
            envelope.insert(AUDIT_TRANSPORT_KEY.to_string(), Value::Array(audit));
        }
    }
}

impl HostEffects for WasmHost {
    fn audit_record(&mut self, record: &Value) -> Result<(), String> {
        self.audit.push(tag_wasm_runtime(record)?);
        Ok(())
    }

    fn respond(&mut self, _value: &Value) -> Result<(), String> {
        // The VM stores the response in its checkpoint; the host just needs to accept it.
        // NativeHost prints to stdout; WASM has no stdout, so this is a no-op.
        Ok(())
    }

    fn emit_event(&mut self, event: &Value) {
        if let Ok(serialized) = serde_json::to_vec(event) {
            unsafe { emit_event(serialized.as_ptr(), serialized.len()) };
        }
    }

    fn write_file(&mut self, request: &Value) -> Result<(), String> {
        let serialized = serde_json::to_vec(request).map_err(|error| error.to_string())?;
        let status = unsafe { write_file(serialized.as_ptr(), serialized.len()) };
        if status != 0 {
            Err("host write_file returned non-zero".to_string())
        } else {
            Ok(())
        }
    }
}

unsafe fn run_from_raw_parts(
    bytecode_ptr: *const u8,
    bytecode_len: usize,
    input_ptr: *const u8,
    input_len: usize,
) -> Value {
    let bytecode_bytes = std::slice::from_raw_parts(bytecode_ptr, bytecode_len);
    let input_bytes = std::slice::from_raw_parts(input_ptr, input_len);

    let package: Value = match serde_json::from_slice(bytecode_bytes) {
        Ok(value) => value,
        Err(error) => return failure(format!("Invalid bytecode JSON: {error}"), Vec::new()),
    };
    let input: Value = match serde_json::from_slice(input_bytes) {
        Ok(value) => value,
        Err(error) => return failure(format!("Invalid input JSON: {error}"), Vec::new()),
    };

    // Fail-fast: reject tools that declare permissions WASM cannot satisfy.
    if let Some(unsupported) = check_wasm_permissions(&package) {
        return failure(
            format!("Tool requires permissions unavailable in WASM: {unsupported}"),
            Vec::new(),
        );
    }

    let limit = *INSTRUCTION_LIMIT
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    let mut host = WasmHost { audit: Vec::new() };
    let mut result = match Vm::new(package, input).and_then(|mut vm| {
        vm.set_emit_events(false);
        vm.set_instruction_limit(limit);
        vm.run(&mut host)
    }) {
        Ok(result) => result,
        Err(VmError { message, events }) => failure(message, events),
    };
    attach_audit(&mut result, host.audit);
    result
}

/// Returns the first unsupported permission kind if the manifest declares
/// capabilities that cannot be satisfied in a WASM sandbox.
fn check_wasm_permissions(package: &Value) -> Option<String> {
    const UNSUPPORTED: &[&str] = &[
        "http_request",
        "filesystem",
        "file_read",
        "file_write",
        "file_copy",
        "file_move",
        "file_mkdir",
        "file_delete",
        "http_download",
    ];

    let permissions = package
        .get("manifest")
        .and_then(|m| m.get("permissions"))
        .and_then(Value::as_array)?;

    for perm in permissions {
        if let Some(kind) = perm.get("kind").and_then(Value::as_str) {
            if UNSUPPORTED.contains(&kind) {
                return Some(kind.to_string());
            }
        }
    }
    None
}

fn failure(message: String, events: Vec<Value>) -> Value {
    json!({
        "success": false,
        "error": message,
        "results": {
            "events": events
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_http_permission() {
        let package = json!({
            "manifest": {
                "permissions": [{"kind": "http_request", "scope": "https://api.example.com"}]
            }
        });
        assert_eq!(
            check_wasm_permissions(&package),
            Some("http_request".to_string())
        );
    }

    #[test]
    fn rejects_filesystem_permission() {
        let package = json!({
            "manifest": {
                "permissions": [{"kind": "filesystem"}]
            }
        });
        assert_eq!(
            check_wasm_permissions(&package),
            Some("filesystem".to_string())
        );
    }

    #[test]
    fn allows_no_permissions() {
        let package = json!({
            "manifest": {
                "permissions": []
            }
        });
        assert_eq!(check_wasm_permissions(&package), None);
    }

    #[test]
    fn allows_no_manifest() {
        let package = json!({});
        assert_eq!(check_wasm_permissions(&package), None);
    }

    #[test]
    fn tag_wasm_runtime_adds_runtime_identity() {
        let record = json!({"rule_id": "pricing.tier", "success": true});
        let tagged = tag_wasm_runtime(&record).expect("record is an object");
        assert_eq!(tagged["rule_id"], json!("pricing.tier"));
        assert_eq!(tagged["runtime"]["kind"], json!("wasm"));
        assert_eq!(
            tagged["runtime"]["version"],
            json!(env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn tag_wasm_runtime_rejects_non_object() {
        assert!(tag_wasm_runtime(&json!("nope")).is_err());
    }

    #[test]
    fn attach_audit_adds_records_to_result_envelope() {
        let mut result = json!({"success": true, "results": {}});
        attach_audit(&mut result, vec![json!({"rule_id": "pricing.tier"})]);
        assert_eq!(
            result[AUDIT_TRANSPORT_KEY][0]["rule_id"],
            json!("pricing.tier")
        );
    }

    #[test]
    fn attach_audit_leaves_result_untouched_when_empty() {
        let mut result = json!({"success": true});
        attach_audit(&mut result, Vec::new());
        assert_eq!(result, json!({"success": true}));
    }

    #[test]
    fn attach_audit_drops_forged_transport_key() {
        // A program that smuggled records under the reserved key (e.g. via a
        // CHECKPOINT context_key) must not reach the embedder's callback.
        let mut result = json!({"success": true, "__devlish_audit__": [{"rule_id": "forged"}]});
        attach_audit(&mut result, Vec::new());
        assert_eq!(result, json!({"success": true}));

        let mut result = json!({"success": true, "__devlish_audit__": [{"rule_id": "forged"}]});
        attach_audit(&mut result, vec![json!({"rule_id": "real"})]);
        assert_eq!(result[AUDIT_TRANSPORT_KEY], json!([{"rule_id": "real"}]));
    }
}
