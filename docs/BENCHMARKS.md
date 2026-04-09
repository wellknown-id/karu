# Karu Performance Benchmarks

Karu was created to replace Cedar in [kodus](https://github.com/wellknown-id/kodus) - and not because Cedar was slow, but because we realised [along the way](https://en.wikipedia.org/wiki/Coastline_paradox) it wasn't expressive enough for the use cases we planned for. We needed a replacement that could give us something like [Polar's approachableness](https://www.osohq.com/docs/reference/polar/introduction) and keep Cedar's performance characteristics. We had a choice, fork from the archived remnants of Polar or be inspired by it. We took a punt and chose the latter. We didn't expect to beat Cedar in every single performance test we tried. So we attached a `strict mode` to Karu to play fairly. When we were still faster, we added native Cedar support.

_TLDR; Karu is faster at Cedar than Cedar is at Cedar._

## Summary

| Metric                                   | Karu     | Cedar      | Winner                |
| ---------------------------------------- | -------- | ---------- | --------------------- |
| **WASM Bundle**                          | 319 KB   | 1.8 MB     | **Karu 5.6x smaller** |
| **Native eval**                          | 15.4 ns  | 1,108 ns   | **Karu 72x faster**   |
| **WASM eval (precompiled)**              | 465 ns   | 88,350 ns  | **Karu 190x faster**  |
| **WASM eval (parse+eval)**               | 3,485 ns | 192,150 ns | **Karu 55x faster**   |
| **Complex (20 rules, native)**           | 663 ns   | 14,137 ns  | **Karu 21x faster**   |
| **Complex (20 rules, WASM precompiled)** | 7,543 ns | 205,410 ns | **Karu 27x faster**   |

## Methodology

All benchmarks use [Criterion.rs](https://github.com/bheisler/criterion.rs) for
statistically rigorous measurement (100 samples, 5-second measurement window).

- **Native**: Direct Rust library calls (`karu::compile` / `cedar_policy::Authorizer`)
- **WASM**: In-process via [wasmtime](https://wasmtime.dev/) with Cranelift JIT (`OptLevel::Speed`)
  - the WASM is compiled to native machine code _before_ benchmarking starts
- **Precompiled**: Policy is compiled once; only evaluation is benchmarked
- **Parse+eval**: Policy is parsed and evaluated every iteration

```bash
# Core Criterion benches from crates/karu/benches/
cargo bench -p karu --features dev --bench evaluation --bench parser_compare

# Advanced Rust + WASM Criterion benches
cd crates/karu/benches/advanced-benchmark
./build.sh
cargo run --release --bin seed
cargo bench

# Node/WASM benches
cargo install wasm-pack --locked
cd /path/to/karu/crates/karu
wasm-pack build . --target nodejs --no-default-features --features wasm,cedar --out-dir benches/wasm_bench/pkg
cd benches/wasm_bench
npm ci
node bench_node.mjs
node bench_cedar.mjs
```

On pull requests, `.github/workflows/benchmarks.yml` runs the same benchmark suites
against both the PR head and the PR base commit, then fails the workflow if any
measured benchmark regresses by more than the configured threshold.

### Scenarios

| Scenario            | Description                                              |
| ------------------- | -------------------------------------------------------- |
| **Simple**          | 1 rule, 1 condition: `allow access if role == "admin"`   |
| **Multi-condition** | 1 rule, 4 conditions: role + active + level + department |
| **Nested path**     | 1 rule, 6-deep JSON traversal: `a.b.c.d.e.f == true`     |
| **Complex**         | 20 separate rules, each with 1 condition, all matching   |

## Native Benchmarks (Rust library calls)

| Scenario               | Karu        | Cedar      | Speedup |
| ---------------------- | ----------- | ---------- | ------- |
| Simple (1 condition)   | **15.4 ns** | 1,108 ns   | **72x** |
| Multi-condition (4)    | **115 ns**  | 1,222 ns   | **11x** |
| Nested path (6 levels) | **37.6 ns** | 1,606 ns   | **43x** |
| Complex (20 rules)     | **663 ns**  | 14,137 ns  | **21x** |

## Karu Loading Cedar Policies (Native)

Karu can import Cedar syntax and evaluate it without any performance penalty.
Policies are converted from Cedar → Karu via `compile_cedar()`.

| Scenario           | Karu (native) | Karu (Cedar import) | Cedar (native) |
| ------------------ | ------------- | ------------------- | -------------- |
| Simple             | 15.4 ns       | **21.2 ns**         | 1,108 ns       |
| Multi-condition    | 115 ns        | **140 ns**          | 1,222 ns       |
| Nested path        | 37.6 ns       | **43.4 ns**         | 1,606 ns       |
| Complex (20 rules) | 663 ns        | **195 ns**          | 14,137 ns      |

> **Karu loading Cedar policies is 8–73x faster than Cedar native**

## WASM Benchmarks - Precompiled (eval-only)

Policy compiled once, only evaluation is timed.

| Scenario           | Karu         | Karu (Cedar import) | Cedar       | Speedup vs Cedar |
| ------------------ | ------------ | ------------------- | ----------- | ---------------- |
| Simple             | **465 ns**   | 631 ns              | 88,350 ns   | **190x**         |
| Multi-condition    | **1,260 ns** | 1,477 ns            | 99,264 ns   | **79x**          |
| Nested path        | **1,233 ns** | 1,403 ns            | 125,420 ns  | **102x**         |
| Complex (20 rules) | **7,543 ns** | 6,497 ns            | 205,410 ns  | **27x**          |

> Karu precompiled WASM (465 ns) is **faster than Cedar native** (1,108 ns)!
> Karu loading Cedar via WASM (631 ns) is **still faster than Cedar native**!

## WASM Benchmarks - Parse + Eval

Policy parsed and evaluated every iteration.

| Scenario           | Karu          | Karu (Cedar import) | Cedar       | Speedup vs Cedar |
| ------------------ | ------------- | ------------------- | ----------- | ---------------- |
| Simple             | **3,485 ns**  | 5,492 ns            | 192,150 ns  | **55x**          |
| Multi-condition    | **9,733 ns**  | 13,827 ns           | 254,120 ns  | **26x**          |
| Nested path        | **6,206 ns**  | 7,988 ns            | 247,220 ns  | **40x**          |
| Complex (20 rules) | **56,498 ns** | 115,710 ns          | 713,360 ns  | **13x**          |

## Native vs WASM Overhead

| Scenario        | Native    | WASM Precompiled | Overhead |
| --------------- | --------- | ---------------- | -------- |
| Simple (Karu)   | 15.4 ns   | 465 ns           | 30x      |
| Simple (Cedar)  | 1,108 ns  | 88,350 ns        | 80x      |
| Complex (Karu)  | 663 ns    | 7,543 ns         | 11x      |
| Complex (Cedar) | 14,137 ns | 205,410 ns       | 15x      |

Karu's WASM overhead is **11–30x** (vs Cedar's **15–80x**), meaning
Karu translates better to the WASM runtime.

## Bundle Size

| Engine   | WASM Size  |
| -------- | ---------- |
| **Karu** | **319 KB** |
| Cedar    | 1.8 MB     |

## Scale Benchmark: Realistic Authorization Workload

Benchmarked against a seeded SQLite database with **10,000 users**, **1.5M files**,
and **1M share records**. Queries are generated deterministically from real DB data,
covering read (public, own, shared, org, disabled) and delete (own, admin) patterns.

### Data Setup

```bash
cd benches/advanced-benchmark
cargo run --release --bin seed       # 10K users, ~3s
cargo run --release --bin seed -- --full   # 1M users, ~5 min
```

### Read Authorization (5 rules, ~1000 diverse queries)

| Threads | Karu ops/sec | Cedar ops/sec | Speedup |
| ------- | ------------ | ------------- | ------- |
| **1**   | **2.00 M**   | 82 K          | **24x** |
| **2**   | **3.99 M**   | 144 K         | **28x** |
| **4**   | **6.90 M**   | 193 K         | **36x** |
| **8**   | **11.00 M**  | 248 K         | **44x** |
| **16**  | **14.32 M**  | 273 K         | **52x** |

### Delete Authorization (2 rules, ~400 diverse queries)

| Threads | Karu ops/sec | Cedar ops/sec | Speedup |
| ------- | ------------ | ------------- | ------- |
| **1**   | **2.87 M**   | 129 K         | **22x** |
| **2**   | **5.35 M**   | 235 K         | **23x** |
| **4**   | **8.55 M**   | 360 K         | **24x** |
| **8**   | **10.78 M**  | 541 K         | **20x** |
| **16**  | **10.86 M**  | 591 K         | **18x** |

### Thread Scaling Efficiency

| Threads | Karu (read) | Cedar (read) |
| ------- | ----------- | ------------ |
| 1→2     | **2.00x**   | 1.76x        |
| 1→4     | **3.45x**   | 2.35x        |
| 1→8     | **5.50x**   | 3.02x        |
| 1→16    | **7.16x**   | 3.33x        |

> At 16 threads, Karu evaluates **14.32 million authorization decisions per second**.
> Cedar maxes out at **273K** - a **52x difference**.
> With 10,000 concurrent users hitting a single node, **Karu handles all evaluations in 0.7ms**.

## Karu Standalone Benchmarks

Additional microbenchmarks from the core crate:

### Evaluation

| Benchmark                    | Time     |
| ---------------------------- | -------- |
| eval_simple                  | 15.95 ns |
| eval_multi_condition (4)     | 95.67 ns |
| eval_nested_path (6 levels)  | 39.10 ns |
| collection_search (10 items) | 182.8 ns |
| collection_search (100)      | 1.270 µs |
| collection_search (1000)     | 12.07 µs |
| throughput_1000              | 19.11 µs |
| parse_simple                 | 928 ns   |
| parse_100_rules              | 154.1 µs |

### Parser Comparison: Handrolled vs Tree-sitter

| Scenario  | Handrolled | Tree-sitter | Speedup |
| --------- | ---------- | ----------- | ------- |
| Simple    | 757 ns     | 8.78 µs     | **12x** |
| Medium    | 5.54 µs    | 65.5 µs     | **12x** |
| 100 rules | 111 µs     | 1.29 ms     | **12x** |

## Optimization Notes

The evaluation hot path has been extensively profiled and optimized:

- **`evaluate_fast()`** - bypasses HashMap/bindings for non-quantified conditions
- **Literal fast-path** - `Eq`/`Ne` with literal patterns does direct `Value` comparison
- **`resolve_fast()`** - path resolution without HashMap allocation
- **`matches_ref()`** - pattern matching without Bindings struct allocation
- **Zero-clone evaluation** - patterns are used by reference, never cloned during eval

Per-condition cost is ~26 ns (path resolve + value compare + dispatch).
Path resolution costs ~5.6 ns per dot-segment.

---

_Benchmarked: 2026-04-01 • AMD Ryzen Threadripper 3960X (24-core, SMT off) •
Ubuntu 24.04.3 LTS (kernel 6.17.0) • 256 GB DDR4 • 1.93.0 •
Criterion.rs with wasmtime 43 (Cranelift JIT, OptLevel::Speed)_
