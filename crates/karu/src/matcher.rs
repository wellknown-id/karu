//! The unification/matcher engine - the heart of Karu.
//!
//! This module determines if JSON data matches a Pattern using structural
//! matching with duck typing semantics.

use crate::bindings::Bindings;
use crate::pattern::Pattern;
use serde_json::Value;

/// Check if a JSON value matches a pattern.
///
/// # Duck Typing Semantics
/// - Object patterns match if the data contains ALL required keys
/// - Extra keys in the data are IGNORED
/// - Array patterns match element-by-element
///
/// # Examples
/// ```
/// use karu::{Pattern, matches};
/// use serde_json::json;
///
/// // Literal matching
/// assert!(matches(&json!(42), &Pattern::literal(42)));
///
/// // Object partial matching (duck typing)
/// let pattern = Pattern::object([
///     ("name", Pattern::literal("lhs")),
/// ]);
/// let data = json!({"name": "lhs", "extra": "ignored"});
/// assert!(matches(&data, &pattern));
/// ```
pub fn matches(data: &Value, pattern: &Pattern) -> bool {
    match_with_bindings(data, pattern).is_some()
}

/// Check if a JSON value matches a pattern (reference-only, no bindings allocation).
///
/// This is the fast path used during condition evaluation when bindings
/// are not needed. Avoids creating a Bindings struct entirely.
#[inline]
pub fn matches_ref(data: &Value, pattern: &Pattern) -> bool {
    match_ref_inner(data, pattern)
}

/// Check if a JSON value matches a pattern, returning captured bindings.
///
/// Returns `Some(bindings)` if the pattern matches, `None` otherwise.
/// Named variables (`Pattern::Variable(Some(name))`) capture their matched values.
///
/// # Examples
/// ```
/// use karu::{Pattern, match_with_bindings};
/// use serde_json::json;
///
/// let pattern = Pattern::object([
///     ("name", Pattern::var("captured_name")),
///     ("value", Pattern::literal(10)),
/// ]);
/// let data = json!({"name": "lhs", "value": 10, "extra": "ignored"});
///
/// let bindings = match_with_bindings(&data, &pattern).unwrap();
/// assert_eq!(bindings.get("captured_name"), Some(&json!("lhs")));
/// ```
pub fn match_with_bindings(data: &Value, pattern: &Pattern) -> Option<Bindings> {
    if !pattern_needs_bindings(pattern) {
        return if match_ref_inner(data, pattern) {
            Some(Bindings::new())
        } else {
            None
        };
    }
    let mut bindings = Bindings::new();
    if match_inner(data, pattern, &mut bindings) {
        Some(bindings)
    } else {
        None
    }
}

/// Internal recursive matcher that populates bindings.
fn match_inner(data: &Value, pattern: &Pattern, bindings: &mut Bindings) -> bool {
    match pattern {
        Pattern::Literal(lit_val) => data == lit_val,

        Pattern::Variable(name) => {
            // Capture the value if the variable has a name
            if let Some(var_name) = name {
                bindings.bind(var_name.clone(), data.clone());
            }
            true // Wildcards always match
        }

        Pattern::Object(required_fields) => {
            // Duck typing: data must contain ALL keys from pattern.
            // Extra keys in data are IGNORED.
            if let Some(data_map) = data.as_object() {
                for (req_key, req_pattern) in required_fields {
                    match data_map.get(req_key) {
                        Some(val) => {
                            if !match_inner(val, req_pattern, bindings) {
                                return false;
                            }
                        }
                        None => return false,
                    }
                }
                true
            } else {
                false
            }
        }

        Pattern::Array(patterns) => {
            if let Some(arr) = data.as_array() {
                if arr.len() != patterns.len() {
                    return false;
                }
                for (item, pat) in arr.iter().zip(patterns.iter()) {
                    if !match_inner(item, pat, bindings) {
                        return false;
                    }
                }
                true
            } else {
                false
            }
        }

        Pattern::Not(inner) => !match_inner(data, inner, &mut Bindings::new()),

        Pattern::Or(alternatives) => {
            // Return first matching alternative's bindings
            for alt in alternatives {
                let mut alt_bindings = Bindings::new();
                if match_inner(data, alt, &mut alt_bindings) {
                    bindings.extend(alt_bindings);
                    return true;
                }
            }
            false
        }

        Pattern::And(conjuncts) => {
            for conj in conjuncts {
                if !match_inner(data, conj, bindings) {
                    return false;
                }
            }
            true
        }

        // PathRef is resolved at condition evaluation time, not here.
        // If we reach here, it means the pattern wasn't resolved properly.
        Pattern::PathRef(_) => false,

        Pattern::Glob(glob_pattern) => {
            // Simple glob matching for strings
            if let Some(s) = data.as_str() {
                glob_match(glob_pattern, s)
            } else {
                false
            }
        }

        Pattern::Type(constraint) => {
            use crate::pattern::TypeConstraint;
            match constraint {
                TypeConstraint::String => data.is_string(),
                TypeConstraint::Number => data.is_number(),
                TypeConstraint::Bool => data.is_boolean(),
                TypeConstraint::Array => data.is_array(),
                TypeConstraint::Object => data.is_object(),
                TypeConstraint::Null => data.is_null(),
            }
        }
    }
}

