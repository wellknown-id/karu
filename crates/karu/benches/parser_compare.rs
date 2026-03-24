//! Parser comparison benchmarks: handrolled vs tree-sitter.
//!
//! Run with: `cargo bench --bench parser_compare --features dev`

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use karu::grammar::grammar::Program;
use karu::parser::Parser;
use rust_sitter::Language;

/// Simple single-rule policy
const SIMPLE: &str = r#"allow access if role == "admin";"#;

/// Complex multi-rule policy (generated at bench time)
fn complex_policy() -> String {
    let mut source = String::new();
    for i in 0..100 {
        source.push_str(&format!(
            r#"allow rule_{i} if field_{i} == {i} and active == true;"#
        ));
        source.push('\n');
    }
    source
}

/// Medium policy with various constructs
const MEDIUM: &str = r#"
    allow read_access if action == "read" and resource.public == true;
    deny delete_admin if action == "delete" and role != "admin";
    allow owner_edit if resource.ownerId == principal.id;
    deny blocked if principal.status == "blocked";
    allow api_access if action == "read" or action == "list";
"#;

fn bench_parse_simple(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_simple");

    group.bench_function("handrolled", |b| {
        b.iter(|| Parser::parse(black_box(SIMPLE)).unwrap())
    });

    group.bench_function("tree_sitter", |b| {
        b.iter(|| Program::parse(black_box(SIMPLE)).result.unwrap())
    });

    group.finish();
}

fn bench_parse_medium(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_medium");

    group.bench_function("handrolled", |b| {
        b.iter(|| Parser::parse(black_box(MEDIUM)).unwrap())
    });

    group.bench_function("tree_sitter", |b| {
        b.iter(|| Program::parse(black_box(MEDIUM)).result.unwrap())
    });

    group.finish();
}

fn bench_parse_100_rules(c: &mut Criterion) {
    let complex = complex_policy();
    let mut group = c.benchmark_group("parse_100_rules");

    group.bench_function("handrolled", |b| {
        b.iter(|| Parser::parse(black_box(&complex)).unwrap())
    });

    group.bench_function("tree_sitter", |b| {
        b.iter(|| Program::parse(black_box(&complex)).result.unwrap())
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_parse_simple,
    bench_parse_medium,
    bench_parse_100_rules,
);

criterion_main!(benches);
