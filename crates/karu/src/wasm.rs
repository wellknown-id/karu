//! WebAssembly FFI bindings for Karu.
//!
//! This module provides C-compatible FFI functions for embedding Karu
//! in other runtimes via WebAssembly.
//!
//! # Memory Model
//!
//! All strings are passed via pointers and lengths. The host is responsible
//! for allocating and freeing memory for input strings. Karu allocates
//! memory for output strings which the host should free using `karu_free`.

use crate::compiler;
use crate::parser::Parser;
use crate::rule::Effect;
#[cfg(feature = "cedar")]
use crate::transpile;
use std::slice;

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

// ============================================================================
// wasm-bindgen exports for browser playground
// ============================================================================

/// Helper to create a plain JS object with a single string property
#[cfg(feature = "wasm")]
fn js_object_with_str(key: &str, value: &str) -> JsValue {
    let obj = js_sys::Object::new();
    js_sys::Reflect::set(&obj, &JsValue::from_str(key), &JsValue::from_str(value)).unwrap();
    obj.into()
}

/// Helper to create a plain JS object with ok and rules properties
#[cfg(feature = "wasm")]
fn js_check_result(rules: usize) -> JsValue {
    let obj = js_sys::Object::new();
    js_sys::Reflect::set(&obj, &JsValue::from_str("ok"), &JsValue::from_bool(true)).unwrap();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("rules"),
        &JsValue::from_f64(rules as f64),
    )
    .unwrap();
    obj.into()
}

/// Evaluate a policy against JSON input (browser-friendly API).
/// Returns a plain object: { result: "ALLOW" | "DENY" } or { error: string }
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn karu_eval_js(policy: &str, input: &str) -> JsValue {
    match compiler::compile(policy) {
        Ok(compiled) => match serde_json::from_str::<serde_json::Value>(input) {
            Ok(json) => {
                let result = match compiled.evaluate(&json) {
                    Effect::Allow => "ALLOW",
                    Effect::Deny => "DENY",
                };
                js_object_with_str("result", result)
            }
            Err(e) => js_object_with_str("error", &format!("Invalid JSON: {}", e)),
        },
        Err(e) => js_object_with_str("error", &format!("Parse error: {:?}", e)),
    }
}

/// Transpile a Karu policy to Cedar syntax (browser-friendly API).
/// Returns { cedar: string } or { error: string }
#[cfg(all(feature = "wasm", feature = "cedar"))]
#[wasm_bindgen]
pub fn karu_transpile_js(policy: &str) -> JsValue {
    match Parser::parse(policy) {
        Ok(ast) => match transpile::to_cedar(&ast) {
            Ok(cedar) => js_object_with_str("cedar", &cedar),
            Err(e) => js_object_with_str("error", &format!("{}", e)),
        },
        Err(e) => js_object_with_str("error", &format!("Parse error: {:?}", e)),
    }
}

/// Check a Karu policy for syntax errors (browser-friendly API).
/// Returns { ok: true, rules: number } or { error: string }
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn karu_check_js(policy: &str) -> JsValue {
    match Parser::parse(policy) {
        Ok(ast) => js_check_result(ast.rules.len()),
        Err(e) => js_object_with_str("error", &format!("{:?}", e)),
    }
}

/// Simulate policy evaluation with detailed trace (browser-friendly API).
/// Returns { decision, matched_rules, trace } or { error: string }
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn karu_simulate_js(policy: &str, input: &str) -> JsValue {
    match compiler::compile(policy) {
        Ok(compiled) => match serde_json::from_str::<serde_json::Value>(input) {
            Ok(json) => {
                let simulator = crate::simulate::Simulator::new(compiled);
                let result = simulator.simulate(&json);

                let obj = js_sys::Object::new();
                let decision = match result.decision {
                    Effect::Allow => "ALLOW",
                    Effect::Deny => "DENY",
                };
                js_sys::Reflect::set(
                    &obj,
                    &JsValue::from_str("decision"),
                    &JsValue::from_str(decision),
                )
                .unwrap();

                let rules = js_sys::Array::new();
                for name in &result.matched_rules {
                    rules.push(&JsValue::from_str(name));
                }
                js_sys::Reflect::set(&obj, &JsValue::from_str("matched_rules"), &rules).unwrap();

                obj.into()
            }
            Err(e) => js_object_with_str("error", &format!("Invalid JSON: {}", e)),
        },
        Err(e) => js_object_with_str("error", &format!("Parse error: {:?}", e)),
    }
}

