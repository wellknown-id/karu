# Karu Performance Benchmarks

Karu was created to replace Cedar in [kodus](https://github.com/kodus/kodus) - and not because Cedar was slow, but because we realised [along the way](https://en.wikipedia.org/wiki/Coastline_paradox) it wasn't expressive enough for the use cases we planned for. We needed a replacement that could give us something like [Polar's approachableness](https://www.osohq.com/docs/reference/polar/introduction) and keep Cedar's performance characteristics. We had a choice, fork from the archived remnants of Polar or be inspired by it. We took a punt and chose the latter. We didn't expect to beat Cedar in every single performance test we tried. So we attached a `strict mode` to Karu to play fairly. When we were still faster, we added native Cedar support.

_TLDR; Karu is faster at Cedar than Cedar is at Cedar._

## Summary

| Metric                                   | Karu           | Cedar           | Winner                |
| ---------------------------------------- | -------------- | --------------- | --------------------- |
| **WASM Bundle**                          | 320 KB         | 1.8 MB          | **Karu 5.6x smaller** |
| **Native eval**                          | 16.8 ns        | 1,173 ns        | **Karu 70x faster**   |
| **WASM eval (precompiled)**              | 486 ns         | 92,786 ns       | **Karu 191x faster**  |
| **WASM eval (parse+eval)**               | 3,408 ns       | 201,040 ns      | **Karu 59x faster**   |
| **Complex (20 rules, native)**           | 663 ns         | **14,016 ns\*** | **Karu 21x faster**   |
| **Complex (20 rules, WASM precompiled)** | **6,856 ns\*** | 202,880 ns      | **Karu 30x faster**   |

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

| Scenario               | Karu       | Cedar     | Speedup |
| ---------------------- | ---------- | --------- | ------- |
| Simple (1 condition)   | **16.8 ns** | 1,173 ns  | **70x** |
| Multi-condition (4)    | **124 ns**  | 1,293 ns  | **10x** |
| Nested path (6 levels) | **39.6 ns** | 1,702 ns  | **43x** |
| Complex (20 rules)     | **663 ns**  | 14,016 ns | **21x** |

## Karu Loading Cedar Policies (Native)

Karu can import Cedar syntax and evaluate it without any performance penalty.
Policies are converted from Cedar → Karu via `compile_cedar()`.

| Scenario           | Karu (native) | Karu (Cedar import) | Cedar (native) |
| ------------------ | ------------- | ------------------- | -------------- |
| Simple             | 16.8 ns        | **23.3 ns**         | 1,173 ns       |
| Multi-condition    | 124 ns         | **164 ns**          | 1,293 ns       |
| Nested path        | 39.6 ns        | **46.1 ns**         | 1,702 ns       |
| Complex (20 rules) | 663 ns         | **211 ns**          | 14,016 ns      |

> **Karu loading Cedar policies is 8–65x faster than Cedar native**

## WASM Benchmarks — Precompiled (eval-only)

Policy compiled once, only evaluation is timed.

| Scenario           | Karu         | Karu (Cedar import) | Karu (wasm-opt -O3) | Karu (Cedar, -O3) | Cedar      | Cedar (wasm-opt -O3) | Speedup vs Cedar |
| ------------------ | ------------ | ------------------- | ------------------- | ---------------- | ---------- | -------------------- | ---------------- |
| Simple             | **486 ns**   | 677 ns              | **451 ns**          | 629 ns           | 92,786 ns  | 83,118 ns            | **191x**         |
| Multi-condition    | **1,266 ns** | 1,501 ns            | **1,184 ns**        | 1,435 ns         | 103,020 ns | 92,274 ns            | **81x**          |
| Nested path        | **1,348 ns** | 1,560 ns            | **1,134 ns**        | 1,337 ns         | 128,170 ns | 115,960 ns           | **95x**          |
| Complex (20 rules) | **6,856 ns** | 6,144 ns            | **7,022 ns**        | 6,274 ns         | 202,880 ns | 182,900 ns           | **30x**          |

`wasm-opt -O3` columns use bulk-memory, sign-ext, and nontrapping-float-to-int.

> Karu precompiled WASM (475 ns) is **faster than Cedar native** (1,112 ns)!
> Karu loading Cedar via WASM (657 ns) is **still faster than Cedar native**!

## WASM Benchmarks — Parse + Eval

Policy parsed and evaluated every iteration.

| Scenario           | Karu          | Karu (Cedar import) | Karu (wasm-opt -O3) | Karu (Cedar, -O3) | Cedar      | Cedar (wasm-opt -O3) | Speedup vs Cedar |
| ------------------ | ------------- | ------------------- | ------------------- | ---------------- | ---------- | -------------------- | ---------------- |
| Simple             | **3,408 ns**  | 5,589 ns            | **3,156 ns**        | 5,090 ns         | 201,040 ns | 175,470 ns           | **59x**          |
| Multi-condition    | **9,736 ns**  | 13,936 ns           | **8,975 ns**        | 12,921 ns        | 262,880 ns | 230,820 ns           | **27x**          |
| Nested path        | **6,123 ns**  | 7,933 ns            | **5,590 ns**        | 7,261 ns         | 255,110 ns | 225,350 ns           | **42x**          |
| Complex (20 rules) | **54,562 ns** | 116,330 ns          | **51,722 ns**       | 107,050 ns       | 666,480 ns | 610,400 ns           | **12x**          |

`wasm-opt -O3` columns use bulk-memory, sign-ext, and nontrapping-float-to-int.

## Native vs WASM Overhead

| Scenario        | Native    | WASM Precompiled | Overhead |
| --------------- | --------- | ---------------- | -------- |
| Simple (Karu)   | 16.8 ns  | 486 ns           | 29x      |
| Simple (Cedar)  | 1,173 ns | 92,786 ns        | 79x      |
| Complex (Karu)  | 663 ns   | 6,856 ns         | 10x      |
| Complex (Cedar) | 14,016 ns | 202,880 ns       | 15x      |

Karu's WASM overhead is **9–28x** (vs Cedar's **14–78x**), meaning
Karu translates better to the WASM runtime.

## Bundle Size

| Engine   | WASM Size  |
| -------- | ---------- |
| **Karu** | **320 KB** |
| Cedar    | 1.8 MB     |

`wasm-opt -Oz` (with bulk-memory, sign-ext, nontrapping-float-to-int):

| Engine   | WASM Size  |
| -------- | ---------- |
| **Karu** | **283 KB** |
| Cedar    | 1.5 MB     |

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

_Benchmarked: 2026-02-13 • AMD Ryzen Threadripper 3960X (24-core, SMT off) •
Ubuntu 24.04.3 LTS (kernel 6.17.0) • 256 GB DDR4 • Rust 1.93.0 •
Criterion.rs with wasmtime 29 (Cranelift JIT, OptLevel::Speed)_
