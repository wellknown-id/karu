# Contributing to Karu

## Quick Start

```bash
cargo build --workspace
cargo test --workspace
```

## WASM Builds

Browser/WASM builds for `crates/karu` are a separate feature path and should disable the native default features:

```bash
# Browser-facing WASM API
cargo check --target wasm32-unknown-unknown --manifest-path crates/karu/Cargo.toml --no-default-features --features wasm

# npm/playground package shape
cargo check --target wasm32-unknown-unknown --manifest-path crates/karu/Cargo.toml --no-default-features --features wasm,cedar
```

`--features wasm` on its own still combines with the crate's native default features, including `lsp`, which pulls in Tokio settings that are not supported on `wasm32-unknown-unknown`.

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

Language support for Karu policy files lives in [`editors/vscode/`](editors/vscode/).

Features include syntax highlighting, diagnostics, hover, completion, document symbols, go-to-definition, and semantic tokens.

### Quick Start (Development)

Open the workspace in VS Code and press **F5** to launch the Extension Development Host (builds both the LSP server and extension automatically).

### Manual Setup

```bash
# 1. Build the LSP server
cargo build --release --bin karu-lsp

# 2. Install extension dependencies
cd editors/vscode
npm install

# 3. Link extension to VS Code
ln -s "$(pwd)" ~/.vscode/extensions/karu
```

### Nightly Artifacts

The nightly GitHub Actions workflow publishes:

- `karu` CLI archives for Linux, macOS, and Windows on both x64 and arm64
- A self-contained VSIX that bundles `karu-lsp` for those same platforms
- npm package (WASM) via `wasm-pack` — published to npm with `--tag nightly`
- Native static libraries (`libkaru.a` + `karu.h`) for Go and Swift bindings
- Android shared libraries (`libkaru.so`) for arm64-v8a, armeabi-v7a, x86_64

### Language Bindings

| Language | Path | Usage |
| -------- | ---- | ----- |
| **Go** | `bindings/go/` | `import "github.com/wellknown-id/karu/bindings/go"` |
| **Swift** | `bindings/swift/` | SPM package wrapping `libkaru` via C FFI |
| **JavaScript** | `npm/karu/` | `npm install karu` (WASM) |

To list the Swift package on the [Swift Package Index](https://swiftpackageindex.com), [submit an issue here](https://github.com/SwiftPackageIndex/PackageList/issues/new?template=add_package.yml).

To build the FFI bindings locally:

```bash
# Build static lib + C header
cargo build -p karu --features ffi,cedar --no-default-features --release
# Header is written to crates/karu/include/karu.h
# Library is at target/release/libkaru.a
```

### Release Secrets

The following GitHub repository secrets must be configured for publishing:

| Secret | Purpose |
| ------ | ------- |
| `CARGO_REGISTRY_TOKEN` | crates.io API token for publishing `karu` and `karu-cli` |
| `NPM_TOKEN` | npm access token for publishing the WASM package |
| `VSCE_PAT` | VS Code Marketplace personal access token |
| `OVSX_TOKEN` | Open VSX Registry access token |

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
