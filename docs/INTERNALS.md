# Karu Internals

## How It Works

Karu uses a **recursive structural matcher**.

1. **Traverse**: It walks the JSON tree to the target path (e.g. `resource.context.namedArguments`).
2. **Iterate**: The `in` operator triggers an array iterator over the resolved value.
3. **Unify**: For every item, it checks if the item is a superset of the pattern `{ name: "lhs", value: 10 }`.
   - _Item 1_: `{name: "junk"}` → Fail (name ≠ lhs)
   - _Item 2_: `{name: "lhs", value: 10, type: "int"}` → Success (contains name:lhs and value:10. Extra fields ignored.)

### Evaluation Model

Policy evaluation is **default-deny**. Rules are evaluated in order:

- **`allow`** rules grant access when their conditions match.
- **`deny`** rules explicitly block access.
- If no rule matches, the result is `Deny`.

When multiple rules match, `deny` takes precedence over `allow` (deny-overrides). This is consistent with Cedar's evaluation semantics.

### Compilation Pipeline

```
Source (.karu)
    │
    ├─ Parser ──→ AST (RuleAst, ExprAst, PatternAst)
    │
    ├─ Compiler ──→ Policy (Rule, Condition, Pattern)
    │                 ├─ Path resolution
    │                 ├─ Pattern compilation
    │                 └─ Assertion inlining
    │
    └─ Runtime ──→ Effect (Allow | Deny)
                    ├─ Recursive JSON traversal
                    ├─ Structural pattern matching
                    └─ Variable binding extraction
```

### Pattern Matching

Patterns use **structural subtyping** — a pattern matches any JSON value that contains at least the specified fields:

| Pattern    | Matches                      | Doesn't Match    |
| ---------- | ---------------------------- | ---------------- |
| `"alice"`  | `"alice"`                    | `"bob"`          |
| `42`       | `42`                         | `43`             |
| `true`     | `true`                       | `false`          |
| `_`        | anything                     | —                |
| `{ a: 1 }` | `{ a: 1 }`, `{ a: 1, b: 2 }` | `{ a: 2 }`, `{}` |
| `[1, 2]`   | `[1, 2]`, `[1, 2, 3]`        | `[2, 1]`         |

### Variable Bindings

The `in` operator with quantifiers (`forall`, `exists`) can bind loop variables:

```
forall item in resource.items: item.status == "active"
```

During each iteration, `item` is bound to the current array element, and `item.status` resolves against it.

### Cedar Interop Pipeline

```
Cedar Source (.cedar)             Cedar Schema (.cedarschema)
    │                                 │
    ├─ cedar_parser ──→ CedarAST      ├─ cedar_schema_parser ──→ ModuleDef[]
    │                                 │
    └─ cedar_import ──→ Karu AST ←────┘
                           │
                           ├─ Compile ──→ Karu Policy (native eval)
                           └─ Transpile ──→ Cedar Source (round-trip)
```

### WASM Build

```bash
# Build for browser (--no-default-features disables LSP/tokio)
wasm-pack build --target web --no-default-features --features wasm
# Output: pkg/karu_bg.wasm
```

**C-FFI exports** (for wazero, CGO, or any WASM host):

| Function         | Signature                                        | Returns                   |
| ---------------- | ------------------------------------------------ | ------------------------- |
| `karu_alloc`     | `(size: usize)`                                  | `*mut u8`                 |
| `karu_free`      | `(ptr: *mut u8, size: usize)`                    | void                      |
| `karu_compile`   | `(source_ptr, source_len)`                       | `*mut KaruPolicy` or null |
| `karu_policy_free` | `(policy: *mut KaruPolicy)`                    | void                      |
| `karu_evaluate`  | `(policy, input_ptr, input_len)`                 | `1` / `0` / `-1`         |
| `karu_eval_once` | `(policy_ptr, policy_len, input_ptr, input_len)` | `1` / `0` / `-1`         |

**wasm-bindgen exports** (browser/JS):

| Function            | Signature                              | Returns                                       |
| ------------------- | -------------------------------------- | --------------------------------------------- |
| `karu_eval_js`      | `(policy: &str, input: &str)`         | `{result: "ALLOW"\|"DENY"}` or `{error: ...}` |
| `karu_check_js`     | `(policy: &str)`                      | `{ok: bool, rules: number}` or `{error: ...}` |
| `karu_simulate_js`  | `(policy: &str, input: &str)`         | `{decision, matched_rules}` or error          |
| `karu_diff_js`      | `(old: &str, new: &str)`              | `{added, removed, modified, summary}` or error |
| `karu_batch_js`     | `(policy: &str, inputs: &str)`        | `{results: ["ALLOW"\|"DENY", ...]}` or error  |
| `karu_transpile_js` | `(policy: &str)` *(cedar feature)*    | `{cedar: string}` or `{error: ...}`           |
