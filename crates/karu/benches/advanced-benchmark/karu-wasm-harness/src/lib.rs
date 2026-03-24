//! Thin WASM harness for Karu.
//!
//! Re-exports the C-FFI functions from karu::wasm so they appear
//! as WASM module exports when compiled to wasm32-wasip1.
//!
//! Exported functions:
//!   karu_alloc(size) -> ptr
//!   karu_free(ptr, size)
//!   karu_compile(source_ptr, source_len) -> policy_handle
//!   karu_compile_cedar(source_ptr, source_len) -> policy_handle
//!   karu_evaluate(policy_handle, input_ptr, input_len) -> i32
//!   karu_eval_once(policy_ptr, policy_len, input_ptr, input_len) -> i32
//!   karu_eval_cedar_once(policy_ptr, policy_len, input_ptr, input_len) -> i32
//!   karu_policy_free(policy_handle)

// Re-export the FFI functions so they're visible as WASM exports
pub use karu::wasm::{
    karu_alloc, karu_compile, karu_compile_cedar, karu_eval_cedar_once, karu_eval_once,
    karu_evaluate, karu_free, karu_policy_free,
};