/// Fast reference-only matcher - no Bindings allocation.
///
/// Handles all pattern types without creating a Bindings struct.
/// Variables always match (bindings are discarded).
#[inline]
fn match_ref_inner(data: &Value, pattern: &Pattern) -> bool {
    match pattern {
        Pattern::Literal(lit_val) => data == lit_val,
        Pattern::Variable(_) => true, // Wildcards always match
        Pattern::Object(required_fields) => {
            if let Some(data_map) = data.as_object() {
                for (req_key, req_pattern) in required_fields {
                    match data_map.get(req_key) {
                        Some(val) => {
                            if !match_ref_inner(val, req_pattern) {
                                return false;
                            }
                        }
                        None => return false,
                    }
                }
                true
            } else {
                false
            }
        }
        Pattern::Array(patterns) => {
            if let Some(arr) = data.as_array() {
                if arr.len() != patterns.len() {
                    return false;
                }
                for (item, pat) in arr.iter().zip(patterns.iter()) {
                    if !match_ref_inner(item, pat) {
                        return false;
                    }
                }
                true
            } else {
                false
            }
        }
        Pattern::Not(inner) => !match_ref_inner(data, inner),
        Pattern::Or(alternatives) => alternatives.iter().any(|alt| match_ref_inner(data, alt)),
        Pattern::And(conjuncts) => conjuncts.iter().all(|conj| match_ref_inner(data, conj)),
        Pattern::PathRef(_) => false,
        Pattern::Glob(glob_pattern) => {
            if let Some(s) = data.as_str() {
                glob_match(glob_pattern, s)
            } else {
                false
            }
        }
        Pattern::Type(constraint) => {
            use crate::pattern::TypeConstraint;
            match constraint {
                TypeConstraint::String => data.is_string(),
                TypeConstraint::Number => data.is_number(),
                TypeConstraint::Bool => data.is_boolean(),
                TypeConstraint::Array => data.is_array(),
                TypeConstraint::Object => data.is_object(),
                TypeConstraint::Null => data.is_null(),
            }
        }
    }
}

/// Check if a pattern contains named variables requiring bindings.
fn pattern_needs_bindings(pattern: &Pattern) -> bool {
    match pattern {
        Pattern::Variable(Some(_)) => true,
        Pattern::Variable(None)
        | Pattern::Literal(_)
        | Pattern::PathRef(_)
        | Pattern::Glob(_)
        | Pattern::Type(_) => false,
        Pattern::Object(fields) => fields.values().any(pattern_needs_bindings),
        Pattern::Array(items) => items.iter().any(pattern_needs_bindings),
        Pattern::Not(inner) => pattern_needs_bindings(inner),
        Pattern::Or(alternatives) => alternatives.iter().any(pattern_needs_bindings),
        Pattern::And(conjuncts) => conjuncts.iter().any(pattern_needs_bindings),
    }
}

/// Simple glob pattern matching (iterative, O(n*m) worst-case).
///
/// Supports `*` (any characters) and `?` (single character).
///
/// Uses an iterative two-pointer algorithm with backtracking bookmarks
/// to avoid the exponential blowup of recursive approaches.
fn glob_match(pattern: &str, text: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let txt: Vec<char> = text.chars().collect();
    let (plen, tlen) = (pat.len(), txt.len());

    let mut pi = 0; // pattern index
    let mut ti = 0; // text index
    let mut star_pi = usize::MAX; // pattern index after last '*'
    let mut star_ti = usize::MAX; // text index when last '*' was hit

    while ti < tlen {
        if pi < plen && (pat[pi] == '?' || pat[pi] == txt[ti]) {
            // Exact or single-char wildcard match - advance both
            pi += 1;
            ti += 1;
        } else if pi < plen && pat[pi] == '*' {
            // Star: bookmark this position and advance pattern
            star_pi = pi + 1;
            star_ti = ti;
            pi += 1;
        } else if star_pi != usize::MAX {
            // Mismatch but we have a star bookmark - backtrack:
            // let the star consume one more character from text
            star_ti += 1;
            ti = star_ti;
            pi = star_pi;
        } else {
            return false;
        }
    }

    // Consume trailing stars in pattern
    while pi < plen && pat[pi] == '*' {
        pi += 1;
    }

    pi == plen
}

