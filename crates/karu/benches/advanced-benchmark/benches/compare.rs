//! Advanced benchmark: Karu vs Cedar — Native + WASM comparison.
//!
//! Uses Criterion for statistically rigorous benchmarking.
//! Native engines are called directly as Rust libraries.
//! WASM engines are loaded in-process via the wasmtime crate.
//!
//! Benchmark groups:
//!   - karu_native / cedar_native: direct library calls
//!   - karu_wasm / cedar_wasm: WASM via wasmtime (parse + eval each iteration)
//!   - karu_wasm_precompiled / cedar_wasm_precompiled: WASM with policy pre-compiled once
//!
//! Run with: `cargo bench` (from benches/advanced-benchmark/)

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use serde_json::json;
use std::path::Path;
use std::time::Duration;
use wasmtime::*;

// ── Scenario definitions ────────────────────────────────────────────────────

struct Scenario {
    name: &'static str,
    karu_policy: String,
    cedar_policy: String,
    karu_input: serde_json::Value,
    /// Input structured for Cedar-imported policies (paths like `principal.role`, `context.a.b.c`).
    cedar_input: serde_json::Value,
}

fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario {
            name: "simple",
            karu_policy: r#"allow access if role == "admin";"#.into(),
            cedar_policy:
                r#"permit(principal, action, resource) when { principal.role == "admin" };"#.into(),
            karu_input: json!({"role": "admin"}),
            cedar_input: json!({"principal": {"role": "admin"}}),
        },
        Scenario {
            name: "multi_condition",
            karu_policy: r#"allow access if 
                role == "admin" and 
                active == true and 
                level >= 5 and
                department == "engineering";"#
                .into(),
            cedar_policy: r#"permit(principal, action, resource) when { 
                principal.role == "admin" &&
                principal.active == true &&
                principal.level >= 5 &&
                principal.department == "engineering"
            };"#
            .into(),
            karu_input: json!({
                "role": "admin",
                "active": true,
                "level": 10,
                "department": "engineering"
            }),
            cedar_input: json!({
                "principal": {
                    "role": "admin",
                    "active": true,
                    "level": 10,
                    "department": "engineering"
                }
            }),
        },
        Scenario {
            name: "nested_path",
            karu_policy: r#"allow access if a.b.c.d.e.f == true;"#.into(),
            cedar_policy:
                r#"permit(principal, action, resource) when { context.a.b.c.d.e.f == true };"#
                    .into(),
            karu_input: json!({"a": {"b": {"c": {"d": {"e": {"f": true}}}}}}),
            cedar_input: json!({"context": {"a": {"b": {"c": {"d": {"e": {"f": true}}}}}}}),
        },
        {
            let karu_rules: String = (0..20)
                .map(|i| format!(r#"allow rule{i} if field{i} == "value{i}";"#))
                .collect::<Vec<_>>()
                .join("\n");
            let cedar_rules: String = (0..20)
                .map(|i| format!(r#"permit(principal, action == Action::"rule{i}", resource) when {{ context.field{i} == "value{i}" }};"#))
                .collect::<Vec<_>>()
                .join("\n");
            let karu_input: serde_json::Value = json!((0..20)
                .map(|i| (format!("field{i}"), format!("value{i}")))
                .collect::<std::collections::HashMap<String, String>>());
            let cedar_input: serde_json::Value = {
                let fields: std::collections::HashMap<String, String> = (0..20)
                    .map(|i| (format!("field{i}"), format!("value{i}")))
                    .collect();
                json!({"context": fields})
            };
            Scenario {
                name: "complex_20_rules",
                karu_policy: karu_rules,
                cedar_policy: cedar_rules,
                karu_input,
                cedar_input,
            }
        },
    ]
}

// ── Karu Native ─────────────────────────────────────────────────────────────

fn bench_karu_native(c: &mut Criterion) {
    let mut group = c.benchmark_group("karu_native");
    group.measurement_time(Duration::from_secs(5));

    for scenario in scenarios() {
        let compiled = karu::compile(&scenario.karu_policy).expect(&format!(
            "Failed to compile karu policy for {}",
            scenario.name
        ));
        let input = scenario.karu_input.clone();

        group.bench_with_input(BenchmarkId::new("eval", scenario.name), &(), |b, _| {
            b.iter(|| compiled.evaluate(black_box(&input)))
        });
    }
    group.finish();
}

// ── Cedar Native ────────────────────────────────────────────────────────────

fn bench_cedar_native(c: &mut Criterion) {
    use cedar_policy::*;

    let mut group = c.benchmark_group("cedar_native");
    group.measurement_time(Duration::from_secs(5));

    for scenario in scenarios() {
        let policy_set: PolicySet = scenario.cedar_policy.parse().expect(&format!(
            "Failed to parse cedar policy for {}",
            scenario.name
        ));
        let entities = Entities::empty();
        let authorizer = Authorizer::new();

        let action: EntityUid = r#"Action::"access""#.parse().unwrap();
        let principal: EntityUid = r#"User::"alice""#.parse().unwrap();
        let resource: EntityUid = r#"Resource::"res1""#.parse().unwrap();

        let context = if scenario.name == "nested_path" {
            Context::from_json_value(scenario.karu_input.clone(), None).unwrap()
        } else {
            Context::empty()
        };

        let request = Request::new(principal, action, resource, context, None).unwrap();

        group.bench_with_input(BenchmarkId::new("eval", scenario.name), &(), |b, _| {
            b.iter(|| {
                authorizer.is_authorized(
                    black_box(&request),
                    black_box(&policy_set),
                    black_box(&entities),
                )
            })
        });
    }
    group.finish();
}

// ── WASM helpers ────────────────────────────────────────────────────────────

/// Create an optimized wasmtime Engine.
///
/// Cranelift compiles the WASM to native code during Module::from_file,
/// so all benchmark iterations run JIT-compiled native code — not
/// interpreted bytecode.
fn optimized_engine() -> Engine {
    let mut config = Config::new();
    // Cranelift with max optimization (this is already the default,
    // but we make it explicit for clarity).
    config.cranelift_opt_level(OptLevel::Speed);
    // Disable NaN canonicalization for faster float ops
    config.cranelift_nan_canonicalization(false);
    Engine::new(&config).expect("Failed to create wasmtime engine")
}

/// A pre-loaded WASM module with alloc/free/eval exports.
struct WasmEngine {
    store: Store<wasmtime_wasi::p1::WasiP1Ctx>,
    instance: Instance,
    memory: Memory,
    alloc_fn: TypedFunc<i32, i32>,
    free_fn: TypedFunc<(i32, i32), ()>,
}

impl WasmEngine {
    fn load(wasm_path: &str) -> Option<Self> {
        let path = Path::new(wasm_path);
        if !path.exists() {
            eprintln!("⚠  WASM file not found: {} (skipping)", wasm_path);
            return None;
        }

        let engine = optimized_engine();
        // Module::from_file triggers Cranelift JIT compilation → native code
        let module = Module::from_file(&engine, path)
            .expect(&format!("Failed to compile WASM module: {}", wasm_path));

        let wasi_ctx = wasmtime_wasi::WasiCtxBuilder::new().build_p1();
        let mut store = Store::new(&engine, wasi_ctx);
        let mut linker = Linker::new(&engine);

        // Link WASI preview1 imports
        wasmtime_wasi::p1::add_to_linker_sync(&mut linker, |ctx| ctx).ok();

        let instance = linker
            .instantiate(&mut store, &module)
            .expect("Failed to instantiate WASM module");

        let memory = instance
            .get_memory(&mut store, "memory")
            .expect("WASM module must export 'memory'");

        let alloc_fn = instance
            .get_typed_func::<i32, i32>(&mut store, "karu_alloc")
            .or_else(|_| instance.get_typed_func::<i32, i32>(&mut store, "cedar_alloc"))
            .expect("WASM module must export an alloc function");

        let free_fn = instance
            .get_typed_func::<(i32, i32), ()>(&mut store, "karu_free")
            .or_else(|_| instance.get_typed_func::<(i32, i32), ()>(&mut store, "cedar_free"))
            .expect("WASM module must export a free function");

        Some(WasmEngine {
            store,
            instance,
            memory,
            alloc_fn,
            free_fn,
        })
    }

    /// Write a string into WASM linear memory, returning (ptr, len).
    fn write_string(&mut self, s: &str) -> (i32, i32) {
        let len = s.len() as i32;
        let ptr = self
            .alloc_fn
            .call(&mut self.store, len)
            .expect("alloc failed");
        self.memory
            .write(&mut self.store, ptr as usize, s.as_bytes())
            .expect("memory write failed");
        (ptr, len)
    }

    /// Free a previously allocated string.
    fn free_string(&mut self, ptr: i32, len: i32) {
        self.free_fn
            .call(&mut self.store, (ptr, len))
            .expect("free failed");
    }

    /// Get a typed function from the instance.
    fn get_func<P: WasmParams, R: WasmResults>(&mut self, name: &str) -> TypedFunc<P, R> {
        self.instance
            .get_typed_func::<P, R>(&mut self.store, name)
            .expect(&format!("WASM module must export '{}'", name))
    }

    /// Call the one-shot eval function (parse + eval each time).
    fn eval_once(
        &mut self,
        eval_fn: &TypedFunc<(i32, i32, i32, i32), i32>,
        policy: &str,
        input: &str,
    ) -> i32 {
        let (policy_ptr, policy_len) = self.write_string(policy);
        let (input_ptr, input_len) = self.write_string(input);

        let result = eval_fn
            .call(
                &mut self.store,
                (policy_ptr, policy_len, input_ptr, input_len),
            )
            .expect("eval call failed");

        self.free_string(policy_ptr, policy_len);
        self.free_string(input_ptr, input_len);

        result
    }

    /// Compile a policy once, returning a handle (pointer in WASM memory).
    fn compile_policy(&mut self, compile_fn: &TypedFunc<(i32, i32), i32>, policy: &str) -> i32 {
        let (policy_ptr, policy_len) = self.write_string(policy);
        let handle = compile_fn
            .call(&mut self.store, (policy_ptr, policy_len))
            .expect("compile call failed");
        self.free_string(policy_ptr, policy_len);
        assert!(handle != 0, "WASM policy compile returned null");
        handle
    }

    /// Evaluate a pre-compiled policy handle with input.
    fn eval_precompiled(
        &mut self,
        eval_fn: &TypedFunc<(i32, i32, i32), i32>,
        handle: i32,
        input: &str,
    ) -> i32 {
        let (input_ptr, input_len) = self.write_string(input);
        let result = eval_fn
            .call(&mut self.store, (handle, input_ptr, input_len))
            .expect("evaluate call failed");
        self.free_string(input_ptr, input_len);
        result
    }

    /// Free a compiled policy handle.
    fn free_policy(&mut self, free_fn: &TypedFunc<i32, ()>, handle: i32) {
        free_fn
            .call(&mut self.store, handle)
            .expect("policy_free failed");
    }
}

// ── Karu WASM (parse + eval each iteration) ─────────────────────────────────

fn bench_karu_wasm(c: &mut Criterion) {
    let wasm_path = "wasm/karu.wasm";
    let mut engine = match WasmEngine::load(wasm_path) {
        Some(e) => e,
        None => return,
    };

    let eval_fn = engine.get_func::<(i32, i32, i32, i32), i32>("karu_eval_once");

    // Smoke test
    let result = engine.eval_once(
        &eval_fn,
        r#"allow access if role == "admin";"#,
        r#"{"role": "admin"}"#,
    );
    assert_eq!(result, 1, "Karu WASM smoke test failed");

    let mut group = c.benchmark_group("karu_wasm");
    group.measurement_time(Duration::from_secs(5));

    for scenario in scenarios() {
        let policy = scenario.karu_policy.clone();
        let input = scenario.karu_input.to_string();

        group.bench_with_input(
            BenchmarkId::new("parse_and_eval", scenario.name),
            &(),
            |b, _| b.iter(|| engine.eval_once(&eval_fn, black_box(&policy), black_box(&input))),
        );
    }
    group.finish();
}

// ── Karu WASM pre-compiled (compile once, eval many) ────────────────────────

fn bench_karu_wasm_precompiled(c: &mut Criterion) {
    let wasm_path = "wasm/karu.wasm";
    let mut engine = match WasmEngine::load(wasm_path) {
        Some(e) => e,
        None => return,
    };

    let compile_fn = engine.get_func::<(i32, i32), i32>("karu_compile");
    let eval_fn = engine.get_func::<(i32, i32, i32), i32>("karu_evaluate");
    let policy_free_fn = engine.get_func::<i32, ()>("karu_policy_free");

    let mut group = c.benchmark_group("karu_wasm_precompiled");
    group.measurement_time(Duration::from_secs(5));

    for scenario in scenarios() {
        let input = scenario.karu_input.to_string();

        // Compile policy ONCE, outside the benchmark loop
        let handle = engine.compile_policy(&compile_fn, &scenario.karu_policy);

        group.bench_with_input(BenchmarkId::new("eval", scenario.name), &(), |b, _| {
            b.iter(|| engine.eval_precompiled(&eval_fn, handle, black_box(&input)))
        });

        engine.free_policy(&policy_free_fn, handle);
    }
    group.finish();
}

// ── Cedar WASM (parse + eval each iteration) ────────────────────────────────

fn bench_cedar_wasm(c: &mut Criterion) {
    let wasm_path = "wasm/cedar.wasm";
    let mut engine = match WasmEngine::load(wasm_path) {
        Some(e) => e,
        None => return,
    };

    let eval_fn = engine.get_func::<(i32, i32, i32, i32), i32>("cedar_eval");

    // Smoke test
    let result = engine.eval_once(
        &eval_fn,
        r#"permit(principal, action, resource) when { true };"#,
        "{}",
    );
    assert_eq!(result, 1, "Cedar WASM smoke test failed");

    let mut group = c.benchmark_group("cedar_wasm");
    group.measurement_time(Duration::from_secs(5));

    for scenario in scenarios() {
        let policy = scenario.cedar_policy.clone();
        let input = scenario.karu_input.to_string();

        group.bench_with_input(
            BenchmarkId::new("parse_and_eval", scenario.name),
            &(),
            |b, _| b.iter(|| engine.eval_once(&eval_fn, black_box(&policy), black_box(&input))),
        );
    }
    group.finish();
}

// ── Cedar WASM pre-compiled (compile once, eval many) ───────────────────────

fn bench_cedar_wasm_precompiled(c: &mut Criterion) {
    let wasm_path = "wasm/cedar.wasm";
    let mut engine = match WasmEngine::load(wasm_path) {
        Some(e) => e,
        None => return,
    };

    let compile_fn = engine.get_func::<(i32, i32), i32>("cedar_compile");
    let eval_fn = engine.get_func::<(i32, i32, i32), i32>("cedar_evaluate");
    let policy_free_fn = engine.get_func::<i32, ()>("cedar_policy_free");

    let mut group = c.benchmark_group("cedar_wasm_precompiled");
    group.measurement_time(Duration::from_secs(5));

    for scenario in scenarios() {
        let input = scenario.karu_input.to_string();

        // Compile policy ONCE, outside the benchmark loop
        let handle = engine.compile_policy(&compile_fn, &scenario.cedar_policy);

        group.bench_with_input(BenchmarkId::new("eval", scenario.name), &(), |b, _| {
            b.iter(|| engine.eval_precompiled(&eval_fn, handle, black_box(&input)))
        });

        engine.free_policy(&policy_free_fn, handle);
    }
    group.finish();
}

// ── Karu loading Cedar (native) ─────────────────────────────────────────────

fn bench_karu_cedar_native(c: &mut Criterion) {
    let mut group = c.benchmark_group("karu_cedar_native");
    group.measurement_time(Duration::from_secs(5));

    for scenario in scenarios() {
        let compiled = karu::compile_cedar(&scenario.cedar_policy).expect(&format!(
            "Failed to compile cedar policy via karu for {}",
            scenario.name
        ));
        let input = scenario.cedar_input.clone();

        group.bench_with_input(BenchmarkId::new("eval", scenario.name), &(), |b, _| {
            b.iter(|| compiled.evaluate(black_box(&input)))
        });
    }
    group.finish();
}

// ── Karu loading Cedar WASM (parse+eval each iteration) ─────────────────────

fn bench_karu_cedar_wasm(c: &mut Criterion) {
    let wasm_path = "wasm/karu.wasm";
    let mut engine = match WasmEngine::load(wasm_path) {
        Some(e) => e,
        None => return,
    };

    let eval_fn = engine.get_func::<(i32, i32, i32, i32), i32>("karu_eval_cedar_once");

    // Smoke test
    let result = engine.eval_once(
        &eval_fn,
        r#"permit(principal, action, resource) when { principal.role == "admin" };"#,
        r#"{"principal": {"role": "admin"}}"#,
    );
    assert_eq!(result, 1, "Karu Cedar WASM smoke test failed");

    let mut group = c.benchmark_group("karu_cedar_wasm");
    group.measurement_time(Duration::from_secs(5));

    for scenario in scenarios() {
        let policy = scenario.cedar_policy.clone();
        let input = scenario.cedar_input.to_string();

        group.bench_with_input(
            BenchmarkId::new("parse_and_eval", scenario.name),
            &(),
            |b, _| b.iter(|| engine.eval_once(&eval_fn, black_box(&policy), black_box(&input))),
        );
    }
    group.finish();
}

// ── Karu loading Cedar WASM pre-compiled (compile once, eval many) ──────────

fn bench_karu_cedar_wasm_precompiled(c: &mut Criterion) {
    let wasm_path = "wasm/karu.wasm";
    let mut engine = match WasmEngine::load(wasm_path) {
        Some(e) => e,
        None => return,
    };

    let compile_fn = engine.get_func::<(i32, i32), i32>("karu_compile_cedar");
    let eval_fn = engine.get_func::<(i32, i32, i32), i32>("karu_evaluate");
    let policy_free_fn = engine.get_func::<i32, ()>("karu_policy_free");

    let mut group = c.benchmark_group("karu_cedar_wasm_precompiled");
    group.measurement_time(Duration::from_secs(5));

    for scenario in scenarios() {
        let input = scenario.cedar_input.to_string();

        // Compile Cedar policy ONCE via karu_compile_cedar
        let handle = engine.compile_policy(&compile_fn, &scenario.cedar_policy);

        group.bench_with_input(BenchmarkId::new("eval", scenario.name), &(), |b, _| {
            b.iter(|| engine.eval_precompiled(&eval_fn, handle, black_box(&input)))
        });

        engine.free_policy(&policy_free_fn, handle);
    }
    group.finish();
}

// ── Criterion groups ────────────────────────────────────────────────────────

criterion_group!(
    benches,
    bench_karu_native,
    bench_karu_cedar_native,
    bench_cedar_native,
    bench_karu_wasm,
    bench_karu_wasm_precompiled,
    bench_karu_cedar_wasm,
    bench_karu_cedar_wasm_precompiled,
    bench_cedar_wasm,
    bench_cedar_wasm_precompiled,
);
criterion_main!(benches);
