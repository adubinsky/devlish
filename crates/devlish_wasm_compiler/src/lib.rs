use devlish_core::{compile_source_to_json, CompileError, CompileOptions};
use serde_json::json;
use std::sync::Mutex;

static LAST_RESULT: Mutex<Vec<u8>> = Mutex::new(Vec::new());

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
pub unsafe extern "C" fn devlish_compile(source_ptr: *const u8, source_len: usize) -> i32 {
    let source_bytes = std::slice::from_raw_parts(source_ptr, source_len);
    let source = match std::str::from_utf8(source_bytes) {
        Ok(s) => s,
        Err(error) => {
            store_result(&format_error(&format!("Invalid UTF-8 source: {error}")));
            return 1;
        }
    };

    let options = CompileOptions {
        source_path: None,
        search_paths: vec![],
    };

    match compile_source_to_json(source, options) {
        Ok(json_bytecode) => {
            *LAST_RESULT.lock().expect("result lock poisoned") = json_bytecode.into_bytes();
            0
        }
        Err(CompileError { diagnostics }) => {
            let diag_values: Vec<serde_json::Value> = diagnostics
                .iter()
                .map(|d| {
                    json!({
                        "line": d.line,
                        "message": d.message,
                        "source_text": d.source_text,
                    })
                })
                .collect();
            let error_json = json!({
                "success": false,
                "diagnostics": diag_values,
            });
            store_result(
                &serde_json::to_string(&error_json)
                    .unwrap_or_else(|_| r#"{"success":false}"#.to_string()),
            );
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn devlish_compile_result_ptr() -> *const u8 {
    LAST_RESULT.lock().expect("result lock poisoned").as_ptr()
}

#[no_mangle]
pub extern "C" fn devlish_compile_result_len() -> usize {
    LAST_RESULT.lock().expect("result lock poisoned").len()
}

fn store_result(value: &str) {
    *LAST_RESULT.lock().expect("result lock poisoned") = value.as_bytes().to_vec();
}

fn format_error(message: &str) -> String {
    let error_json = json!({
        "success": false,
        "diagnostics": [{
            "line": 0,
            "message": message,
            "source_text": "",
        }],
    });
    serde_json::to_string(&error_json).unwrap_or_else(|_| r#"{"success":false}"#.to_string())
}
