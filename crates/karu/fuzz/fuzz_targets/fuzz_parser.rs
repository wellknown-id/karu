//! Fuzz target for the Karu parser.
//!
//! Tests that the parser never panics on any input. The parser now
//! fails fast (returns Err) on invalid input — it should never panic.

#![no_main]

use karu::parser::Parser;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Only test valid UTF-8 strings
    if let Ok(source) = std::str::from_utf8(data) {
        // Parser should NEVER panic — Err is fine, panic is not
        match Parser::parse(source) {
            Ok(program) => {
                // Valid parse - rules should be well-formed
                for rule in &program.rules {
                    assert!(!rule.name.is_empty(), "Rule name should not be empty");
                }
            }
            Err(_) => {
                // Expected for invalid input — fail-fast is correct
            }
        }
    }
});
