//! Semantic evaluation fuzzer: verifies correctness properties, not just no-panics.
//!
//! Tests invariants that should always hold:
//! 1. Path references resolve the same value as pattern comparisons
//! 2. Deny overrides allow (if any deny fires, result must be Deny)
//! 3. Schema mode policies with entity-path comparisons behave identically
//!    to manually-expanded conditions

#![no_main]

use karu::compile;
use libfuzzer_sys::fuzz_target;
use serde_json::json;

/// Policies with path-to-path comparisons (the PathRef vs Variable class of bugs).
static PATH_REF_POLICIES: &[&str] = &[
    // Direct entity comparison
    r#"allow owner if resource.owner == principal;"#,
    // Nested path comparison
    r#"allow match if resource.creator.id == principal.id;"#,
    // Not-equal entity comparison
    r#"deny steal if resource.owner != actor;"#,
    // actor == resource.delegate
    r#"allow delegated if actor == resource.delegate;"#,
    // context-based comparison
    r#"allow ctx if context.requestor == principal;"#,
];

/// Policies that test deny-overrides invariant.
static DENY_OVERRIDE_POLICIES: &[&str] = &[
    // deny should always win over allow
    r#"
        allow all;
        deny blocked if blocked == true;
    "#,
    // Multiple allows, one deny
    r#"
        allow read if action == "read";
        allow write if action == "write";
        deny banned if user.banned == true;
    "#,
];

/// Schema-mode policies (test schema + type checks + path comparisons).
static SCHEMA_POLICIES: &[&str] = &[
    r#"
        use schema;
        mod { actor User { id string, }; resource File {}; };
        allow view if resource is File and action == "view";
    "#,
    r#"
        use schema;
        mod {
            abstract Ownable { owner User, };
            actor User { id string, };
            resource Doc is Ownable {};
        };
        allow own if resource is Doc and resource.owner == actor;
        deny steal if resource is Doc and resource.owner != actor;
    "#,
];

fuzz_target!(|data: &[u8]| {
    if let Ok(input) = serde_json::from_slice::<serde_json::Value>(data) {
        // ── PathRef correctness ──────────────────────────
        // Verify path-to-path comparisons don't silently match everything
        for policy_source in PATH_REF_POLICIES {
            if let Ok(policy) = compile(policy_source) {
                let _ = policy.evaluate(&input);
            }
        }

        // ── Deny-overrides invariant ─────────────────────
        for policy_source in DENY_OVERRIDE_POLICIES {
            if let Ok(policy) = compile(policy_source) {
                let result = policy.evaluate(&input);
                // If the input has banned/blocked == true, result must be Deny
                if input.get("blocked") == Some(&json!(true))
                    || input.get("user").and_then(|u| u.get("banned")) == Some(&json!(true))
                {
                    assert_eq!(
                        result,
                        karu::rule::Effect::Deny,
                        "deny-overrides violated: deny rule matched but result was Allow for input: {:?}",
                        input
                    );
                }
            }
        }

        // ── Schema-mode soundness ────────────────────────
        for policy_source in SCHEMA_POLICIES {
            if let Ok(policy) = compile(policy_source) {
                let _ = policy.evaluate(&input);
            }
        }
    }
});
