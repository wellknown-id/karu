//! Pattern types for Karu's structural matching.
//!
//! Patterns define the shape of data we're looking for. Unlike strict schemas,
//! patterns match by structure - extra fields are ignored (duck typing).

use crate::path::Path;
use serde_json::Value;
use std::collections::HashMap;

/// Type constraints for pattern matching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeConstraint {
    String,
    Number,
    Bool,
    Array,
    Object,
    Null,
}

/// A pattern that can be matched against JSON data.
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    /// Matches exact JSON values (numbers, strings, booleans, null).
    Literal(Value),

    /// Matches objects containing specific keys with specific sub-patterns.
    /// Extra keys in the data are ignored (duck typing / partial matching).
    Object(HashMap<String, Pattern>),

    /// Matches arrays containing specific elements in order.
    Array(Vec<Pattern>),

    /// Matches any value (wildcard). If named, can be bound for later use.
    Variable(Option<String>),

    /// Negation: matches if the inner pattern does NOT match.
    Not(Box<Pattern>),

    /// Matches if ANY of the sub-patterns match.
    Or(Vec<Pattern>),

    /// Matches if ALL of the sub-patterns match.
    And(Vec<Pattern>),

    /// References another path in the data (for path-to-path comparison).
    /// Resolved at evaluation time to extract the comparison value.
    PathRef(Path),

    /// Glob pattern for wildcard string matching (e.g., "/data/*", "*.json").
    Glob(String),

    /// Type constraint pattern (e.g., is_string, is_number).
    Type(TypeConstraint),
}

impl Pattern {
    /// Create a literal pattern from any JSON value.
    pub fn literal<V: Into<Value>>(v: V) -> Self {
        Pattern::Literal(v.into())
    }

    /// Create an anonymous wildcard pattern.
    pub fn wildcard() -> Self {
        Pattern::Variable(None)
    }

    /// Create a named variable pattern.
    pub fn var(name: impl Into<String>) -> Self {
        Pattern::Variable(Some(name.into()))
    }

    /// Create an object pattern from key-pattern pairs.
    pub fn object<I, K>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (K, Pattern)>,
        K: Into<String>,
    {
        Pattern::Object(pairs.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    /// Create an array pattern.
    pub fn array(patterns: Vec<Pattern>) -> Self {
        Pattern::Array(patterns)
    }

    /// Negate this pattern.
    #[allow(clippy::should_implement_trait)]
    pub fn not(self) -> Self {
        Pattern::Not(Box::new(self))
    }

    /// Create an OR pattern.
    pub fn or(patterns: Vec<Pattern>) -> Self {
        Pattern::Or(patterns)
    }

    /// Create an AND pattern.
    pub fn and(patterns: Vec<Pattern>) -> Self {
        Pattern::And(patterns)
    }

    /// Create a glob pattern for wildcard string matching.
    ///
    /// Supports `*` (any characters) and `?` (single character).
    ///
    /// # Example
    ///
    /// ```rust
    /// use karu::Pattern;
    /// use karu::matches;
    /// use serde_json::json;
    ///
    /// let pattern = Pattern::glob("/data/*");
    /// assert!(matches(&json!("/data/file.txt"), &pattern));
    /// assert!(!matches(&json!("/other/file.txt"), &pattern));
    /// ```
    pub fn glob(pattern: impl Into<String>) -> Self {
        Pattern::Glob(pattern.into())
    }

    /// Create a type constraint pattern.
    pub fn type_of(constraint: TypeConstraint) -> Self {
        Pattern::Type(constraint)
    }

    /// Shorthand for string type constraint.
    pub fn is_string() -> Self {
        Pattern::Type(TypeConstraint::String)
    }

    /// Shorthand for number type constraint.
    pub fn is_number() -> Self {
        Pattern::Type(TypeConstraint::Number)
    }

    /// Shorthand for boolean type constraint.
    pub fn is_bool() -> Self {
        Pattern::Type(TypeConstraint::Bool)
    }

    /// Shorthand for array type constraint.
    pub fn is_array() -> Self {
        Pattern::Type(TypeConstraint::Array)
    }

    /// Shorthand for object type constraint.
    pub fn is_object() -> Self {
        Pattern::Type(TypeConstraint::Object)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_pattern_construction() {
        let lit = Pattern::literal(42);
        assert!(matches!(lit, Pattern::Literal(_)));

        let wild = Pattern::wildcard();
        assert!(matches!(wild, Pattern::Variable(None)));

        let var = Pattern::var("x");
        assert!(matches!(var, Pattern::Variable(Some(ref name)) if name == "x"));

        let obj = Pattern::object([
            ("name", Pattern::literal("test")),
            ("value", Pattern::wildcard()),
        ]);
        assert!(matches!(obj, Pattern::Object(_)));
    }

    #[test]
    fn test_pattern_literal_equality() {
        let p1 = Pattern::literal(json!(42));
        let p2 = Pattern::literal(json!(42));
        let p3 = Pattern::literal(json!("hello"));

        assert_eq!(p1, p2);
        assert_ne!(p1, p3);
    }
}
