// SPDX-License-Identifier: MIT

//! Benchmarks `PathAst` path formatting by comparing the previous
//! `path_to_string` implementation with the current allocation-avoiding version.
//! Run with `cargo bench --bench lint_bench`.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use karu::ast::{PathAst, PathSegmentAst};

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

fn new_path_to_string(path: &PathAst) -> String {
    // New version avoiding clone
    use std::fmt::Write;
    let mut out = String::new();
    let mut first = true;
    for segment in &path.segments {
        if !first {
            out.push('.');
        }
        first = false;
        match segment {
            PathSegmentAst::Field(name) => out.push_str(name),
            PathSegmentAst::Index(idx) => write!(out, "[{}]", idx).unwrap(),
            PathSegmentAst::Variable(var) => write!(out, "[{}]", var).unwrap(),
        }
    }
    out
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
    group.bench_function("new", |b| b.iter(|| new_path_to_string(black_box(&path))));
    group.finish();
}

criterion_group!(benches, bench_path_to_string);
criterion_main!(benches);
