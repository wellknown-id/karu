// SPDX-License-Identifier: MIT

//! Karu performance benchmarks.
//!
//! Run with: `cargo bench`

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use karu::compile;
use serde_json::json;

/// Benchmark parsing a simple single-rule policy
fn bench_parse_simple(c: &mut Criterion) {
    let source = r#"allow access if role == "admin";"#;

    c.bench_function("parse_simple", |b| {
        b.iter(|| compile(black_box(source)).unwrap())
    });
}

/// Benchmark parsing a complex multi-rule policy
fn bench_parse_complex(c: &mut Criterion) {
    // Generate a policy with 100 rules
    let mut source = String::new();
    for i in 0..100 {
        source.push_str(&format!(
            r#"allow rule_{i} if field_{i} == {i} and active == true;"#
        ));
        source.push('\n');
    }

    c.bench_function("parse_100_rules", |b| {
        b.iter(|| compile(black_box(&source)).unwrap())
    });
}

/// Benchmark evaluating a simple equality check
fn bench_eval_simple(c: &mut Criterion) {
    let policy = compile(r#"allow access if role == "admin";"#).unwrap();
    let request = json!({"role": "admin"});

    c.bench_function("eval_simple", |b| {
        b.iter(|| policy.evaluate(black_box(&request)))
    });
}

/// Benchmark collection search (the `in` operator)
fn bench_eval_collection_search(c: &mut Criterion) {
    let policy = compile(r#"allow access if {name: "target"} in items;"#).unwrap();

    // Create arrays of varying sizes
    let mut group = c.benchmark_group("collection_search");

    for size in [10, 100, 1000].iter() {
        let items: Vec<_> = (0..*size)
            .map(|i| json!({"name": format!("item_{}", i), "value": i}))
            .collect();
        // Put target at end to measure worst case
        let mut items_with_target = items.clone();
        items_with_target.push(json!({"name": "target", "value": 999}));

        let request = json!({"items": items_with_target});

        group.bench_with_input(BenchmarkId::new("size", size), size, |b, _| {
            b.iter(|| policy.evaluate(black_box(&request)))
        });
    }

    group.finish();
}

/// Benchmark deeply nested path resolution
fn bench_eval_nested_path(c: &mut Criterion) {
    let policy = compile(r#"allow access if a.b.c.d.e.f == true;"#).unwrap();
    let request = json!({
        "a": {"b": {"c": {"d": {"e": {"f": true}}}}}
    });

    c.bench_function("eval_nested_path", |b| {
        b.iter(|| policy.evaluate(black_box(&request)))
    });
}

/// Benchmark multiple condition evaluation
fn bench_eval_multi_condition(c: &mut Criterion) {
    let policy = compile(
        r#"
        allow access if 
            role == "admin" and 
            active == true and 
            level >= 5 and
            department == "engineering";
    "#,
    )
    .unwrap();

    let request = json!({
        "role": "admin",
        "active": true,
        "level": 10,
        "department": "engineering"
    });

    c.bench_function("eval_multi_condition", |b| {
        b.iter(|| policy.evaluate(black_box(&request)))
    });
}

/// Benchmark throughput: evaluations per second
fn bench_throughput(c: &mut Criterion) {
    let policy = compile(r#"allow access if role == "admin";"#).unwrap();
    let requests: Vec<_> = (0..1000)
        .map(|i| json!({"role": if i % 2 == 0 { "admin" } else { "user" }, "id": i}))
        .collect();

    c.bench_function("throughput_1000", |b| {
        b.iter(|| {
            for req in &requests {
                let _ = policy.evaluate(black_box(req));
            }
        })
    });
}

criterion_group!(
    benches,
    bench_parse_simple,
    bench_parse_complex,
    bench_eval_simple,
    bench_eval_collection_search,
    bench_eval_nested_path,
    bench_eval_multi_condition,
    bench_throughput,
);

criterion_main!(benches);
