# Security Audit

> **Scope**: Parser, compiler, and evaluator hardening against adversarial inputs.
>
> **Date**: 1 April 2026
>
> **Baseline commit**: `abadfd6` (main)
>
> **Type**: Internal review — not an independent external audit.

---

## Fixes Applied

### 1. Empty String Injection

**Commits**: `6018478`, `b246dbe`

The parser accepted `Token::String("")` in name positions without validation. A fuzzer-discovered input `deny"";` produced a rule with an empty name, violating downstream invariants.

**Fix**: Added empty-string rejection at all name-position consumption sites:

| Site | Example | Status |
|------|---------|--------|
| Rule name | `deny"";` | Rejected |
| Import path | `import "";` | Rejected |
| Action name | `action "";` | Rejected |
| Test name | `test "" { ... }` | Rejected |
| Pattern value | `x == ""` | Allowed (valid) |
| Object key | `{"": val}` | Allowed (valid JSON) |

### 2. Stack Overflow via Deep Recursion

**Commit**: `60bdc92`

The recursive descent parser had no depth limit. Crafted inputs with 20,000+ levels of nesting overflowed the default 8MB thread stack, crashing the process with `SIGABRT`.

**Attack vectors**:
- `not not not ... not true` (20k deep — `parse_unary_expr` → self)
- `(((... true ...)))` (nested parens — `parse_primary_expr` → `parse_expr` cycle)
- `{a: {b: {c: ...}}}` (nested objects — `parse_pattern` → self)

**Fix**: Added `MAX_DEPTH = 256` counter on the `Parser` struct, checked at `parse_expr`, `parse_unary_expr`, and `parse_pattern` entry points. Inputs exceeding the limit produce a clean `ParseError`.

### 3. Double Negation Amplification

**Commit**: `abadfd6`

Even with the depth limit, an adversary could waste 256 depth slots with trivial `not not` pairs that cancel out semantically.

**Fix**: Consecutive `not` tokens are now counted iteratively and collapsed at parse time. `not not x` compiles to `x`; `not not not x` compiles to `not x`. 50,000 consecutive `not` tokens cost exactly one depth increment.

---

## Red Team Exercise

Nine adversarial scenarios were tested against the engine in release mode. All passed.

### Results

| # | Scenario | Outcome | Time |
|---|----------|---------|------|
| 1 | **Glob ReDoS** — `like "*a*a*a*...*b"` vs `"aaa..."×100` | ✅ Safe (linear matcher) | 2.6µs |
| 2 | **Quadratic quantifier** — `forall × exists` on 10k items | ✅ 2.3ms (inherent O(n²)) | 2.3ms |
| 3 | **Memory bomb** — 10MB string literal in policy | ✅ Bounded by input size | 36ms |
| 4 | **Assertion cycle** — `assert a if b; assert b if a;` | ✅ Detected at compile | instant |
| 5 | **100k OR clauses** | ✅ Linear | 1.5ms eval |
| 6 | **1000-segment path** — `a.b.c.d...×1000` | ✅ Fast | 620ns |
| 7 | **10k rules** | ✅ Linear | 121µs eval |
| 8 | **Exponential assertion tree** — depth-15 binary tree | ✅ No explosion | 58µs |
| 9 | **1M element JSON input** — simple policy | ✅ Short-circuit | 770ns |

### Analysis

**Glob matcher** (`like` operator): Uses `split('*')` + linear `find()` — no regex, no backtracking. Not susceptible to ReDoS.

**Assertion inlining**: The compiler tracks an `expanding: HashSet<String>` during assertion expansion. Cycles (`A → B → A`) are detected and produce a compile error. The tree-shaped expansion test (depth 15, theoretically 2^15 nodes) completes in 58µs because each assertion is expanded inline at its single reference site — there is no shared sub-expression duplication.

**Quantifier complexity**: `forall x in arr: exists y in arr: ...` is inherently O(n²). At n=10k this takes 2.3ms (release). An adversary would need to control both the policy definition and submit enormous input arrays to cause real pain. Policy authoring is a privileged operation, not an untrusted input surface.

**No-match short-circuit**: The evaluator traverses paths lazily. A 1M-element JSON object is evaluated in 770ns when the matched path is a top-level field — irrelevant subtrees are never visited.

### Residual Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| O(n^k) nested quantifiers with large inputs | Low | Policy is a privileged surface; input size is caller-controlled |
| Memory proportional to policy size | Low | Inherent; 10MB policy compiles in 36ms |
| No compile-time limit on rule/clause count | Low | Linear scaling; 100k clauses compile in 63ms |
