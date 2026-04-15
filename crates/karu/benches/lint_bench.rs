// SPDX-License-Identifier: MIT
//! Benchmarks `PathAst` path formatting by comparing the previous
//! `path_to_string` implementation with the current allocation-avoiding version.
//! Run with `cargo bench --bench lint_bench`.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use karu::ast::{PathAst, PathSegmentAst};
use karu::lint::path_to_string_for_bench;

fn old_path_to_string(path: &PathAst) -> String {
    path.segments
        .iter()
        .map(|s| match s {
            PathSegmentAst::Field(name) => name.clone(),
            PathSegmentAst::Index(idx) => format!("[{}]", idx),
            PathSegmentAst::Variable(var) => format!("[{}]", var),
        })
        .collect::<Vec<_>>()
        .join(".")
}

fn bench_path_to_string(c: &mut Criterion) {
    let path = PathAst {
        segments: vec![
            PathSegmentAst::Field("user".to_string()),
            PathSegmentAst::Field("profile".to_string()),
            PathSegmentAst::Index(42),
            PathSegmentAst::Field("metadata".to_string()),
            PathSegmentAst::Variable("key".to_string()),
        ],
    };

    let mut group = c.benchmark_group("path_to_string");
    group.bench_function("old", |b| b.iter(|| old_path_to_string(black_box(&path))));
    group.bench_function("new", |b| {
        b.iter(|| path_to_string_for_bench(black_box(&path)))
    });
    group.finish();
}

criterion_group!(benches, bench_path_to_string);
criterion_main!(benches);
