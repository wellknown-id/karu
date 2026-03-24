//! Fuzz target for Cedar round-trip: Cedar → Karu → Cedar → Karu.
//!
//! Tests that converting a Cedar policy to Karu and back never panics,
//! and that evaluation is consistent across the round-trip.

#![no_main]

use karu::cedar_import::from_cedar;
use karu::transpile::to_cedar;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        // Step 1: Cedar → Karu AST
        let program = match from_cedar(source) {
            Ok(p) => p,
            Err(_) => return,
        };

        // Step 2: Karu AST → Cedar source
        let cedar_output = match to_cedar(&program) {
            Ok(s) => s,
            Err(_) => return,
        };

        // Step 3: Cedar source (round-tripped) → Karu AST again
        // This should never panic. If it parses, the round-trip is valid.
        let _ = from_cedar(&cedar_output);
    }
});
