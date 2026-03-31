# Karu Fit Assessment for kodus

_Response to POLICY-OPTIONS.md requirements_
_Date: 2026-02-05 (updated 2026-03-31)_

---

## Summary

Karu is a **strong conceptual fit** for kodus's policy model but has a **Go integration gap** that would require WASM-based bridging.

---

## Requirements Compatibility

| #   | Requirement                   | Status | Notes                                                            |
| --- | ----------------------------- | ------ | ---------------------------------------------------------------- |
| 1   | PARC Model                    | ✅     | Native JSON path access for all four dimensions                  |
| 2   | Permit/Forbid + Deny Override | ✅     | `allow`/`deny` with deny-overrides semantics                     |
| 3   | ABAC Conditions               | ✅     | Full path expressions: `context.hour_utc >= 9`                   |
| 4   | Path Pattern Matching         | ✅     | Native `like` glob operator: `resource.path like "*/data/*"`     |
| 5   | Typed Entity System           | ⚠️     | Duck-typed JSON, use `principal.type == "Module"` convention     |
| 6   | Container Membership          | ✅     | Array search with `in` operator                                  |
| 7   | Structured Data Access        | ✅     | `context.arguments["0"].host` works directly                     |
| 8   | Go Native Library             | ❌     | **Rust-only** - requires wazero WASM or CGO                      |
| 9   | AST Introspection             | ✅     | `Parser::parse()` returns full AST                               |
| 10  | Performance (<100μs)          | ✅     | **Verified: 12μs simple, 2.2μs/op batch**                        |
| 11  | Composable Policies           | ⚠️     | Concatenate policy source before compilation; no merge API       |
| 12  | Deterministic Hashing         | ✅     | Serialize AST for hashing                                        |
| 13  | Policy Recorder               | ✅     | Denial + AST = regenerate rules                                  |
| 14  | Human-Readable Syntax         | ✅     | Polar-inspired, very readable                                    |
| 15  | Error Messages                | ✅     | Rich errors with line/column spans, caret annotations, multi-error |
| 16  | Docs & Maintenance            | ✅     | Full LSP, playground, fuzz testing, simulation, versioning/diff  |

---

## Key Gaps

### 1. Go Integration (Blocking)

kodus is Go; Karu is Rust. Options:

| Approach            | Effort | Notes                                            |
| ------------------- | ------ | ------------------------------------------------ |
| **WASM via wazero** | Medium | kodus already has WASM expertise; proven pattern |
| **CGO bindings**    | Medium | Adds build complexity                            |
| **Stay with Cedar** | Zero   | It's working - path of least resistance          |

### 2. Typed Entities (Workaround Available)

Cedar: `Module::"app.wasm"`
Karu: `principal.type == "Module" and principal.id == "app.wasm"`

Functionally equivalent, syntactically different.

### ~~3. Glob Patterns~~ (Resolved)

Karu now supports the `like` keyword for glob pattern matching (`*`, `?`), compiled to `Pattern::Glob`. This resolves the parity gap with Cedar's `like` operator.

---

## Syntax Comparison

**Cedar (current)**:

```cedar
permit(
    principal,
    action == Action::"filesystem:read",
    resource
) when {
    resource.path like "*/data/*"
};
```

**Karu (equivalent)**:

```karu
allow filesystem_read if
    action == "filesystem:read" and
    resource.path like "*/data/*";
```

---

## Recommendation

| If...                             | Then...                          |
| --------------------------------- | -------------------------------- |
| Staying Go-native is priority     | **Keep Cedar**                   |
| Want to unify policy + playground | Prototype **wazero integration** |
| Migrating kodus to Rust           | Karu becomes **ideal choice**    |

---

## Wazero Integration Path

kodus already has WASM expertise - wazero integration is a natural fit.

### What's Already Available

The Karu WASM build (`pkg/karu_bg.wasm`, **221 KB**) exports:

**wasm-bindgen (browser) API:**

