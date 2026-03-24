# Advanced Benchmark: Karu vs Cedar (Native + WASM)

Compares all four execution targets using Criterion.rs with in-process wasmtime:

| Target           | Implementation | How It Runs                         |
| ---------------- | -------------- | ----------------------------------- |
| **Karu native**  | Rust library   | Direct `karu::compile` + `evaluate` |
| **Karu WASM**    | Rust → WASM    | In-process via wasmtime crate       |
| **Cedar native** | Rust library   | Direct `cedar_policy::Authorizer`   |
| **Cedar WASM**   | Rust → WASM    | In-process via wasmtime crate       |

## Prerequisites

- Rust toolchain
- `wasm32-wasip1` target: `rustup target add wasm32-wasip1`

## Building WASM Artifacts

```bash
./build.sh
```

This builds:

- `wasm/karu.wasm` - Karu compiled for WASI
- `wasm/cedar.wasm` - Cedar CLI compiled for WASI (requires cloning cedar repo)

## Running Benchmarks

```bash
cargo bench
```

Results are saved to `target/criterion/` with HTML reports.

## Scenarios

| Scenario        | Description                |
| --------------- | -------------------------- |
| Simple          | Single equality condition  |
| Multi-condition | 4 conditions ANDed         |
| Nested path     | 6 levels of object nesting |
| Complex         | 20 rules evaluated         |
