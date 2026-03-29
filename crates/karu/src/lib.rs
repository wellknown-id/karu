//! # Karu
//!
//! An embeddable policy engine focusing on structural pattern matching over
//! arbitrary JSON data. Inspired by Polar/Oso, designed to solve the complex
//! hierarchical data validation that strict-schema engines like Cedar cannot handle.
//!
//! ## Core Philosophy
//!
//! - **Structure over Schema**: We don't enforce schemas. We match patterns.
//! - **Search, Don't Index**: Lists are searched automatically with `in`.
//! - **Partial Matching**: Pattern `{a: 1}` matches `{a: 1, b: 2}`.
//! - **Optionally Strict**: Flip a switch for Cedar-level rigor when needed.
//!
//! ## Quick Start
//!
//! ```rust
//! use karu::{Policy, Rule, Condition, Pattern, Effect};
//! use serde_json::json;
//!
//! // Create a policy
//! let policy = Policy::new()
//!     .with_rule(Rule::allow("admin_access", vec![
//!         Condition::eq("principal.role", Pattern::literal("admin")),
//!     ]))
//!     .with_rule(Rule::deny("block_dangerous", vec![
//!         Condition::eq("action", Pattern::literal("delete")),
//!     ]));
//!
//! // Evaluate
//! let request = json!({
//!     "principal": {"role": "admin"},
//!     "action": "read"
//! });
//!
//! assert_eq!(policy.evaluate(&request), Effect::Allow);
//! ```
//!
//! ## Collection Search (The Killer Feature)
//!
//! ```rust
//! use karu::{Policy, Rule, Condition, Pattern, Effect};
//! use serde_json::json;
//!
//! let policy = Policy::new()
//!     .with_rule(Rule::allow("check_capability", vec![
//!         Condition::contains(
//!             "user.permissions",
//!             Pattern::object([
//!                 ("action", Pattern::literal("write")),
//!                 ("resource", Pattern::literal("/data/*")),
//!             ]),
//!         ),
//!     ]));
//!
//! let request = json!({
//!     "user": {
//!         "permissions": [
//!             {"action": "read", "resource": "*"},
//!             {"action": "write", "resource": "/data/*"}
//!         ]
//!     }
//! });
//!
//! assert_eq!(policy.evaluate(&request), Effect::Allow);
//! ```

pub mod ast;
pub mod bindings;
#[cfg(feature = "cedar")]
pub mod cedar_import;
#[cfg(feature = "cedar")]
pub mod cedar_parser;
#[cfg(feature = "cedar")]
pub mod cedar_schema_parser;
pub mod compiler;
pub mod diff;
#[cfg(feature = "dev")]
pub mod format;
pub mod lexer;
pub mod lint;
pub mod matcher;
pub mod parser;
pub mod path;
pub mod pattern;
pub mod resolver;
pub mod rule;
pub mod schema;
pub mod simulate;
pub mod transpile;
pub mod type_registry;
pub mod wasm;

#[cfg(feature = "lsp")]
pub mod lsp;

#[cfg(feature = "dev")]
pub mod grammar;

#[cfg(all(feature = "dev", feature = "cedar"))]
pub mod cedar_grammar;

#[cfg(all(feature = "dev", feature = "cedar"))]
pub mod cedar_schema_grammar;

// Re-exports for convenience
pub use bindings::Bindings;
#[cfg(feature = "cedar")]
pub use cedar_import::{
    from_cedar, from_cedar_to_source, from_cedar_with_schema, from_cedarschema,
};
#[cfg(feature = "cedar")]
pub use cedar_schema_parser::parse_cedarschema;
pub use compiler::{compile, compile_with_host_asserts};
pub use matcher::{
    all_match, any_match_with_bindings, any_matches, match_with_bindings, matches, matches_ref,
};
pub use path::{Path, PathSegment};
pub use pattern::{Pattern, TypeConstraint};
pub use rule::{
    Condition, ConditionExpr, Effect, EvalContext, HostAssertFn, IndexedPolicy, Operator, Policy,
    Rule,
};
#[cfg(feature = "cedar")]
pub use transpile::{to_cedar, to_cedarschema, TranspileError};

