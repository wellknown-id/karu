# Contributing to Karu

## Quick Start

```bash
cargo build --workspace
cargo test --workspace
```

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

## Documentation

All project documentation lives in [`docs/`](docs/):

| Document                                                      | Description                             |
| ------------------------------------------------------------- | --------------------------------------- |
| [KARU.md](docs/KARU.md)                                       | Full language specification (EBNF)      |
| [INTERNALS.md](docs/INTERNALS.md)                             | Engine architecture and WASM API        |
| [BENCHMARKS.md](docs/BENCHMARKS.md)                           | Performance benchmarks vs Cedar         |
| [ROADMAP.md](docs/ROADMAP.md)                                 | Development roadmap                     |
| [CEDAR-COMPARISON.md](docs/CEDAR-COMPARISON.md)               | Cedar-to-Karu translation guide         |
| [CEDAR-SUPPORT-TRACKING.md](docs/CEDAR-SUPPORT-TRACKING.md)   | Cedar feature compatibility matrix      |
| [KNOWN-CEDAR-LIMITATIONS.md](docs/KNOWN-CEDAR-LIMITATIONS.md) | Current Cedar interop gaps              |
| [MODELING-ABAC.md](docs/MODELING-ABAC.md)                     | Attribute-based access control guide    |
| [MODELING-RBAC.md](docs/MODELING-RBAC.md)                     | Role-based access control guide         |
| [MODELING-REBAC.md](docs/MODELING-REBAC.md)                   | Relationship-based access control guide |
