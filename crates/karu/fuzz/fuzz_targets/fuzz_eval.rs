//! Fuzz target for policy evaluation with arbitrary JSON inputs.
//!
//! Unlike fuzz_compile (which tests compile with random policies),
//! this target compiles known-good policies and fuzzes the evaluation
//! path with arbitrary JSON data, testing the resolver, matcher,
//! and condition evaluation with unexpected input shapes.

#![no_main]

use karu::compile;
use libfuzzer_sys::fuzz_target;

/// Pre-compiled policies covering diverse evaluation paths.
static POLICIES: &[&str] = &[
    // Simple equality
    r#"allow access if role == "admin";"#,
    // Multi-condition
    r#"allow access if role == "admin" and active == true and level >= 5;"#,
    // Nested path
    r#"allow access if a.b.c.d.e.f == true;"#,
    // Collection search (in operator)
    r#"allow access if {name: "target"} in items;"#,
    // Or conditions
    r#"allow access if role == "admin" or role == "superuser";"#,
    // Inequality
    r#"deny access if trust_score < 50;"#,
    // Deep nesting with multiple operators
    r#"allow access if user.profile.verified == true and user.age >= 18;"#,
    // Pattern with string comparison
    r#"allow access if action != "delete";"#,
    // Multiple rules (deny-override)
    r#"
        allow read if action == "read";
        deny blocked if user.status == "blocked";
    "#,
];

fuzz_target!(|data: &[u8]| {
    // Try to interpret fuzz input as JSON
    if let Ok(input) = serde_json::from_slice::<serde_json::Value>(data) {
        for policy_source in POLICIES {
            let policy = compile(policy_source).expect("Known-good policy should compile");
            // Evaluation should never panic regardless of input shape
            let _ = policy.evaluate(&input);
        }
    }
});
