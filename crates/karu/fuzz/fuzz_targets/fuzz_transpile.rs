//! Fuzz target for the Karu transpiler.
//!
//! Tests that transpilation to Cedar never panics and handles
//! all parsed policies gracefully, including context handling.

#![no_main]

use karu::parser::Parser;
use karu::transpile::to_cedar;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        // Parse first - only fuzz transpile on valid ASTs
        if let Ok(ast) = Parser::parse(source) {
            // to_cedar should never panic - only return Ok or Err
            // This exercises context extraction and all pattern conversions
            let _ = to_cedar(&ast);
        }
    }
});
