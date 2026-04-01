//! Policy vs IndexedPolicy equivalence fuzzer.
//!
//! IndexedPolicy pre-separates deny/allow rules for faster evaluation.
//! This target verifies it always produces identical results to the
//! standard Policy evaluation, and that batch evaluation matches serial.

#![no_main]

use karu::compile;
use karu::rule::IndexedPolicy;
use libfuzzer_sys::fuzz_target;

/// Known-good policies covering diverse patterns.
static POLICIES: &[&str] = &[
    r#"allow access if role == "admin";"#,
    r#"
        allow read if action == "read";
        deny blocked if user.status == "blocked";
    "#,
    r#"
        allow all;
        deny banned if banned == true;
    "#,
    r#"
        allow view if action == "view" and resource.public == true;
        allow owner if resource.owner == principal;
        deny steal if resource.owner != actor;
    "#,
    r#"
        allow admin if role == "admin" or role == "superuser";
        deny low if level < 3;
    "#,
    r#"allow deep if a.b.c.d == true;"#,
    r#"
        allow r1 if x == 1;
        allow r2 if x == 2;
        allow r3 if x == 3;
        deny d1 if y == true;
        deny d2 if z == true;
    "#,
];

fuzz_target!(|data: &[u8]| {
    // Interpret fuzz data as JSON
    let input = match serde_json::from_slice::<serde_json::Value>(data) {
        Ok(v) => v,
        Err(_) => return,
    };

    for policy_source in POLICIES {
        let policy = compile(policy_source).expect("Known-good policy should compile");
        let indexed = IndexedPolicy::from(policy.clone());

        // Single evaluation equivalence
        let a = policy.evaluate(&input);
        let b = indexed.evaluate(&input);
        assert_eq!(a, b,
            "Policy vs IndexedPolicy disagree!\nPolicy: {}\nInput: {:?}\nPolicy result: {:?}\nIndexed result: {:?}",
            policy_source, input, a, b
        );
    }

    // Batch equivalence: evaluate_batch must equal serial map
    if let Ok(inputs_data) = serde_json::from_slice::<Vec<serde_json::Value>>(data) {
        if inputs_data.len() <= 10 {
            for policy_source in POLICIES {
                let policy = compile(policy_source).expect("Known-good policy should compile");
                let batch = policy.evaluate_batch(&inputs_data);
                let serial: Vec<_> = inputs_data.iter().map(|i| policy.evaluate(i)).collect();
                assert_eq!(batch, serial,
                    "evaluate_batch disagrees with serial evaluation!\nPolicy: {}",
                    policy_source
                );
            }
        }
    }
});
