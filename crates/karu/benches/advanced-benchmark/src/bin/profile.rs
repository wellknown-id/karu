use karu::compile;
use serde_json::json;
use std::hint::black_box;
use std::time::Instant;

fn bench_fn(name: &str, iterations: u64, f: impl Fn()) -> f64 {
    // Warmup
    for _ in 0..1000 {
        f();
    }

    let start = Instant::now();
    for _ in 0..iterations {
        black_box(&f)();
    }
    let elapsed = start.elapsed();
    let ns_per_iter = elapsed.as_nanos() as f64 / iterations as f64;
    println!("{:50} {:>8.1} ns/iter", name, ns_per_iter);
    ns_per_iter
}

fn main() {
    let iters = 5_000_000u64;

    println!("=== CONDITION SCALING (how much does each condition add?) ===\n");

    // Pre-compile all, then bench
    let p1 = compile(r#"allow a if role == "admin";"#).unwrap();
    let p2 = compile(r#"allow a if role == "admin" and active == true;"#).unwrap();
    let p3 = compile(r#"allow a if role == "admin" and active == true and level >= 5;"#).unwrap();
    let p4 = compile(r#"allow a if role == "admin" and active == true and level >= 5 and department == "engineering";"#).unwrap();

    let input = json!({
        "role": "admin",
        "active": true,
        "level": 10,
        "department": "engineering"
    });

    let t1 = bench_fn("1 cond (precompiled)", iters, || {
        black_box(p1.evaluate(black_box(&input)));
    });
    let t2 = bench_fn("2 conds (precompiled)", iters, || {
        black_box(p2.evaluate(black_box(&input)));
    });
    let t3 = bench_fn("3 conds (precompiled)", iters, || {
        black_box(p3.evaluate(black_box(&input)));
    });
    let t4 = bench_fn("4 conds (precompiled)", iters, || {
        black_box(p4.evaluate(black_box(&input)));
    });

    println!("\nPer-condition cost:");
    println!("  1→2: {:.1} ns/condition", t2 - t1);
    println!("  2→3: {:.1} ns/condition", t3 - t2);
    println!("  3→4: {:.1} ns/condition", t4 - t3);
    println!("  Avg: {:.1} ns/condition", (t4 - t1) / 3.0);

    println!("\n=== EVAL BREAKDOWN (simple case) ===\n");

    // Measure individual components
    let path = karu::Path::parse("role");
    let val_admin = json!("admin");

    let t_path = bench_fn("1. path.resolve()", iters, || {
        black_box(path.resolve(black_box(&input)));
    });

    let t_cmp = bench_fn("2. Value == Value", iters, || {
        black_box(black_box(&input["role"]) == black_box(&val_admin));
    });

    let t_hashmap = bench_fn("3. HashMap::new() (empty)", iters, || {
        let m: std::collections::HashMap<String, &serde_json::Value> =
            std::collections::HashMap::new();
        black_box(m);
    });

    let t_eval = bench_fn("TOTAL: policy.evaluate()", iters, || {
        black_box(p1.evaluate(black_box(&input)));
    });

    println!("\nBreakdown:");
    println!(
        "  Path resolve:      {:>6.1} ns ({:.0}%)",
        t_path,
        t_path / t_eval * 100.0
    );
    println!(
        "  Value compare:     {:>6.1} ns ({:.0}%)",
        t_cmp,
        t_cmp / t_eval * 100.0
    );
    println!(
        "  HashMap overhead:  {:>6.1} ns ({:.0}%)",
        t_hashmap,
        t_hashmap / t_eval * 100.0
    );
    println!(
        "  Other overhead:    {:>6.1} ns ({:.0}%)",
        t_eval - t_path - t_cmp - t_hashmap,
        (t_eval - t_path - t_cmp - t_hashmap) / t_eval * 100.0
    );
    println!("  ─────────────────────────");
    println!("  Total:             {:>6.1} ns", t_eval);

    println!("\n=== RULE COUNT SCALING ===\n");

    for n in [1, 2, 5, 10, 20] {
        let src: String = (0..n)
            .map(|i| format!(r#"allow r{i} if f{i} == "v{i}";"#))
            .collect::<Vec<_>>()
            .join("\n");
        let policy = compile(&src).unwrap();
        let input_map: std::collections::HashMap<String, String> =
            (0..n).map(|i| (format!("f{i}"), format!("v{i}"))).collect();
        let inp = json!(input_map);

        bench_fn(
            &format!("{n} rules"),
            iters / std::cmp::max(1, n as u64 / 2),
            || {
                black_box(policy.evaluate(black_box(&inp)));
            },
        );
    }

    println!("\n=== PATH RESOLUTION COST PER SEGMENT ===\n");

    let depths: Vec<(String, serde_json::Value)> = (1..=8)
        .map(|depth| {
            let path_str: String = (0..depth)
                .map(|i| format!("s{i}"))
                .collect::<Vec<_>>()
                .join(".");
            let mut val = json!(true);
            for i in (0..depth).rev() {
                let key = format!("s{i}");
                val = json!({ key: val });
            }
            (path_str, val)
        })
        .collect();

    let mut prev = 0.0;
    for (path_str, val) in &depths {
        let p = karu::Path::parse(path_str);
        let t = bench_fn(&format!("resolve {} segments", p.len()), iters, || {
            black_box(p.resolve(black_box(val)));
        });
        if prev > 0.0 {
            print!("    (delta: {:.1} ns)", t - prev);
        }
        println!();
        prev = t;
    }
}
