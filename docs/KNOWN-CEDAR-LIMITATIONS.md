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

## Policy Parser

| Feature                          | Status  | Notes                                                                                                                                                                                                                  |
| -------------------------------- | ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **`is` type narrowing**          | ✅ Done  | `principal is Admin in Group::"g1"` maps to `principal.type == "Admin" AND "g1" in principal.groups`. Requires entity data to carry a `type` field.                                                                    |
| **`ip()` / `decimal()` methods** | ✅ Done  | `ip(path).isInRange(ip("cidr"))`, `.isIpv4()`, `.isIpv6()`, `.isLoopback()`, `.isMulticast()` and `decimal(path).lessThan(decimal("v"))` etc. are fully supported.                                                    |
| **`@id("...")` annotations**     | ✅ Done  | Policy-level `@id("name")` annotations are used to name the imported rule. Other annotations (e.g. `@doc`) are silently dropped.                                                                                       |
| **`has` / `like`**               | ✅ Done  | Attribute existence tests (`has`) and glob pattern matching (`like`) are fully supported.                                                                                                                               |
| **`datetime()` / `duration()`**  | Low     | These extension functions are parsed but method calls on them are not yet converted.                                                                                                                                    |
| **Template slots**               | Low     | `?principal` / `?resource` template slots are not supported.                                                                                                                                                           |

## Round-Trip Fidelity

- Round-trip tests verify **structural equivalence** (entity/action/field counts, names, effects), not byte-for-byte identical output.
- Formatting, whitespace, and declaration ordering may differ between input and output.
- Multi-entity/action grouping (`entity A, B;`) is expanded to individual declarations on re-emit.
