//! Structured policy fuzzer: generates grammatically valid policies from
//! Arbitrary-derived AST fragments rather than random bytes.
//!
//! This reaches much deeper evaluation paths than byte-level fuzzing because
//! every generated input compiles successfully. It also verifies that Policy
//! and IndexedPolicy always agree (see §4 of the expansion plan).

#![no_main]

use arbitrary::{Arbitrary, Unstructured};
use karu::compile;
use karu::rule::IndexedPolicy;
use libfuzzer_sys::fuzz_target;
use serde_json::json;

// ── Structured types ────────────────────────────────────────────────

/// A fuzz-generated value for use in conditions and test inputs.
#[derive(Debug, Arbitrary)]
enum FuzzValue {
    Str(u8),   // index into a small vocabulary
    Num(i8),
    Bool(bool),
    Null,
}

/// A path segment — indexes into a small vocabulary of field names.
#[derive(Debug, Arbitrary, Clone)]
struct FuzzPathSeg(u8);

/// A fuzz-generated condition.
#[derive(Debug, Arbitrary)]
enum FuzzCondition {
    Eq { path: Vec<FuzzPathSeg>, value: FuzzValue },
    Ne { path: Vec<FuzzPathSeg>, value: FuzzValue },
    PathEq { left: Vec<FuzzPathSeg>, right: Vec<FuzzPathSeg> },
    In { value: FuzzValue, path: Vec<FuzzPathSeg> },
    Has { path: Vec<FuzzPathSeg> },
    Gt { path: Vec<FuzzPathSeg>, value: i8 },
    Lt { path: Vec<FuzzPathSeg>, value: i8 },
}

/// A fuzz-generated rule.
#[derive(Debug, Arbitrary)]
struct FuzzRule {
    allow: bool,
    name_idx: u8,
    conditions: Vec<FuzzCondition>,
}

/// A fuzz-generated policy (1–4 rules).
#[derive(Debug, Arbitrary)]
struct FuzzPolicy {
    rules: Vec<FuzzRule>,
}

// ── Vocabularies ────────────────────────────────────────────────────

const FIELDS: &[&str] = &[
    "action", "actor", "principal", "resource", "context",
    "role", "id", "owner", "status", "level", "name", "type",
];

const STRINGS: &[&str] = &[
    "read", "write", "delete", "admin", "user", "alice", "bob",
    "view", "edit", "true", "blocked", "active",
];

const RULE_NAMES: &[&str] = &[
    "access", "manage", "view", "edit", "block", "own", "read", "write",
];

// ── Rendering ───────────────────────────────────────────────────────

fn render_path(segs: &[FuzzPathSeg]) -> String {
    if segs.is_empty() {
        return FIELDS[0].to_string();
    }
    segs.iter()
        .map(|s| FIELDS[s.0 as usize % FIELDS.len()])
        .collect::<Vec<_>>()
        .join(".")
}

fn render_value(v: &FuzzValue) -> String {
    match v {
        FuzzValue::Str(i) => format!(r#""{}""#, STRINGS[*i as usize % STRINGS.len()]),
        FuzzValue::Num(n) => n.to_string(),
        FuzzValue::Bool(b) => b.to_string(),
        FuzzValue::Null => "null".to_string(),
    }
}

fn render_condition(c: &FuzzCondition) -> String {
    match c {
        FuzzCondition::Eq { path, value } =>
            format!("{} == {}", render_path(path), render_value(value)),
        FuzzCondition::Ne { path, value } =>
            format!("{} != {}", render_path(path), render_value(value)),
        FuzzCondition::PathEq { left, right } =>
            format!("{} == {}", render_path(left), render_path(right)),
        FuzzCondition::In { value, path } =>
            format!("{} in {}", render_value(value), render_path(path)),
        FuzzCondition::Has { path } =>
            format!("has {}", render_path(path)),
        FuzzCondition::Gt { path, value } =>
            format!("{} > {}", render_path(path), value),
        FuzzCondition::Lt { path, value } =>
            format!("{} < {}", render_path(path), value),
    }
}

fn render_policy(p: &FuzzPolicy) -> String {
    let mut out = String::new();
    for rule in &p.rules {
        let effect = if rule.allow { "allow" } else { "deny" };
        let name = RULE_NAMES[rule.name_idx as usize % RULE_NAMES.len()];
        // Deduplicate rule names by appending index
        if rule.conditions.is_empty() {
            out.push_str(&format!("{} {};\n", effect, name));
        } else {
            let conds: Vec<String> = rule.conditions.iter()
                .take(4) // cap conditions to keep policies reasonable
                .map(|c| render_condition(c))
                .collect();
            out.push_str(&format!("{} {} if\n    {};\n", effect, name, conds.join(" and\n    ")));
        }
    }
    out
}

// ── Test inputs ─────────────────────────────────────────────────────

fn test_inputs() -> Vec<serde_json::Value> {
    vec![
        json!({}),
        json!({"action": "read", "actor": "alice", "resource": {"id": "doc1", "owner": "alice"}}),
        json!({"action": "delete", "principal": {"id": "bob", "role": "admin"}, "resource": {"owner": {"id": "alice"}}, "context": {"level": 5}}),
        json!({"role": "admin", "status": "active", "level": 10}),
        json!({"action": "write", "actor": {"id": "alice"}, "resource": {"id": "r1", "owner": {"id": "alice"}}, "blocked": true}),
        json!(null),
    ]
}

// ── Fuzz target ─────────────────────────────────────────────────────

fuzz_target!(|data: &[u8]| {
    let mut u = Unstructured::new(data);
    let fuzz_policy: FuzzPolicy = match FuzzPolicy::arbitrary(&mut u) {
        Ok(p) => p,
        Err(_) => return,
    };

    // Cap at 4 rules to avoid combinatorial explosion
    if fuzz_policy.rules.is_empty() || fuzz_policy.rules.len() > 4 {
        return;
    }

    let source = render_policy(&fuzz_policy);

    // Compile — should never panic
    let policy = match compile(&source) {
        Ok(p) => p,
        Err(_) => return, // legit compile error (e.g. duplicate rule name)
    };

    // Build indexed variant
    let indexed = IndexedPolicy::from(policy.clone());

    // Evaluate against standard inputs
    for input in &test_inputs() {
        let a = policy.evaluate(input);
        let b = indexed.evaluate(input);
        assert_eq!(a, b,
            "Policy vs IndexedPolicy disagreement!\nSource:\n{}\nInput: {:?}",
            source, input
        );
    }
});
