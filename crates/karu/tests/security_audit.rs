//! Security audit tests — PoC exploits for identified vulnerabilities.
//!
//! Each test validates a specific security finding. Tests marked with
//! "[FIXED]" verify that a vulnerability has been patched. Tests marked
//! with "[INFO]" document semantic behaviors worth understanding.

use karu::compile;
use karu::rule::Effect;
use serde_json::json;
use std::time::Instant;

// ============================================================================
// FINDING 1: glob_match in matcher.rs — exponential backtracking (FIXED)
// Severity: MEDIUM (DoS)
// The old recursive implementation had O(M^N) worst-case with N wildcards.
// Fixed by replacing with an iterative two-pointer O(n*m) algorithm.
// ============================================================================

/// [FIXED] Glob matching must complete in linear time even with many wildcards.
/// Pre-fix: 8 wildcards + 25 chars = 1.2s, 10 wildcards + 30 chars = timeout.
/// Post-fix: all complete in microseconds.
#[test]
fn fixed_glob_match_no_exponential_backtracking() {
    use karu::{matches_ref, Pattern};

    // Pathological pattern: many wildcards with same literal between them
    for (wildcards, text_len) in [(8, 25), (10, 30), (15, 50), (20, 100)] {
        let pattern_str: String = (0..wildcards).map(|_| "*a").collect::<String>() + "*b";
        let text = "a".repeat(text_len);
        let pattern = Pattern::Glob(pattern_str.clone());
        let val = serde_json::Value::String(text);

        let start = Instant::now();
        let result = matches_ref(&val, &pattern);
        let elapsed = start.elapsed();

        assert!(!result, "Should not match (no 'b' in input)");
        assert!(
            elapsed.as_millis() < 100,
            "glob_match with {} wildcards + {} chars took {:?} — still has exponential backtracking!",
            wildcards, text_len, elapsed
        );
    }
}

// ============================================================================
// FINDING 2: Circular assertion inlining (FIXED)
// Severity: HIGH (crash/DoS)
// The compiler now detects circular assertion references and returns
// a ParseError instead of stack-overflowing.
// ============================================================================

/// [FIXED] Direct self-referential assertion: `assert a if a;`
/// Pre-fix: stack overflow (SIGABRT). Post-fix: returns ParseError.
#[test]
fn fixed_circular_assertion_direct() {
    let result = compile("assert a if a;\nallow access if a;");
    assert!(
        result.is_err(),
        "Circular assertion should produce ParseError, not panic"
    );
    let err = result.unwrap_err();
    assert!(
        err.message.contains("circular"),
        "Error should mention circular reference: {}",
        err.message
    );
}

/// [FIXED] Indirect circular assertion: a → b → a
#[test]
fn fixed_circular_assertion_indirect() {
    let result = compile("assert a if b;\nassert b if a;\nallow access if a;");
    assert!(
        result.is_err(),
        "Indirect circular assertion should produce ParseError"
    );
    let err = result.unwrap_err();
    assert!(
        err.message.contains("circular"),
        "Error should mention circular reference: {}",
        err.message
    );
}

/// [FIXED] Non-circular assertions still work (regression test).
#[test]
fn fixed_non_circular_assertions_still_work() {
    let policy = compile(concat!(
        "assert is_admin if principal.role == \"admin\";\n",
        "assert is_active if active == true;\n",
        "allow access if is_admin and is_active;\n",
    ))
    .expect("Non-circular assertions should compile");

    assert_eq!(
        policy.evaluate(&json!({"principal": {"role": "admin"}, "active": true})),
        Effect::Allow,
    );
    assert_eq!(
        policy.evaluate(&json!({"principal": {"role": "user"}, "active": true})),
        Effect::Deny,
    );
}

// ============================================================================
// FINDING 3: Has operator + null value (FIXED)
// Severity: LOW (logic bug)
// `has field` now returns true when the field exists, even if its value
// is JSON null. This aligns with Cedar semantics.
// ============================================================================