/// Check if ANY element in an array matches a pattern.
///
/// This is the key feature for searching collections - the `in` operator.
/// Uses the allocation-free `matches_ref` path since bindings are not needed.
#[inline]
pub fn any_matches(data: &Value, pattern: &Pattern) -> bool {
    if let Some(arr) = data.as_array() {
        for item in arr {
            if match_ref_inner(item, pattern) {
                return true;
            }
        }
    }
    // Reverse case: data is a scalar, pattern is a Literal(Array).
    // Check if data appears in the array (supports `path in ["a", "b"]`).
    if let Pattern::Literal(Value::Array(arr)) = pattern {
        if arr.iter().any(|item| item == data) {
            return true;
        }
    }
    false
}

/// Check if ANY element in an array matches, returning the bindings from the first match.
pub fn any_match_with_bindings(data: &Value, pattern: &Pattern) -> Option<Bindings> {
    if !pattern_needs_bindings(pattern) {
        if let Some(arr) = data.as_array() {
            for item in arr {
                if match_ref_inner(item, pattern) {
                    return Some(Bindings::new());
                }
            }
        }
        // Reverse case: data is a scalar, pattern is a Literal(Array).
        // Check if data appears in the array (supports `path in ["a", "b"]`).
        if let Pattern::Literal(Value::Array(arr)) = pattern {
            if arr.iter().any(|item| item == data) {
                return Some(Bindings::new());
            }
        }
        return None;
    }

    if let Some(arr) = data.as_array() {
        for item in arr {
            if let Some(bindings) = match_with_bindings(item, pattern) {
                return Some(bindings);
            }
        }
    }
    // Reverse case: data is a scalar, pattern is a Literal(Array).
    // Check if data appears in the array (supports `path in ["a", "b"]`).
    if let Pattern::Literal(Value::Array(arr)) = pattern {
        if arr.iter().any(|item| item == data) {
            return Some(Bindings::new());
        }
    }
    None
}

