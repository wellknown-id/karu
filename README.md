# Karu

Karu is an embeddable policy engine focusing on structural pattern matching over arbitrary JSON data. It is the spiritual successor to the ideas found in logic-based policy languages, designed to solve complex hierarchical data validation that strict-schema engines cannot handle.

## Core Philosophy

- **Structure over Schema**: We do not enforce a schema on input data. We enforce patterns on the data we find.
- **Search, Don't Index**: If you provide a list, we will search it. You shouldn't have to re-map your application data to fit the policy engine.
- **Partial Matching**: A pattern `{a: 1}` matches `{a: 1, b: 2}`.
- **Optionally Strict**: When you need Cedar-level rigor, flip a switch. Karu can enforce strict schemas, exhaustive matching, and static analysis - but only when you ask for it. RFCs shouldn't block good ideas.

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

The core library implementing the unification engine, parser, and pattern matching runtime.

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
