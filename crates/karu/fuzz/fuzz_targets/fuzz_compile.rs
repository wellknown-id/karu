//! Fuzz target for the full Karu compile pipeline.
//!
//! Tests that parsing + compiling never panics and produces
//! valid policies that can be evaluated.

#![no_main]

use karu::compile;
use libfuzzer_sys::fuzz_target;
use serde_json::json;

fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        // compile() handles parsing + compilation
        // It should never panic - only return Ok or Err
        match compile(source) {
            Ok(policy) => {
                // If we got a policy, evaluation should never panic
                let test_requests = [
                    json!({
                        "principal": { "id": "test", "role": "user" },
                        "action": "test",
                        "resource": { "id": "res1" },
                        "context": { "ipAddress": "10.0.0.1", "readOnly": false }
                    }),
                    json!({}),
                    json!(null),
                    json!("string"),
                    json!(42),
                ];

                for request in &test_requests {
                    // Evaluate should never panic regardless of input
                    let _ = policy.evaluate(request);
                }
            }
            Err(_) => {
                // Compile/parse error is fine - just shouldn't panic
            }
        }
    }
});
