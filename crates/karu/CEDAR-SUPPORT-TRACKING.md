# Cedar Support Tracking

Status of Cedar feature support in Karu.

## Parser (cedar_parser.rs) — ✅ Full Grammar

The Cedar parser covers the complete Cedar grammar specification. All features below parse correctly.

## Cedar → Karu Import (cedar_import.rs)

### ✅ Supported

| Feature | Karu Mapping |
|---------|-------------|
| `permit` / `forbid` | `allow` / `deny` |
| Scope `principal == Entity` | `Compare(principal, Eq, "id")` |
| Scope `principal in Entity` | `In("id", principal.groups)` |
| Scope `action == Entity` | `Compare(action, Eq, "id")` |
| Scope `action in Entity` | `Compare(action, Eq, "id")` |
| Scope `action in [Entity, ...]` | `Or(Compare(action, Eq, "id"), ...)` |
| Scope `resource == Entity` | `Compare(resource, Eq, "id")` |
| Scope `resource in Entity` | `Compare(resource.type, Eq, "id")` |
| `when { expr }` | Rule body condition |
| `unless { expr }` | `Not(expr)` |
| `&&` | `And(...)` |
| `||` | `Or(...)` |
| `!` | `Not(...)` |
| `==`, `!=`, `<`, `>`, `<=`, `>=` | `Compare(path, op, pattern)` |
| `Entity::"id"` (entity refs) | String literal `"id"` |
| `resource.field` (dot access) | `PathAst` segments |
| `.contains(x)` | `In(x, collection)` |
| `.containsAll([set])` | `Compare(path, ContainsAll, [array])` |
| `.containsAny([set])` | `Compare(path, ContainsAny, [array])` |
| `has field` | `Has { path }` (true if non-null) |
| `like "glob*"` | `Like { path, pattern }` (glob match) |
| Set literals `[a, b]` in patterns | `PatternAst::Array(...)` |
| Record literals `{k: v}` in patterns | `PatternAst::Object(...)` |
| `@id("name")` annotation | Rule name |
| `ip(path).isInRange(ip("cidr"))` | `Compare(path, IpIsInRange, "cidr")` |
| `ip(path).isIpv4()` / `.isIpv6()` | `Compare(path, IsIpv4/IsIpv6, _)` |
| `ip(path).isLoopback()` / `.isMulticast()` | `Compare(path, IsLoopback/IsMulticast, _)` |
| `decimal(path).lessThan(decimal("v"))` | `Compare(path, DecimalLt, "v")` |
| `decimal(path).lessThanOrEqual(...)` | `Compare(path, DecimalLe, "v")` |
| `decimal(path).greaterThan(...)` | `Compare(path, DecimalGt, "v")` |
| `decimal(path).greaterThanOrEqual(...)` | `Compare(path, DecimalGe, "v")` |
| `if C then T else E` | `Or(And(C,T), And(Not(C),E))` — short-circuits for bool branches |
| Arithmetic (`+`, `-`, `*`) | Constant-folded at import time |
| Unary negation (`-expr`) | Constant-folded to negative literal |
| Karu `or` expressions | `ConditionExpr::Or(...)` — full nested boolean logic |

### ❌ Unsupported (explicit error on import)

These features parse correctly but fail at import because Karu's AST/evaluator has no equivalent. Each produces a clear error message.

| Feature | Reason | Potential Fix |
|---------|--------|---------------|
| `is TypeName` | No entity type system in Karu | Add `IsType(path, type)` to `ExprAst` + evaluator |
| Template slots (`?principal`, `?resource`) | No template instantiation | Add template parameter system |

## LSP Support

- ✅ Syntax error diagnostics for `.cedar` files (via Cedar parser)
- ✅ Karu import compatibility warnings
- ✅ Document symbols (policy outline)
- ❌ Semantic tokens / syntax highlighting (via TextMate grammar only, not LSP)
- ❌ Hover information for Cedar keywords
- ❌ Go-to-definition for Cedar entities
- ❌ Completion for Cedar keywords
- ❌ Convert to Karu feature that resaves a file as a .karu

## VS Code Extension

- ✅ `.cedar` file association
- ✅ TextMate syntax highlighting
- ✅ Language configuration (comments, brackets, auto-close)
- ✅ LSP activation for `.cedar` files
