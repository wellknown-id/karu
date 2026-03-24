# Known Cedar Limitations

Karu supports full Cedar round-trip (`.cedar` ↔ `.karu` ↔ `.cedarschema`), but the following limitations exist in the current implementation.

## Schema Parser

| Limitation                        | Impact | Notes                                                                                                                                  |
| --------------------------------- | ------ | -------------------------------------------------------------------------------------------------------------------------------------- |
| **Entity kind always `Resource`** | Low    | Cedar schemas don't distinguish `actor` vs `resource`. All entities default to `Resource`; Karu typed mode requires manual annotation. |
| **`tags` parsed and discarded**   | Low    | Cedar `entity ... tags Type` is consumed but not stored in the AST.                                                                    |
| **`enum` entities not supported** | Low    | Cedar's `entity Foo enum [...]` syntax is not implemented.                                                                             |
| **Annotations discarded**         | Low    | `@doc("...")` and other annotations are parsed but not preserved in round-trip.                                                        |

## Schema Transpiler

| Limitation                       | Impact   | Notes                                                                                                                                |
| -------------------------------- | -------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| **Actions emitted individually** | Cosmetic | `action X, Y appliesTo {...}` is re-emitted as separate `action "X" ...` and `action "Y" ...` declarations. Semantically equivalent. |
| **No action grouping**           | Cosmetic | Multi-action declarations are expanded during parsing and not re-collapsed on output.                                                |
| **Inline record indentation**    | Cosmetic | Nested `TypeRef::Record` fields use fixed 4-space indent rather than context-sensitive indentation.                                  |

## Policy Parser

| Limitation                | Impact | Notes                                                                                              |
| ------------------------- | ------ | -------------------------------------------------------------------------------------------------- |
| **`is` type narrowing**   | Medium | Cedar `principal is Admin in Group::"g1"` type tests are not yet supported.                        |
| **Extension functions**   | Low    | `ip()`, `decimal()`, `datetime()`, `duration()` are parsed as function calls but not type-checked. |
| **Annotations discarded** | Low    | Policy-level `@id("...")` annotations are not preserved.                                           |

## Round-Trip Fidelity

- Round-trip tests verify **structural equivalence** (entity/action/field counts, names, effects), not byte-for-byte identical output.
- Formatting, whitespace, and declaration ordering may differ between input and output.
- Multi-entity/action grouping (`entity A, B;`) is expanded to individual declarations on re-emit.
