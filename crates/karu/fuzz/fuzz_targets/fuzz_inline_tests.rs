//! Fuzz target for inline test parsing and execution.
//!
//! Tests that the test block parser handles arbitrary input without
//! panicking (nested objects, arrays, shorthand syntax), and that
//! test execution never panics.

#![no_main]

use karu::parser::Parser;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        // Parse should never panic, even with malformed test blocks
        match Parser::parse(source) {
            Ok(program) => {
                // If there are tests, validate their structure
                for test in &program.tests {
                    assert!(!test.name.is_empty(), "Test name should not be empty");
                    for entity in &test.entities {
                        assert!(!entity.kind.is_empty(), "Entity kind should not be empty");
                        // If shorthand, there should be exactly one field named "id"
                        if entity.shorthand {
                            assert_eq!(entity.fields.len(), 1, "Shorthand should have exactly 1 field");
                            assert_eq!(entity.fields[0].0, "id", "Shorthand field should be named 'id'");
                        }
                    }
                }

                // If tests exist and policy compiles, run_inline_tests should not panic
                if !program.tests.is_empty() {
                    let _ = karu::lsp_core::run_inline_tests(source);
                }
            }
            Err(_) => {
                // Parse error is fine
            }
        }
    }
});
