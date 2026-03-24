# Karu

Karu is an embeddable policy engine focused on structural pattern matching over arbitrary JSON data.

This repository is organized as a small Cargo workspace:

- `crates/karu` — core engine
- `crates/karu-cli` — CLI binary (`karu`)
- `crates/karu-lsp` — language server (`karu-lsp`)

## Quick start

```bash
cargo build --workspace
cargo test --workspace
```

For editor tooling, see `crates/karu/editors/vscode/`.
