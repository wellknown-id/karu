//! Fuzz target for the Karu lexer.
//!
//! Tests that the lexer never panics and always returns either
//! a valid token stream or a well-formed LexError.

#![no_main]

use karu::lexer::Lexer;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        // Lexer should never panic
        let result = Lexer::tokenize_spanned(source);

        // Result should be Ok or a proper LexError - never panic
        match result {
            Ok(tokens) => {
                // All tokens should have valid positions
                for token in &tokens {
                    assert!(token.line >= 1, "Line number should be >= 1");
                    assert!(token.column >= 1, "Column number should be >= 1");
                }
            }
            Err(e) => {
                // Error should have valid position info
                assert!(e.line >= 1, "Error line should be >= 1");
                assert!(e.column >= 1, "Error column should be >= 1");
                assert!(!e.message.is_empty(), "Error message should not be empty");
            }
        }
    }
});