/// Evaluate a Cedar policy against JSON input (browser-friendly API).
/// Imports the Cedar policy to Karu, compiles, and simulates.
/// Returns { decision, matched_rules } or { error: string }
#[cfg(all(feature = "wasm", feature = "cedar"))]
#[wasm_bindgen]
pub fn karu_eval_cedar_js(cedar_policy: &str, input: &str) -> JsValue {
    match crate::compile_cedar(cedar_policy) {
        Ok(compiled) => match serde_json::from_str::<serde_json::Value>(input) {
            Ok(json) => {
                let simulator = crate::simulate::Simulator::new(compiled);
                let result = simulator.simulate(&json);

                let obj = js_sys::Object::new();
                let decision = match result.decision {
                    Effect::Allow => "ALLOW",
                    Effect::Deny => "DENY",
                };
                js_sys::Reflect::set(
                    &obj,
                    &JsValue::from_str("decision"),
                    &JsValue::from_str(decision),
                )
                .unwrap();

                let rules = js_sys::Array::new();
                for name in &result.matched_rules {
                    rules.push(&JsValue::from_str(name));
                }
                js_sys::Reflect::set(&obj, &JsValue::from_str("matched_rules"), &rules).unwrap();

                obj.into()
            }
            Err(e) => js_object_with_str("error", &format!("Invalid JSON: {}", e)),
        },
        Err(e) => js_object_with_str("error", &format!("Cedar import error: {}", e)),
    }
}

/// Compare two policies and return diff (browser-friendly API).
/// Returns { added, removed, modified, summary } or { error: string }
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn karu_diff_js(old_policy: &str, new_policy: &str) -> JsValue {
    let old = match compiler::compile(old_policy) {
        Ok(p) => p,
        Err(e) => return js_object_with_str("error", &format!("Old policy error: {:?}", e)),
    };
    let new = match compiler::compile(new_policy) {
        Ok(p) => p,
        Err(e) => return js_object_with_str("error", &format!("New policy error: {:?}", e)),
    };

    let diff = crate::diff::PolicyDiff::compare(&old, &new);
    let obj = js_sys::Object::new();

    let added = js_sys::Array::new();
    for rule in &diff.added {
        added.push(&JsValue::from_str(&rule.name));
    }
    js_sys::Reflect::set(&obj, &JsValue::from_str("added"), &added).unwrap();

    let removed = js_sys::Array::new();
    for rule in &diff.removed {
        removed.push(&JsValue::from_str(&rule.name));
    }
    js_sys::Reflect::set(&obj, &JsValue::from_str("removed"), &removed).unwrap();

    let modified = js_sys::Array::new();
    for rule in &diff.modified {
        modified.push(&JsValue::from_str(&rule.name));
    }
    js_sys::Reflect::set(&obj, &JsValue::from_str("modified"), &modified).unwrap();

    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("summary"),
        &JsValue::from_str(&diff.summary()),
    )
    .unwrap();

    obj.into()
}

/// Batch evaluate multiple inputs against a policy (browser-friendly API).
/// Takes policy string and JSON array of inputs.
/// Returns { results: ["ALLOW" | "DENY", ...] } or { error: string }
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn karu_batch_js(policy: &str, inputs: &str) -> JsValue {
    match compiler::compile(policy) {
        Ok(compiled) => match serde_json::from_str::<Vec<serde_json::Value>>(inputs) {
            Ok(input_vec) => {
                let results = compiled.evaluate_batch(&input_vec);
                let arr = js_sys::Array::new();
                for effect in results {
                    let s = match effect {
                        Effect::Allow => "ALLOW",
                        Effect::Deny => "DENY",
                    };
                    arr.push(&JsValue::from_str(s));
                }
                let obj = js_sys::Object::new();
                js_sys::Reflect::set(&obj, &JsValue::from_str("results"), &arr).unwrap();
                obj.into()
            }
            Err(e) => js_object_with_str("error", &format!("Invalid JSON array: {}", e)),
        },
        Err(e) => js_object_with_str("error", &format!("Parse error: {:?}", e)),
    }
}

// ============================================================================
// C-compatible FFI (original implementation)
// ============================================================================

/// Result codes for FFI functions.
#[repr(C)]
pub enum KaruResult {
    Ok = 0,
    ErrorParse = 1,
    ErrorEvaluate = 2,
    ErrorMemory = 3,
}

/// An opaque handle to a compiled policy.
pub struct KaruPolicy {
    inner: crate::rule::Policy,
}

