//! Differential fuzzer: verifies that rule ordering doesn't change policy decisions.
//!
//! Given a fuzz-generated policy with N rules, this target:
//! 1. Compiles the original ordering → evaluates
//! 2. Reverses the rules → recompiles → evaluates
//! 3. Assert both produce the same Effect
//!
//! This catches ordering-sensitive bugs in deny-overrides evaluation
//! and accidental mutation of shared state between rules.

#![no_main]

use karu::parser::Parser;
use karu::compiler::compile_program;
use libfuzzer_sys::fuzz_target;
use serde_json::json;
use std::collections::HashSet;

/// Fixed test inputs covering diverse shapes.
fn test_inputs() -> Vec<serde_json::Value> {
    vec![
        json!({}),
        json!({"action": "read", "role": "admin", "principal": "alice"}),
        json!({"action": "delete", "principal": {"id": "bob"}, "resource": {"owner": {"id": "alice"}}}),
        json!({"role": "user", "active": true, "level": 5}),
        json!({"banned": true, "role": "admin"}),
        json!({"user": {"status": "blocked"}, "action": "read"}),
    ]
}

fuzz_target!(|data: &[u8]| {
    let source = match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(_) => return,
    };

    // Parse into AST
    let mut program = match Parser::parse(source) {
        Ok(p) => p,
        Err(_) => return,
    };

    // Need at least 2 rules for reordering to matter
    if program.rules.len() < 2 || program.rules.len() > 10 {
        return;
    }

    // Compile original ordering
    let policy_a = match compile_program(&program, &HashSet::new()) {
        Ok(p) => p,
        Err(_) => return,
    };

    // Reverse rules
    program.rules.reverse();
    let policy_b = match compile_program(&program, &HashSet::new()) {
        Ok(p) => p,
        Err(_) => return,
    };

    // Both orderings must produce the same result
    for input in &test_inputs() {
        let a = policy_a.evaluate(input);
        let b = policy_b.evaluate(input);
        assert_eq!(a, b,
            "Rule reordering changed result!\nOriginal: {:?}\nReversed: {:?}\nInput: {:?}\nSource: {}",
            a, b, input, source
        );
    }
});
