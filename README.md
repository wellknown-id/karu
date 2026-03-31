# Karu

<p align="center">
  <img src="assets/karu-hero-1280px-640px.png" alt="Karu - Structural Pattern Matching for JSON" width="640" />
</p>

Karu is an embeddable policy engine focusing on structural pattern matching over arbitrary JSON data. It is the spiritual successor to the ideas found in logic-based policy languages, designed to solve complex hierarchical data validation that strict-schema engines cannot handle.

It came to be because a coming soon wellknown.id project needed an expressive policy engine as fast as Cedar. That internal project started with Cedar until we accidentally wrote an engine that ran **much faster at Cedar than Cedar!**. _And so it stuck._

## How Fast?

| Benchmark                    | Karu              | Cedar        |                  |
| ---------------------------- | ----------------- | ------------ | ---------------- |
| WASM bundle size             | **319 KB**        | 1.8 MB       | **5.6× smaller** |
| Native eval (1 rule)         | **16 ns**         | 1,162 ns     | **71× faster**   |
| WASM eval (precompiled)      | **459 ns**        | 94,054 ns    | **205× faster**  |
| Complex (20 rules, native)   | **649 ns**        | 13,749 ns    | **21× faster**   |
| Realistic authz (16 threads) | **10.6M ops/sec** | 270K ops/sec | **39× faster**   |

> Karu running in WASM (459 ns) is faster than Cedar running natively (1,162 ns). [Full benchmarks →](crates/karu/BENCHMARKS.md)

## Core Philosophy

- **Structure over Schema**: We do not enforce a schema on input data. We enforce patterns on the data we find.
- **Search, Don't Index**: If you provide a list, we will search it. You shouldn't have to re-map your application data to fit the policy engine.
- **Partial Matching**: A pattern `{a: 1}` matches `{a: 1, b: 2}`.
- **Optionally Strict**: When you need Cedar-level rigor, flip a switch. Karu can enforce strict schemas, exhaustive matching, and static analysis - but only when you ask for it. RFCs shouldn't block good ideas.

## Karu Language

### Rules

Rules are `allow` or `deny`, with an optional `if` body:

```polar
allow public_access;

allow view if
    principal.role == "viewer" and
    action == "read";

deny delete if
    action == "delete" and
    not principal.role == "admin";
```

### Conditions & Operators

| Operator          | Example                  | Description              |
| ----------------- | ------------------------ | ------------------------ |
| `==` `!=`         | `action == "read"`       | Equality / inequality    |
| `<` `<=` `>` `>=` | `principal.age >= 18`    | Numeric comparison       |
| `and` `or` `not`  | `a == 1 and not b == 2`  | Logical combinators      |
| `in`              | `"editor" in user.roles` | Collection search        |
| `is`              | `actor is User`          | Type guard (schema mode) |
| `has`             | `resource has owner`     | Field existence check    |

### Pattern Matching

The `in` operator searches arrays with structural patterns — extra fields are ignored:

```polar
allow access if
    { name: "lhs", value: 10 } in resource.context.namedArguments;
```

Patterns can be literals (`"alice"`, `42`, `true`, `null`), wildcards (`_`), objects (`{ key: value }`), or arrays (`[1, 2]`).

### Quantifiers

```polar
# Every item must match
allow bulk_read if
    forall item in resource.items: item.public == true;

# At least one item must match
allow has_permission if
    exists perm in user.permissions: perm.action == "write";
```

### Schema Mode

Opt into strict typing with `use schema;` and `mod` blocks:

```polar
use schema;

mod MyApp {
    actor User { name String, role String };
    resource Document in Folder { owner User, title String };
    action "Delete" appliesTo { actor User, resource Document };
};

assert is_owner<User, action, Document> if actor.name == resource.owner.name;

allow delete if MyApp:Delete and resource is Document and is_owner;
```

### Inline Tests

Policies can declare tests alongside rules:

```polar
test "alice can view" {
    principal { id: "alice" }
    action { id: "view" }
    expect allow
}
```

### Multi-File Projects

```polar
import "shared/roles.karu";
import "rules.karu";
```

For architecture details, see [docs/INTERNALS.md](docs/INTERNALS.md).

## Comparison

| Feature          | Cedar              | Rego                | Karu                      |
| ---------------- | ------------------ | ------------------- | ------------------------- |
| List Search      | ❌ (Strict Schema) | ✅ (Complex syntax) | ✅ (Native `in` operator) |
| Pattern Matching | ❌                 | ✅                  | ✅                        |
| Strict Mode      | ✅ (Usually)       | ❌                  | ✅ (Optional)             |
| Duck Typing      | ❌                 | ✅                  | ✅                        |
| Syntax           | SQL-like           | Datalog             | Polar-like                |
| Focus            | Performance/Safety | Infrastructure      | Why not both?             |

## Cedar Interop

Karu supports full round-trip conversion with [Cedar](https://www.cedarpolicy.com/) policies and schemas. See [Known Cedar Limitations](crates/karu/KNOWN-CEDAR-LIMITATIONS.md) for current gaps.

Even when running Cedar policies through Karu's import pipeline, evaluation is **fast** - Karu's native engine evaluates at ~19 million ops/sec, roughly 10× faster than the Cedar-WASM runtime for equivalent policies. For detailed numbers, see [BENCHMARKS.md](crates/karu/BENCHMARKS.md).

## Workspace Layout

This repository is organized as a Cargo workspace:

### `crates/karu` - Core Engine

The core library implementing the structural matcher, parser, and pattern matching runtime.

### `crates/karu-cli` - CLI

A command-line tool for interacting with the Karu policy engine, used for testing and managing access control policies.

### `crates/karu-lsp` - Language Server

IDE and editor support for the Karu policy language via the LSP protocol.

- **Policy Diagnostics**: Real-time syntax and semantic validation of Karu policy files.
- **Integration with Karu Engine**: Editor diagnostics match the runtime behavior.
- **Standard LSP Protocol**: Built on `tower-lsp` for a consistent experience across editors.
- **Structural Analysis**: Feedback on Karu's structural pattern matching rules and policy logic.

## VS Code Extension

Language support for Karu policy files lives in [`crates/karu/editors/vscode/`](crates/karu/editors/vscode/).

Features include syntax highlighting, diagnostics, hover, completion, document symbols, go-to-definition, and semantic tokens.

### Quick Start (Development)

Open the workspace in VS Code and press **F5** to launch the Extension Development Host (builds both the LSP server and extension automatically).

### Manual Setup

```bash
# 1. Build the LSP server
cargo build --release --bin karu-lsp

# 2. Install extension dependencies
cd crates/karu/editors/vscode
npm install

# 3. Link extension to VS Code
ln -s "$(pwd)" ~/.vscode/extensions/karu
```

### Nightly Artifacts

The nightly GitHub Actions workflow publishes:

- `karu` CLI archives for Linux, macOS, and Windows on both x64 and arm64.
- A self-contained VSIX that bundles `karu-lsp` for those same platforms.

### Configuration

| Setting           | Description                                         |
| ----------------- | --------------------------------------------------- |
| `karu.serverPath` | Path to `karu-lsp` binary. Leave empty to use PATH. |

## Integration with Kodus

Karu is the core authorization engine for the Kodus platform. It is utilized by the `kodus-authz` crate to enforce fine-grained access control policies across the distributed runtime.

## Quick Start

```bash
cargo build --workspace
cargo test --workspace
```
