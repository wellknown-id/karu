# Karu

Karu is an embeddable policy engine focusing on structural pattern matching over arbitrary JSON data. It is the spiritual successor to the ideas found in logic-based policy languages, designed to solve complex hierarchical data validation that strict-schema engines cannot handle.

## Core Philosophy

- **Structure over Schema**: We do not enforce a schema on input data. We enforce patterns on the data we find.
- **Search, Don't Index**: If you provide a list, we will search it. You shouldn't have to re-map your application data to fit the policy engine.
- **Partial Matching**: A pattern `{a: 1}` matches `{a: 1, b: 2}`.
- **Optionally Strict**: When you need Cedar-level rigor, flip a switch. Karu can enforce strict schemas, exhaustive matching, and static analysis—but only when you ask for it. RFCs shouldn't block good ideas.

## Syntax Draft

Karu uses a simple rule syntax where the `if` body supports deep pattern matching and the `in` operator.

```polar
# The Rule
allow access if
    action == "call" and
    # The Pattern Match
    { name: "lhs", value: 10 } in resource.context.namedArguments;
```

## How it works (Internals)

Karu uses a recursive **Unification Engine** (simplified for matching).

1. **Traverse**: It walks the JSON tree to `resource.context.namedArguments`.
2. **Iterate**: The `in` operator triggers an array iterator.
3. **Unify**: For every item, it checks if the item is a superset of the pattern `{ name: "lhs", value: 10 }`.
   - _Item 1_: `{name: "junk"}` -> Fail (name != lhs)
   - _Item 2_: `{name: "lhs", value: 10, type: "int"}` -> Success (contains name:lhs and value:10. Extra fields ignored.)

## Cedar Interop

Karu supports full round-trip conversion with [Cedar](https://www.cedarpolicy.com/) policies and schemas. See [Known Cedar Limitations](KNOWN-CEDAR-LIMITATIONS.md) for current gaps.

## Comparison

| Feature          | Cedar              | Rego                | Karu                      |
| ---------------- | ------------------ | ------------------- | ------------------------- |
| List Search      | ❌ (Strict Schema) | ✅ (Complex syntax) | ✅ (Native `in` operator) |
| Pattern Matching | ❌                 | ✅                  | ✅                        |
| Strict Mode      | ✅ (Usually)       | ❌                  | ✅ (Optional)             |
| Duck Typing      | ❌                 | ✅                  | ✅                        |
| Syntax           | SQL-like           | Datalog             | Polar-like                |
| Focus            | Performance/Safety | Infrastructure      | Why not both?             |

## Integration with Kodus

Karu is the core authorization engine for the Kodus platform. It is utilized by the `kodus-authz` crate to enforce fine-grained access control policies across the distributed runtime.
