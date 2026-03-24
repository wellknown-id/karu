# Karu Performance Benchmarks

Karu was created to replace Cedar in [kodus](https://github.com/kodus/kodus) - and not because Cedar was slow, but because we realised [along the way](https://en.wikipedia.org/wiki/Coastline_paradox) it wasn't expressive enough for the use cases we planned for. We needed a replacement that could give us something like [Polar's approachableness](https://www.osohq.com/docs/reference/polar/introduction) and keep Cedar's performance characteristics. We had a choice, fork from the archived remnants of Polar or be inspired by it. We took a punt and chose the latter. We didn't expect to beat Cedar in every single performance test we tried. So we attached a `strict mode` to Karu to play fairly. When we were still faster, we added native Cedar support.

_TLDR; Karu is faster at Cedar than Cedar is at Cedar._

## Summary

| Metric                                   | Karu           | Cedar           | Winner                |
| ---------------------------------------- | -------------- | --------------- | --------------------- |
| **WASM Bundle**                          | 319 KB         | 1.8 MB          | **Karu 5.6x smaller** |
| **Native eval**                          | 16.4 ns        | 1,162 ns        | **Karu 71x faster**   |
| **WASM eval (precompiled)**              | 459 ns         | 94,054 ns       | **Karu 205x faster**  |
| **WASM eval (parse+eval)**              | 3,680 ns       | 206,550 ns      | **Karu 56x faster**   |
| **Complex (20 rules, native)**           | 649 ns         | 13,749 ns       | **Karu 21x faster**   |
| **Complex (20 rules, WASM precompiled)** | 7,713 ns       | 219,850 ns      | **Karu 29x faster**   |

## Methodology

All benchmarks use [Criterion.rs](https://github.com/bheisler/criterion.rs) for
statistically rigorous measurement (100 samples, 5-second measurement window).

- **Native**: Direct Rust library calls (`karu::compile` / `cedar_policy::Authorizer`)
- **WASM**: In-process via [wasmtime](https://wasmtime.dev/) with Cranelift JIT (`OptLevel::Speed`)
  — the WASM is compiled to native machine code _before_ benchmarking starts
- **Precompiled**: Policy is compiled once; only evaluation is benchmarked
- **Parse+eval**: Policy is parsed and evaluated every iteration

```bash
# Run benchmarks
cd benches/advanced-benchmark
./build.sh      # build wasm/karu.wasm + wasm/cedar.wasm
cargo bench     # run all benchmarks
```

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
| Simple (1 condition)   | **16.4 ns** | 1,162 ns   | **71x** |
| Multi-condition (4)    | **122 ns**  | 1,276 ns   | **10x** |
| Nested path (6 levels) | **40.6 ns** | 1,665 ns   | **41x** |
| Complex (20 rules)     | **649 ns**  | 13,749 ns  | **21x** |

## Karu Loading Cedar Policies (Native)

Karu can import Cedar syntax and evaluate it without any performance penalty.
Policies are converted from Cedar → Karu via `compile_cedar()`.

| Scenario           | Karu (native) | Karu (Cedar import) | Cedar (native) |
| ------------------ | ------------- | ------------------- | -------------- |
| Simple             | 16.4 ns       | **22.8 ns**         | 1,162 ns       |
| Multi-condition    | 122 ns        | **146 ns**          | 1,276 ns       |
| Nested path        | 40.6 ns       | **46.9 ns**         | 1,665 ns       |
| Complex (20 rules) | 649 ns        | **231 ns**          | 13,749 ns      |

> **Karu loading Cedar policies is 8–60x faster than Cedar native**

## WASM Benchmarks — Precompiled (eval-only)

Policy compiled once, only evaluation is timed.

| Scenario           | Karu         | Karu (Cedar import) | Cedar       | Speedup vs Cedar |
| ------------------ | ------------ | ------------------- | ----------- | ---------------- |
| Simple             | **459 ns**   | 634 ns              | 94,054 ns   | **205x**         |
| Multi-condition    | **1,292 ns** | 1,563 ns            | 106,320 ns  | **82x**          |
| Nested path        | **1,222 ns** | 1,471 ns            | 133,440 ns  | **109x**         |
| Complex (20 rules) | **7,713 ns** | 7,012 ns            | 219,850 ns  | **29x**          |

> Karu precompiled WASM (459 ns) is **faster than Cedar native** (1,162 ns)!
> Karu loading Cedar via WASM (634 ns) is **still faster than Cedar native**!

## WASM Benchmarks — Parse + Eval

Policy parsed and evaluated every iteration.

| Scenario           | Karu           | Karu (Cedar import) | Cedar       | Speedup vs Cedar |
| ------------------ | -------------- | ------------------- | ----------- | ---------------- |
| Simple             | **3,680 ns**   | 5,740 ns            | 206,550 ns  | **56x**          |
| Multi-condition    | **10,437 ns**  | 14,362 ns           | 272,240 ns  | **26x**          |
| Nested path        | **6,571 ns**   | 8,257 ns            | 264,580 ns  | **40x**          |
| Complex (20 rules) | **59,630 ns**  | 121,150 ns          | 742,280 ns  | **12x**          |

## Native vs WASM Overhead

| Scenario        | Native    | WASM Precompiled | Overhead |
| --------------- | --------- | ---------------- | -------- |
| Simple (Karu)   | 16.4 ns   | 459 ns           | 28x      |
| Simple (Cedar)  | 1,162 ns  | 94,054 ns        | 81x      |
| Complex (Karu)  | 649 ns    | 7,713 ns         | 12x      |
| Complex (Cedar) | 13,749 ns | 219,850 ns       | 16x      |

Karu's WASM overhead is **12–28x** (vs Cedar's **16–81x**), meaning
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
| **1**   | **1.93 M**   | 80 K          | **24x** |
| **2**   | **3.07 M**   | 123 K         | **25x** |
| **4**   | **5.99 M**   | 178 K         | **34x** |
| **8**   | **9.60 M**   | 205 K         | **47x** |
| **16**  | **7.48 M**   | 242 K         | **31x** |

### Delete Authorization (2 rules, ~400 diverse queries)

| Threads | Karu ops/sec | Cedar ops/sec | Speedup |
| ------- | ------------ | ------------- | ------- |
| **1**   | **2.83 M**   | 126 K         | **23x** |
| **2**   | **3.82 M**   | 222 K         | **17x** |
| **4**   | **5.60 M**   | 355 K         | **16x** |
| **8**   | **5.61 M**   | 534 K         | **10x** |
| **16**  | **6.57 M**   | 571 K         | **11.5x** |

### Thread Scaling Efficiency

| Threads | Karu (read) | Cedar (read) |
| ------- | ----------- | ------------ |
| 1→2     | **1.59x**   | 1.54x        |
| 1→4     | **3.10x**   | 2.22x        |
| 1→8     | **4.97x**   | 2.56x        |
| 1→16    | **3.87x**   | 3.03x        |

> At 8 threads, Karu evaluates **9.60 million authorization decisions per second**.
> Cedar maxes out at **205K** — a **47x difference**.
> With 10,000 concurrent users hitting a single node, **Karu handles all evaluations in 1.0ms**.

## Karu Standalone Benchmarks

Additional microbenchmarks from the core crate:

### Evaluation

| Benchmark                    | Time       |
| ---------------------------- | ---------- |
| eval_simple                  | 15.95 ns   |
| eval_multi_condition (4)     | 95.67 ns   |
| eval_nested_path (6 levels)  | 39.10 ns   |
| collection_search (10 items) | 182.8 ns   |
| collection_search (100)      | 1.270 µs   |
| collection_search (1000)     | 12.07 µs   |
| throughput_1000              | 19.11 µs   |
| parse_simple                 | 928 ns     |
| parse_100_rules              | 154.1 µs   |

### Parser Comparison: Handrolled vs Tree-sitter

| Scenario   | Handrolled | Tree-sitter | Speedup |
| ---------- | ---------- | ----------- | ------- |
| Simple     | 757 ns     | 8.78 µs     | **12x** |
| Medium     | 5.54 µs    | 65.5 µs     | **12x** |
| 100 rules  | 111 µs     | 1.29 ms     | **12x** |

## Optimization Notes

The evaluation hot path has been extensively profiled and optimized:

- **`evaluate_fast()`** — bypasses HashMap/bindings for non-quantified conditions
- **Literal fast-path** — `Eq`/`Ne` with literal patterns does direct `Value` comparison
- **`resolve_fast()`** — path resolution without HashMap allocation
- **`matches_ref()`** — pattern matching without Bindings struct allocation
- **Zero-clone evaluation** — patterns are used by reference, never cloned during eval

Per-condition cost is ~26 ns (path resolve + value compare + dispatch).
Path resolution costs ~5.6 ns per dot-segment.

---

_Benchmarked: 2026-03-24 • AMD Ryzen Threadripper 3960X (24-core, SMT off) •
Ubuntu 24.04.3 LTS (kernel 6.17.0) • 256 GB DDR4 • Rust 1.93.0 •
Criterion.rs with wasmtime 43 (Cranelift JIT, OptLevel::Speed)_
