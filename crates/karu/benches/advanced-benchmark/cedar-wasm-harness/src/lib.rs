//! Thin WASM harness for Cedar.
//!
//! Provides a C-FFI wrapper around cedar-policy so it can be loaded
//! via wasmtime with the same calling convention as the Karu harness.
//!
//! Exported functions:
//!   cedar_alloc(size) -> ptr
//!   cedar_free(ptr, size)
//!   cedar_eval(policy_ptr, policy_len, input_ptr, input_len) -> i32  (parse + eval)
//!   cedar_compile(policy_ptr, policy_len) -> handle                  (compile only)
//!   cedar_evaluate(handle, input_ptr, input_len) -> i32              (eval with pre-compiled)
//!   cedar_policy_free(handle)

use cedar_policy::*;
use std::slice;

/// Allocate memory for use by the host.
#[no_mangle]
pub extern "C" fn cedar_alloc(size: usize) -> *mut u8 {
    let mut buf: Vec<u8> = Vec::with_capacity(size);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// Free memory allocated by this module.
///
/// # Safety
/// Pointer must have been returned by `cedar_alloc`.
#[no_mangle]
pub unsafe extern "C" fn cedar_free(ptr: *mut u8, size: usize) {
    if !ptr.is_null() {
        let _ = Vec::from_raw_parts(ptr, 0, size);
    }
}

// ── Opaque handle for pre-compiled policy ───────────────────────────────────

struct CedarPolicyHandle {
    policy_set: PolicySet,
    authorizer: Authorizer,
}

/// Compile a Cedar policy and return an opaque handle.
///
/// Returns a handle pointer, or null on error.
///
/// # Safety
/// `policy_ptr` must point to valid UTF-8 of `policy_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cedar_compile(
    policy_ptr: *const u8,
    policy_len: usize,
) -> *mut CedarPolicyHandle {
    if policy_ptr.is_null() {
        return std::ptr::null_mut();
    }

    let policy_bytes = slice::from_raw_parts(policy_ptr, policy_len);
    let policy_str = match std::str::from_utf8(policy_bytes) {
        Ok(s) => s,
        Err(_) => return std::ptr::null_mut(),
    };

    let policy_set: PolicySet = match policy_str.parse() {
        Ok(ps) => ps,
        Err(_) => return std::ptr::null_mut(),
    };

    let handle = Box::new(CedarPolicyHandle {
        policy_set,
        authorizer: Authorizer::new(),
    });
    Box::into_raw(handle)
}

/// Evaluate a pre-compiled Cedar policy against JSON input.
///
/// Returns 1 for Allow, 0 for Deny, -1 for error.
///
/// # Safety
/// `handle` must be a valid pointer from `cedar_compile`.
/// `input_ptr` must point to valid UTF-8 JSON of `input_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn cedar_evaluate(
    handle: *const CedarPolicyHandle,
    input_ptr: *const u8,
    input_len: usize,
) -> i32 {
    if handle.is_null() || input_ptr.is_null() {
        return -1;
    }

    let handle = &*handle;

    let input_bytes = slice::from_raw_parts(input_ptr, input_len);
    let input_str = match std::str::from_utf8(input_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };

    let context_value: serde_json::Value = match serde_json::from_str(input_str) {
        Ok(v) => v,
        Err(_) => return -1,
    };

    let context = match Context::from_json_value(context_value, None) {
        Ok(c) => c,
        Err(_) => Context::empty(),
    };

    let principal: EntityUid = match r#"User::"alice""#.parse() {
        Ok(e) => e,
        Err(_) => return -1,
    };
    let action: EntityUid = match r#"Action::"access""#.parse() {
        Ok(e) => e,
        Err(_) => return -1,
    };
    let resource: EntityUid = match r#"Resource::"res1""#.parse() {
        Ok(e) => e,
        Err(_) => return -1,
    };

    let request = match Request::new(principal, action, resource, context, None) {
        Ok(r) => r,
        Err(_) => return -1,
    };

    let entities = Entities::empty();
    let response = handle
        .authorizer
        .is_authorized(&request, &handle.policy_set, &entities);

    match response.decision() {
        Decision::Allow => 1,
        Decision::Deny => 0,
    }
}

/// Free a compiled Cedar policy handle.
///
/// # Safety
/// `handle` must have been returned by `cedar_compile`.
#[no_mangle]
pub unsafe extern "C" fn cedar_policy_free(handle: *mut CedarPolicyHandle) {
    if !handle.is_null() {
        let _ = Box::from_raw(handle);
    }
}

// ── One-shot eval (parse + eval combined) ───────────────────────────────────

/// Evaluate a Cedar policy (parse + eval in one call).
///
/// `policy_ptr`/`policy_len`: Cedar policy source (UTF-8).
/// `input_ptr`/`input_len`: JSON context (UTF-8).
///
/// Returns 1 for Allow, 0 for Deny, -1 for error.
///
/// # Safety
/// Pointers must be valid and point to UTF-8 data of the given lengths.
#[no_mangle]
pub unsafe extern "C" fn cedar_eval(
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

    let policy_set: PolicySet = match policy_str.parse() {
        Ok(ps) => ps,
        Err(_) => return -1,
    };

    let context_value: serde_json::Value = match serde_json::from_str(input_str) {
        Ok(v) => v,
        Err(_) => return -1,
    };

    let context = match Context::from_json_value(context_value, None) {
        Ok(c) => c,
        Err(_) => Context::empty(),
    };

    let principal: EntityUid = match r#"User::"alice""#.parse() {
        Ok(e) => e,
        Err(_) => return -1,
    };
    let action: EntityUid = match r#"Action::"access""#.parse() {
        Ok(e) => e,
        Err(_) => return -1,
    };
    let resource: EntityUid = match r#"Resource::"res1""#.parse() {
        Ok(e) => e,
        Err(_) => return -1,
    };

    let request = match Request::new(principal, action, resource, context, None) {
        Ok(r) => r,
        Err(_) => return -1,
    };

    let entities = Entities::empty();
    let authorizer = Authorizer::new();
    let response = authorizer.is_authorized(&request, &policy_set, &entities);

    match response.decision() {
        Decision::Allow => 1,
        Decision::Deny => 0,
    }
}