| Function             | Signature                                  | Returns                                          |
| -------------------- | ------------------------------------------ | ------------------------------------------------ |
| `karu_eval_js`       | `(policy: &str, input: &str)`              | `{result: "ALLOW"\|"DENY"}` or `{error: string}` |
| `karu_check_js`      | `(policy: &str)`                           | `{ok: bool, rules: number}` or `{error: string}` |
| `karu_simulate_js`   | `(policy: &str, input: &str)`              | `{decision, matched_rules}` or `{error: string}` |
| `karu_diff_js`       | `(old_policy: &str, new_policy: &str)`     | `{added, removed, modified, summary}` or error   |
| `karu_batch_js`      | `(policy: &str, inputs: &str)`             | `{results: ["ALLOW"\|"DENY", ...]}` or error     |
| `karu_transpile_js`  | `(policy: &str)` _(requires cedar feature)_ | `{cedar: string}` or `{error: string}`           |

**C-compatible FFI (for wazero/CGO integration):**

| Function              | Signature                                        | Returns                    |
| --------------------- | ------------------------------------------------ | -------------------------- |
| `karu_alloc`          | `(size: usize)`                                  | `*mut u8`                  |
| `karu_free`           | `(ptr: *mut u8, size: usize)`                    | void                       |
| `karu_compile`        | `(source_ptr, source_len)`                        | `*mut KaruPolicy` or null  |
| `karu_policy_free`    | `(policy: *mut KaruPolicy)`                       | void                       |
| `karu_evaluate`       | `(policy, input_ptr, input_len)`                  | `1` / `0` / `-1`          |
| `karu_eval_once`      | `(policy_ptr, policy_len, input_ptr, input_len)` | `1` / `0` / `-1`          |

### Go Integration Sketch

```go
package karu

import (
    "context"
    "github.com/tetratelabs/wazero"
    "github.com/tetratelabs/wazero/api"
)

type Engine struct {
    runtime wazero.Runtime
    module  api.Module
}

func New(ctx context.Context, wasmBytes []byte) (*Engine, error) {
    r := wazero.NewRuntime(ctx)
    mod, err := r.Instantiate(ctx, wasmBytes)
    if err != nil {
        return nil, err
    }
    return &Engine{runtime: r, module: mod}, nil
}

func (e *Engine) Evaluate(ctx context.Context, policy, input string) (string, error) {
    // 1. Allocate memory for strings via karu_alloc
    // 2. Copy policy and input bytes to WASM memory
    // 3. Call karu_eval_once(policyPtr, policyLen, inputPtr, inputLen)
    // 4. Interpret result: 1 = ALLOW, 0 = DENY, -1 = error
    // 5. Free memory via karu_free

    evalFn := e.module.ExportedFunction("karu_eval_once")
    results, err := evalFn.Call(ctx, policyPtr, policyLen, inputPtr, inputLen)
    if err != nil {
        return "", err
    }

    switch results[0] {
    case 1:
        return "ALLOW", nil
    case 0:
        return "DENY", nil
    default:
        return "", errors.New("policy evaluation error")
    }
}
```

### Performance Notes (Verified 2026-02-05)

| Metric          | Karu WASM     | Cedar WASM          |
| --------------- | ------------- | ------------------- |
| **Bundle size** | 221 KB        | 4.3 MB (20x larger) |
| **Init time**   | 2 ms          | 0 ms (lazy)         |
| **Simple eval** | **12 μs**     | 113 μs              |
| **Batch (1K)**  | **2.2 μs/op** | 89 μs/op            |

- **Cold start**: ~2ms (WASM instantiation with prefetch)
- **Warm evaluation**: 12-20μs per request
- **Batch throughput**: ~450K ops/sec
- **vs Cedar**: **6-10x faster** across all scenarios
- **Caching**: kodus's existing `(path, action, policyRef)` cache still applies

### Build Command

```bash
# From karu repo (--no-default-features disables LSP/tokio which can't compile to wasm32)
wasm-pack build --target web --no-default-features --features wasm

# Output: pkg/karu_bg.wasm (embed this in kodus)
```

---

## Next Steps (If Proceeding)

1. Prototype wazero-based Karu integration
2. Benchmark evaluation latency in wazero runtime
3. ~~Add glob pattern support to Karu DSL~~ ✅ Done (`like` operator)
4. Build Cedar → Karu migration tool (Karu already has `import` command)

---

_Prepared by: Karu development team_
