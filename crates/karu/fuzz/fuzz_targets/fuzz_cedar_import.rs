//! Fuzz target for the Cedar import pipeline.
//!
//! Tests that `from_cedar` and `compile_cedar` never panic on
//! arbitrary input, including malformed Cedar policy strings.

#![no_main]

use karu::cedar_import::from_cedar;
use karu::compile_cedar;
use libfuzzer_sys::fuzz_target;
use serde_json::json;

fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        // from_cedar should never panic — only Ok or Err
        match from_cedar(source) {
            Ok(program) => {
                // If import succeeded, all rules should have names
                for rule in &program.rules {
                    assert!(!rule.name.is_empty(), "Rule name should not be empty");
                }
            }
            Err(_) => {}
        }

        // compile_cedar should never panic — only Ok or Err
        match compile_cedar(source) {
            Ok(policy) => {
                // Evaluate the imported policy with diverse inputs
                let inputs = [
                    json!({"principal": {"role": "admin"}, "action": "read", "resource": "doc"}),
                    json!({}),
                    json!(null),
                ];
                for input in &inputs {
                    let _ = policy.evaluate(input);
                }
            }
            Err(_) => {}
        }
    }
});
