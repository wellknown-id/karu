//! Cedar round-trip semantic equivalence fuzzer.
//!
//! Tests that converting Cedar → Karu → Cedar → Karu produces policies
//! that evaluate identically to the original. Upgrades the original
//! crash-only version with actual evaluation consistency assertions.

#![no_main]

use karu::cedar_import::from_cedar;
use karu::compiler::compile_program;
use karu::transpile::to_cedar;
use libfuzzer_sys::fuzz_target;
use serde_json::json;
use std::collections::HashSet;

/// Standard test inputs to check evaluation consistency across round-trip.
fn test_inputs() -> Vec<serde_json::Value> {
    vec![
        json!({}),
        json!({"principal": "alice", "action": "read", "resource": "doc1"}),
        json!({"principal": {"id": "bob", "role": "admin"}, "action": "write", "resource": {"id": "secret"}}),
        json!({"action": "delete", "role": "user", "active": true}),
        json!({"principal": "charlie", "action": "manage", "context": {"ip": "10.0.0.1"}}),
    ]
}

fuzz_target!(|data: &[u8]| {
    let source = match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(_) => return,
    };

    // Step 1: Cedar → Karu AST
    let program_a = match from_cedar(source) {
        Ok(p) => p,
        Err(_) => return,
    };

    // Step 2: Compile to evaluate
    let policy_a = match compile_program(&program_a, &HashSet::new()) {
        Ok(p) => p,
        Err(_) => return,
    };

    // Step 3: Karu AST → Cedar source
    let cedar_output = match to_cedar(&program_a) {
        Ok(s) => s,
        Err(_) => return,
    };

    // Step 4: Cedar source (round-tripped) → Karu AST again
    let program_b = match from_cedar(&cedar_output) {
        Ok(p) => p,
        Err(_) => return, // Transpile output not re-parseable is a separate issue
    };

    // Step 5: Compile round-tripped version
    let policy_b = match compile_program(&program_b, &HashSet::new()) {
        Ok(p) => p,
        Err(_) => return,
    };

    // Step 6: Both must evaluate identically across all test inputs
    for input in &test_inputs() {
        let a = policy_a.evaluate(input);
        let b = policy_b.evaluate(input);
        assert_eq!(a, b,
            "Round-trip changed evaluation!\nOriginal Cedar:\n{}\nRound-tripped Cedar:\n{}\nInput: {:?}\nBefore: {:?}\nAfter: {:?}",
            source, cedar_output, input, a, b
        );
    }
});