/// Allocate memory for use by the host.
///
/// Returns a pointer to the allocated memory, or null on failure.
#[no_mangle]
pub extern "C" fn karu_alloc(size: usize) -> *mut u8 {
    let mut buf: Vec<u8> = Vec::with_capacity(size);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Free memory allocated by Karu.
///
/// # Safety
/// The pointer must have been allocated by `karu_alloc` or returned
/// by another Karu function, and must not have been freed already.
#[no_mangle]
pub unsafe extern "C" fn karu_free(ptr: *mut u8, size: usize) {
    if !ptr.is_null() {
        let _ = Vec::from_raw_parts(ptr, 0, size);
    }
}

/// Compile a policy from source code.
///
/// Returns a handle to the compiled policy, or null on error.
///
/// # Safety
/// - `source_ptr` must point to valid UTF-8 bytes
/// - `source_len` must be the exact length of the source string
#[no_mangle]
pub unsafe extern "C" fn karu_compile(source_ptr: *const u8, source_len: usize) -> *mut KaruPolicy {
    if source_ptr.is_null() {
        return std::ptr::null_mut();
    }

    let source_bytes = slice::from_raw_parts(source_ptr, source_len);
    let source = match std::str::from_utf8(source_bytes) {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    match compiler::compile(source) {
        Ok(policy) => {
            let boxed = Box::new(KaruPolicy { inner: policy });
            Box::into_raw(boxed)
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// Free a compiled policy.
///
/// # Safety
/// The handle must have been returned by `karu_compile` and not freed already.
#[no_mangle]
pub unsafe extern "C" fn karu_policy_free(policy: *mut KaruPolicy) {
    if !policy.is_null() {
        let _ = Box::from_raw(policy);
    }
}

/// Evaluate a policy against JSON input.
///
/// Returns 1 for Allow, 0 for Deny, -1 for error.
///
/// # Safety
/// - `policy` must be a valid handle from `karu_compile`
/// - `input_ptr` must point to valid UTF-8 JSON bytes
/// - `input_len` must be the exact length of the input string
#[no_mangle]
pub unsafe extern "C" fn karu_evaluate(
    policy: *const KaruPolicy,
    input_ptr: *const u8,
    input_len: usize,
) -> i32 {
    if policy.is_null() || input_ptr.is_null() {
        return -1;
    }

    let policy = &(*policy).inner;
    let input_bytes = slice::from_raw_parts(input_ptr, input_len);
    let input_str = match std::str::from_utf8(input_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };

    let input: serde_json::Value = match serde_json::from_str(input_str) {
        Ok(v) => v,
        Err(_) => return -1,
    };

    match policy.evaluate(&input) {
        Effect::Allow => 1,
        Effect::Deny => 0,
    }
}

/// Compile and evaluate in one call (convenience function).
///
/// Returns 1 for Allow, 0 for Deny, -1 for error.
///
/// # Safety
/// - `policy_ptr` must point to valid UTF-8 bytes containing policy source
/// - `input_ptr` must point to valid UTF-8 JSON bytes
#[no_mangle]
pub unsafe extern "C" fn karu_eval_once(
    policy_ptr: *const u8,
    policy_len: usize,
    input_ptr: *const u8,
    input_len: usize,
) -> i32 {
    if policy_ptr.is_null() || input_ptr.is_null() {
        return -1;
    }

    let policy_bytes = slice::from_raw_parts(policy_ptr, policy_len);
    let policy_str = match std::str::from_utf8(policy_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };

    let input_bytes = slice::from_raw_parts(input_ptr, input_len);
    let input_str = match std::str::from_utf8(input_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };

    let policy = match compiler::compile(policy_str) {
        Ok(p) => p,
        Err(_) => return -1,
    };

    let input: serde_json::Value = match serde_json::from_str(input_str) {
        Ok(v) => v,
        Err(_) => return -1,
    };

    match policy.evaluate(&input) {
        Effect::Allow => 1,
        Effect::Deny => 0,
    }
}

/// Compile a Cedar policy by importing it to Karu first.
///
/// Returns a handle to the compiled policy, or null on error.
///
/// # Safety
/// - `source_ptr` must point to valid UTF-8 bytes containing Cedar policy source
/// - `source_len` must be the exact length of the source string
#[cfg(feature = "cedar")]
#[no_mangle]
pub unsafe extern "C" fn karu_compile_cedar(
    source_ptr: *const u8,
    source_len: usize,
) -> *mut KaruPolicy {
    if source_ptr.is_null() {
        return std::ptr::null_mut();
    }

    let source_bytes = slice::from_raw_parts(source_ptr, source_len);
    let source = match std::str::from_utf8(source_bytes) {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    match crate::compile_cedar(source) {
        Ok(policy) => {
            let boxed = Box::new(KaruPolicy { inner: policy });
            Box::into_raw(boxed)
        }
        Err(_) => std::ptr::null_mut(),
    }
}

/// Import a Cedar policy, compile, and evaluate in one call.
///
/// Returns 1 for Allow, 0 for Deny, -1 for error.
///
/// # Safety
/// - `policy_ptr` must point to valid UTF-8 bytes containing Cedar policy source
/// - `input_ptr` must point to valid UTF-8 JSON bytes
#[cfg(feature = "cedar")]
#[no_mangle]
pub unsafe extern "C" fn karu_eval_cedar_once(
    policy_ptr: *const u8,
    policy_len: usize,
    input_ptr: *const u8,
    input_len: usize,
) -> i32 {
    if policy_ptr.is_null() || input_ptr.is_null() {
        return -1;
    }

    let policy_bytes = slice::from_raw_parts(policy_ptr, policy_len);
    let policy_str = match std::str::from_utf8(policy_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };

    let input_bytes = slice::from_raw_parts(input_ptr, input_len);
    let input_str = match std::str::from_utf8(input_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };

    let policy = match crate::compile_cedar(policy_str) {
        Ok(p) => p,
        Err(_) => return -1,
    };

    let input: serde_json::Value = match serde_json::from_str(input_str) {
        Ok(v) => v,
        Err(_) => return -1,
    };

    match policy.evaluate(&input) {
        Effect::Allow => 1,
        Effect::Deny => 0,
    }
}

/// Transpile a Karu policy to Cedar syntax.
///
/// On success, writes the Cedar output to the buffer allocated by the host.
/// Returns the length of the Cedar output, or -1 on error.
///
/// If the output buffer is too small, returns the required size (caller should
/// allocate a larger buffer and call again).
///
/// # Safety
/// - `source_ptr` must point to valid UTF-8 bytes containing Karu policy
/// - `output_ptr` must point to a buffer of at least `output_capacity` bytes
#[cfg(feature = "cedar")]
#[no_mangle]
pub unsafe extern "C" fn karu_transpile(
    source_ptr: *const u8,
    source_len: usize,
    output_ptr: *mut u8,
    output_capacity: usize,
) -> i32 {
    if source_ptr.is_null() {
        return -1;
    }

    let source_bytes = slice::from_raw_parts(source_ptr, source_len);
    let source = match std::str::from_utf8(source_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };

    // Parse the Karu policy
    let ast = match Parser::parse(source) {
        Ok(ast) => ast,
        Err(_) => return -1,
    };

    // Transpile to Cedar
    let cedar = match transpile::to_cedar(&ast) {
        Ok(c) => c,
        Err(_) => return -1,
    };

    let cedar_bytes = cedar.as_bytes();
    let cedar_len = cedar_bytes.len();

    // If output buffer is null, just return required size
    if output_ptr.is_null() {
        return cedar_len as i32;
    }

    // If buffer too small, return required size
    if output_capacity < cedar_len {
        return cedar_len as i32;
    }

    // Copy to output buffer
    std::ptr::copy_nonoverlapping(cedar_bytes.as_ptr(), output_ptr, cedar_len);
    cedar_len as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ffi_compile_and_evaluate() {
        let source = r#"allow access if role == "admin";"#;
        let input = r#"{"role": "admin"}"#;

        unsafe {
            let policy = karu_compile(source.as_ptr(), source.len());
            assert!(!policy.is_null());

            let result = karu_evaluate(policy, input.as_ptr(), input.len());
            assert_eq!(result, 1); // Allow

            let deny_input = r#"{"role": "user"}"#;
            let result2 = karu_evaluate(policy, deny_input.as_ptr(), deny_input.len());
            assert_eq!(result2, 0); // Deny

            karu_policy_free(policy);
        }
    }

    #[test]
    fn test_ffi_eval_once() {
        let source = r#"allow access if value > 10;"#;
        let input = r#"{"value": 15}"#;

        unsafe {
            let result = karu_eval_once(source.as_ptr(), source.len(), input.as_ptr(), input.len());
            assert_eq!(result, 1); // Allow
        }
    }

    #[test]
    fn test_ffi_invalid_input() {
        unsafe {
            // Null source
            let policy = karu_compile(std::ptr::null(), 0);
            assert!(policy.is_null());

            // Invalid JSON
            let source = r#"allow access;"#;
            let policy = karu_compile(source.as_ptr(), source.len());
            assert!(!policy.is_null());

            let bad_input = r#"not valid json"#;
            let result = karu_evaluate(policy, bad_input.as_ptr(), bad_input.len());
            assert_eq!(result, -1); // Error

            karu_policy_free(policy);
        }
    }

    #[test]
    fn test_alloc_free() {
        unsafe {
            let ptr = karu_alloc(1024);
            assert!(!ptr.is_null());
            karu_free(ptr, 1024);
        }
    }
}
