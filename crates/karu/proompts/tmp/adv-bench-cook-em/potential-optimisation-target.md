# Optimisation Target: Eliminate unnecessary allocations in `any_matches` and add fast-path for `In` operator

## Problem

The `In` operator (`any_matches`) currently always goes through `any_match_with_bindings` which:
1. Creates a `Bindings` (HashMap) for every element check even when bindings are never used
2. For the scalar-in-literal-array case (`role in ["admin", "editor"]`), it still allocates a `Bindings::new()` on the success path
3. For the array-contains-pattern case, every `match_with_bindings` call allocates a fresh `Bindings` HashMap

## Additionally: `dispatch_op` can fast-path `In` for `Literal(Array)` patterns

When the operator is `In` and the pattern is `Literal(Array(...))`, we know we're doing scalar membership. The current code path:
```
dispatch_op → any_matches → any_match_with_bindings → (array check fails) → literal array check → Bindings::new()
```

Can be simplified to:
```
dispatch_op → arr.contains(data)  // done, zero allocations
```

## Also: Add `#[inline]` to critical evaluation functions

The `ConditionExpr::evaluate`, `Rule::evaluate`, and `Policy::evaluate` functions lack `#[inline]` hints, which prevents cross-function optimizations in the hot evaluation loop.

## Expected Impact

- `multi_condition` and `complex_20_rules` should see minor improvement from inlining
- `scale_karu_read` should see improvement from the `In` fast-path (used in the `role in ["admin", "editor"]` conditions)
- All scenarios benefit from reduced allocation pressure in `any_matches`

## Risk: Low

These are pure fast-path additions - the slow paths remain unchanged for complex pattern types.