/// Check if ALL elements in an array match a pattern.
#[inline]
pub fn all_match(data: &Value, pattern: &Pattern) -> bool {
    if let Some(arr) = data.as_array() {
        arr.iter().all(|item| match_ref_inner(item, pattern))
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ========== Literal Matching ==========

    #[test]
    fn test_literal_integer_match() {
        assert!(matches(&json!(42), &Pattern::literal(42)));
    }

    #[test]
    fn test_literal_integer_no_match() {
        assert!(!matches(&json!(42), &Pattern::literal(99)));
    }

    #[test]
    fn test_literal_string_match() {
        assert!(matches(&json!("hello"), &Pattern::literal("hello")));
    }

    #[test]
    fn test_literal_bool_match() {
        assert!(matches(&json!(true), &Pattern::literal(true)));
        assert!(matches(&json!(false), &Pattern::literal(false)));
    }

    #[test]
    fn test_literal_null_match() {
        assert!(matches(&json!(null), &Pattern::Literal(Value::Null)));
    }

    #[test]
    fn test_literal_type_mismatch() {
        // String "42" should not match integer 42
        assert!(!matches(&json!("42"), &Pattern::literal(42)));
    }

    // ========== Wildcard Matching ==========

    #[test]
    fn test_wildcard_matches_anything() {
        assert!(matches(&json!(42), &Pattern::wildcard()));
        assert!(matches(&json!("hello"), &Pattern::wildcard()));
        assert!(matches(&json!(null), &Pattern::wildcard()));
        assert!(matches(&json!({"a": 1}), &Pattern::wildcard()));
        assert!(matches(&json!([1, 2, 3]), &Pattern::wildcard()));
    }

    #[test]
    fn test_named_variable_matches_anything() {
        assert!(matches(&json!(42), &Pattern::var("x")));
    }

    // ========== Object Matching (Duck Typing) ==========

    #[test]
    fn test_object_exact_match() {
        let pattern = Pattern::object([("name", Pattern::literal("test"))]);
        let data = json!({"name": "test"});
        assert!(matches(&data, &pattern));
    }

    #[test]
    fn test_object_partial_match_extra_fields_ignored() {
        // THE KILLER FEATURE: Extra fields don't cause failure
        let pattern = Pattern::object([("name", Pattern::literal("lhs"))]);
        let data = json!({
            "name": "lhs",
            "value": 10,
            "type": "int",
            "extra": "junk"
        });
        assert!(matches(&data, &pattern));
    }

    #[test]
    fn test_object_missing_required_field() {
        let pattern = Pattern::object([
            ("name", Pattern::literal("test")),
            ("required", Pattern::literal(true)),
        ]);
        let data = json!({"name": "test"}); // missing "required"
        assert!(!matches(&data, &pattern));
    }

    #[test]
    fn test_object_nested_match() {
        let pattern =
            Pattern::object([("outer", Pattern::object([("inner", Pattern::literal(42))]))]);
        let data = json!({
            "outer": {
                "inner": 42,
                "extra": "ignored"
            }
        });
        assert!(matches(&data, &pattern));
    }

    #[test]
    fn test_object_wrong_value() {
        let pattern = Pattern::object([("name", Pattern::literal("expected"))]);
        let data = json!({"name": "actual"});
        assert!(!matches(&data, &pattern));
    }

    #[test]
    fn test_object_pattern_against_non_object() {
        let pattern = Pattern::object([("name", Pattern::literal("test"))]);
        assert!(!matches(&json!(42), &pattern));
        assert!(!matches(&json!("string"), &pattern));
        assert!(!matches(&json!([1, 2, 3]), &pattern));
    }

    // ========== Array Matching ==========

    #[test]
    fn test_array_exact_match() {
        let pattern = Pattern::array(vec![Pattern::literal(1), Pattern::literal(2)]);
        let data = json!([1, 2]);
        assert!(matches(&data, &pattern));
    }

    #[test]
    fn test_array_length_mismatch() {
        let pattern = Pattern::array(vec![Pattern::literal(1), Pattern::literal(2)]);
        let data = json!([1, 2, 3]);
        assert!(!matches(&data, &pattern));
    }

    #[test]
    fn test_array_with_wildcards() {
        let pattern = Pattern::array(vec![
            Pattern::literal(1),
            Pattern::wildcard(),
            Pattern::literal(3),
        ]);
        let data = json!([1, "anything", 3]);
        assert!(matches(&data, &pattern));
    }

    // ========== Logical Operators ==========

    #[test]
    fn test_not_pattern() {
        let pattern = Pattern::literal(42).not();
        assert!(!matches(&json!(42), &pattern));
        assert!(matches(&json!(99), &pattern));
    }

    #[test]
    fn test_or_pattern() {
        let pattern = Pattern::or(vec![
            Pattern::literal(1),
            Pattern::literal(2),
            Pattern::literal(3),
        ]);
        assert!(matches(&json!(1), &pattern));
        assert!(matches(&json!(2), &pattern));
        assert!(matches(&json!(3), &pattern));
        assert!(!matches(&json!(4), &pattern));
    }

    #[test]
    fn test_and_pattern() {
        // Match objects that have BOTH "name" AND "value" fields
        let pattern = Pattern::and(vec![
            Pattern::object([("name", Pattern::wildcard())]),
            Pattern::object([("value", Pattern::wildcard())]),
        ]);
        assert!(matches(&json!({"name": "x", "value": 1}), &pattern));
        assert!(!matches(&json!({"name": "x"}), &pattern));
        assert!(!matches(&json!({"value": 1}), &pattern));
    }

    // ========== Collection Search (the `in` operator) ==========

    #[test]
    fn test_any_matches_finds_element() {
        let pattern = Pattern::object([
            ("name", Pattern::literal("lhs")),
            ("value", Pattern::literal(10)),
        ]);
        let data = json!([
            {"name": "junk", "value": 999},
            {"name": "lhs", "value": 10, "type": "int"},
            {"name": "rhs", "value": 5}
        ]);
        assert!(any_matches(&data, &pattern));
    }

    #[test]
    fn test_any_matches_no_match() {
        let pattern = Pattern::object([("name", Pattern::literal("nonexistent"))]);
        let data = json!([
            {"name": "a"},
            {"name": "b"},
            {"name": "c"}
        ]);
        assert!(!any_matches(&data, &pattern));
    }

    #[test]
    fn test_any_matches_empty_array() {
        let pattern = Pattern::wildcard();
        let data = json!([]);
        assert!(!any_matches(&data, &pattern));
    }

    #[test]
    fn test_any_matches_non_array() {
        let pattern = Pattern::wildcard();
        assert!(!any_matches(&json!(42), &pattern));
        assert!(!any_matches(&json!({"a": 1}), &pattern));
    }

    #[test]
    fn test_all_match_success() {
        let pattern = Pattern::object([("active", Pattern::literal(true))]);
        let data = json!([
            {"active": true, "name": "a"},
            {"active": true, "name": "b"}
        ]);
        assert!(all_match(&data, &pattern));
    }

    #[test]
    fn test_all_match_failure() {
        let pattern = Pattern::object([("active", Pattern::literal(true))]);
        let data = json!([
            {"active": true},
            {"active": false}
        ]);
        assert!(!all_match(&data, &pattern));
    }

    // ========== Complex Scenarios ==========

    #[test]
    fn test_readme_example() {
        // The exact scenario from the README
        let pattern = Pattern::object([
            ("name", Pattern::literal("lhs")),
            ("value", Pattern::literal(10)),
        ]);

        let named_arguments = json!([
            {"name": "random_junk", "value": 999},
            {"name": "lhs", "value": 10, "type": "int"},
            {"name": "rhs", "value": 5, "type": "int"}
        ]);

        assert!(any_matches(&named_arguments, &pattern));
    }

    #[test]
    fn test_deeply_nested_search() {
        // Search for a capability in a complex permission structure
        let pattern = Pattern::object([
            ("capability", Pattern::literal("write")),
            ("resource", Pattern::literal("/data/*")),
        ]);

        let permissions = json!({
            "user": "alice",
            "grants": [
                {"capability": "read", "resource": "/public/*"},
                {"capability": "write", "resource": "/data/*", "conditions": {}},
                {"capability": "admin", "resource": "/admin/*"}
            ]
        });

        // Navigate to grants and search
        let grants = permissions.get("grants").unwrap();
        assert!(any_matches(grants, &pattern));
    }

    // ========== Variable Bindings ==========

    #[test]
    fn test_binding_simple_variable() {
        let pattern = Pattern::var("x");
        let bindings = match_with_bindings(&json!(42), &pattern).unwrap();
        assert_eq!(bindings.get("x"), Some(&json!(42)));
    }

    #[test]
    fn test_binding_object_fields() {
        let pattern = Pattern::object([
            ("name", Pattern::var("captured_name")),
            ("value", Pattern::var("captured_value")),
        ]);
        let data = json!({"name": "alice", "value": 100, "extra": "ignored"});

        let bindings = match_with_bindings(&data, &pattern).unwrap();
        assert_eq!(bindings.get("captured_name"), Some(&json!("alice")));
        assert_eq!(bindings.get("captured_value"), Some(&json!(100)));
    }

    #[test]
    fn test_binding_nested() {
        let pattern = Pattern::object([
            (
                "user",
                Pattern::object([("name", Pattern::var("username"))]),
            ),
            ("role", Pattern::var("role")),
        ]);
        let data = json!({
            "user": {"name": "bob", "id": 123},
            "role": "admin"
        });

        let bindings = match_with_bindings(&data, &pattern).unwrap();
        assert_eq!(bindings.get("username"), Some(&json!("bob")));
        assert_eq!(bindings.get("role"), Some(&json!("admin")));
    }

    #[test]
    fn test_binding_in_array_search() {
        let pattern = Pattern::object([
            ("name", Pattern::literal("target")),
            ("data", Pattern::var("found_data")),
        ]);
        let arr = json!([
            {"name": "other", "data": "wrong"},
            {"name": "target", "data": "correct"},
            {"name": "another", "data": "also_wrong"}
        ]);

        let bindings = any_match_with_bindings(&arr, &pattern).unwrap();
        assert_eq!(bindings.get("found_data"), Some(&json!("correct")));
    }

    #[test]
    fn test_binding_wildcard_no_capture() {
        // Anonymous wildcards don't capture
        let pattern = Pattern::wildcard();
        let bindings = match_with_bindings(&json!(42), &pattern).unwrap();
        assert!(bindings.is_empty());
    }

    #[test]
    fn test_binding_or_captures_first_match() {
        let pattern = Pattern::or(vec![
            Pattern::object([("type", Pattern::literal("a")), ("val", Pattern::var("v"))]),
            Pattern::object([("type", Pattern::literal("b")), ("val", Pattern::var("v"))]),
        ]);
        let data = json!({"type": "b", "val": 42});

        let bindings = match_with_bindings(&data, &pattern).unwrap();
        assert_eq!(bindings.get("v"), Some(&json!(42)));
    }
}