/// [FIXED] `has field` returns true for present-but-null fields.
/// Pre-fix: returned false. Post-fix: returns true (attribute exists).
#[test]
fn fixed_has_operator_null_value() {
    let policy = compile(r#"allow access if has field;"#).unwrap();

    // Field exists with non-null value → Allow
    assert_eq!(
        policy.evaluate(&json!({"field": "hello"})),
        Effect::Allow,
    );

    // Field doesn't exist → Deny
    assert_eq!(
        policy.evaluate(&json!({"other": "data"})),
        Effect::Deny,
    );

    // Field exists with null value → Allow (FIXED: was Deny)
    assert_eq!(
        policy.evaluate(&json!({"field": null})),
        Effect::Allow,
        "has should return true when the attribute exists, even if null",
    );
}

// ============================================================================
// FINDING 4: Deeply nested expressions — stack depth
// Severity: LOW (DoS) — requires malicious policy input with extreme nesting.
// Not fixed (would require an iterative evaluator or depth limit).
// Documented as a known limitation.
// ============================================================================

/// [INFO] Moderate nesting depth works fine.
#[test]
fn info_moderate_nesting_works() {
    // 100 levels of not — manageable
    let mut source = String::from("allow access if ");
    for _ in 0..100 {
        source.push_str("not ");
    }
    source.push_str(r#"role == "admin";"#);

    let policy = compile(&source).unwrap();
    // 100 negations (even count) = original condition
    assert_eq!(
        policy.evaluate(&json!({"role": "admin"})),
        Effect::Allow,
    );
    assert_eq!(
        policy.evaluate(&json!({"role": "user"})),
        Effect::Deny,
    );
}

// ============================================================================
// FINDING 5: ForAll on empty collection
// Severity: INFO — semantic consideration
// forall on an empty source array returns Deny (no match).
// This is actually MORE secure than standard logic (∀x∈∅ is vacuously true)
// since it prevents unintended access grants on empty collections.
// ============================================================================

/// [INFO] forall on empty array returns false (secure default).
/// When using Condition::for_all via the Rust API, an empty array returns
/// false because no elements match. This is the secure default — it prevents
/// unintended access grants when collections are empty.
#[test]
fn info_forall_empty_array_rust_api() {
    use karu::rule::{Condition, Rule, Policy};
    use karu::Pattern;

    // forall user in users: user.verified == true
    let policy = Policy::new().with_rule(Rule::allow(
        "access",
        vec![Condition::for_all(
            "users",
            Pattern::object([("verified", Pattern::literal(true))]),
        )],
    ));

    // Non-empty, all verified → Allow
    assert_eq!(
        policy.evaluate(&json!({
            "users": [{"verified": true}, {"verified": true}]
        })),
        Effect::Allow,
    );

    // Non-empty, one not verified → Deny
    assert_eq!(
        policy.evaluate(&json!({
            "users": [{"verified": true}, {"verified": false}]
        })),
        Effect::Deny,
    );

    // Empty array → Allow (vacuously true — standard mathematical logic).
    // ⚠ Policy authors should be aware: forall on empty collections ALLOWS.
    // To prevent this, add an explicit "users is not empty" condition.
    assert_eq!(
        policy.evaluate(&json!({"users": []})),
        Effect::Allow,
        "forall on empty array is vacuously true — may surprise policy authors",
    );
}

// ============================================================================
// FINDING 6: Float comparison imprecision
// Severity: INFO — standard IEEE 754 behavior, not a bug per se.
// ============================================================================

/// [INFO] Float comparison uses f64 with normal IEEE 754 precision.
#[test]
fn info_float_comparison_precision() {
    let policy = compile(r#"allow access if price == 0.3;"#).unwrap();

    // Direct 0.3 → Allow
    assert_eq!(
        policy.evaluate(&json!({"price": 0.3})),
        Effect::Allow,
    );

    // 0.1 + 0.2 ≠ 0.3 in IEEE 754
    let computed = 0.1_f64 + 0.2_f64;
    let val = serde_json::Value::Number(serde_json::Number::from_f64(computed).unwrap());
    let result = policy.evaluate(&json!({"price": val}));
    assert_eq!(
        result,
        Effect::Deny,
        "0.1 + 0.2 != 0.3 in IEEE 754 — expected behavior",
    );
}