#[cfg(feature = "cedar")]
mod cedar_api {
    use crate::rule::Policy;

    /// Compile a Cedar policy by first converting it to Karu syntax.
    ///
    /// This is a convenience function that chains `from_cedar` and `compile`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use karu::compile_cedar;
    /// use karu::rule::Effect;
    /// use serde_json::json;
    ///
    /// let policy = compile_cedar(
    ///     r#"permit(principal, action, resource) when { principal.role == "admin" };"#
    /// ).unwrap();
    ///
    /// let input = json!({"principal": {"role": "admin"}, "action": "read", "resource": "doc"});
    /// assert_eq!(policy.evaluate(&input), Effect::Allow);
    /// ```
    pub fn compile_cedar(cedar_source: &str) -> Result<Policy, CompileCedarError> {
        let program = crate::cedar_import::from_cedar(cedar_source)
            .map_err(|e| CompileCedarError::Import(e.to_string()))?;
        crate::compiler::compile_program(&program, &std::collections::HashSet::new())
            .map_err(|e| CompileCedarError::Compile(format!("{:?}", e)))
    }

    /// Error type for `compile_cedar`.
    #[derive(Debug)]
    pub enum CompileCedarError {
        /// Error during Cedar → Karu import.
        Import(String),
        /// Error during Karu compilation.
        Compile(String),
    }

    impl std::fmt::Display for CompileCedarError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Import(e) => write!(f, "Cedar import error: {}", e),
                Self::Compile(e) => write!(f, "Compile error: {}", e),
            }
        }
    }

    impl std::error::Error for CompileCedarError {}

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_compile_cedar_error_display() {
            let import_err = CompileCedarError::Import("invalid syntax".to_string());
            assert_eq!(
                format!("{}", import_err),
                "Cedar import error: invalid syntax"
            );

            let compile_err = CompileCedarError::Compile("type error".to_string());
            assert_eq!(format!("{}", compile_err), "Compile error: type error");
        }

        #[test]
        fn test_compile_cedar_import_error() {
            let result = compile_cedar("not a cedar policy");
            assert!(matches!(result, Err(CompileCedarError::Import(_))));
        }

        #[test]
        fn test_compile_cedar_compile_error() {
            // Note: `compile_cedar` delegates to `cedar_import::from_cedar` and then `compiler::compile_program`.
            // Any cedar parsing errors or conversion errors result in `CompileCedarError::Import`.
            // Current `from_cedar` implementation generates simple ASTs that do not fail `compile_program`
            // (e.g., it doesn't generate circular assertions or unknown types for `is` operations).
            // Thus, triggering `CompileCedarError::Compile` through `compile_cedar` is currently unreachable
            // organically, but the enum variant exists for future-proofing as Cedar integration expands.
            // This test verifies the enum variant behaves correctly when explicitly created.
            let compile_err = CompileCedarError::Compile("AST compilation failed".to_string());
            assert!(matches!(compile_err, CompileCedarError::Compile(_)));
            assert_eq!(format!("{}", compile_err), "Compile error: AST compilation failed");
        }

        #[test]
        fn test_compile_cedar_success() {
            use crate::rule::Effect;
            use serde_json::json;

            let policy = compile_cedar(
                r#"permit(principal, action, resource) when { principal.role == "admin" };"#,
            )
            .unwrap();

            let input = json!({
                "principal": {"role": "admin"},
                "action": "read",
                "resource": "doc"
            });
            assert_eq!(policy.evaluate(&input), Effect::Allow);

            let input_deny = json!({
                "principal": {"role": "user"},
                "action": "read",
                "resource": "doc"
            });
            assert_eq!(policy.evaluate(&input_deny), Effect::Deny);
        }
    }
}

#[cfg(feature = "cedar")]
pub use cedar_api::{compile_cedar, CompileCedarError};
