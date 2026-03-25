# Karu VS Code Extension

Language support for Karu policy files, including syntax highlighting and LSP integration.

## Installation

### Quick Start (Development)

```bash
# 1. Build the LSP server
cd /path/to/karu
cargo build --release --bin karu-lsp

# 2. Install extension dependencies
cd crates/karu/editors/vscode
npm install

# 3. Link extension to VS Code
ln -s "$(pwd)" ~/.vscode/extensions/karu
```

### Package as VSIX

```bash
cd crates/karu/editors/vscode
npx @vscode/vsce package --allow-star-activation
code --install-extension karu-0.1.0.vsix
```

The nightly release workflow builds a self-contained VSIX by staging `karu-lsp`
for Linux, macOS, and Windows on both x64 and arm64 before packaging.

To reproduce that packaging flow locally, build each release `karu-lsp` target
under the workspace `target/` directory and then run:

```bash
cd crates/karu/editors/vscode
npm run build:vsix
npx @vscode/vsce package --allow-star-activation
```

## Configuration

| Setting           | Description                                         |
| ----------------- | --------------------------------------------------- |
| `karu.serverPath` | Path to `karu-lsp` binary. Leave empty to use the bundled server, local builds, or PATH. |

Example `settings.json`:

```json
{
  "karu.serverPath": "/path/to/karu/target/release/karu-lsp"
}
```

## Features

- **Syntax Highlighting** - TextMate grammar for `.karu` files
- **Diagnostics** - Parse errors with line/column
- **Hover** - Documentation for keywords
- **Completion** - Keywords with snippets
- **Document Symbols** - Rule outline (Ctrl+Shift+O)
- **Go to Definition** - Jump to rule (F12)
- **Semantic Tokens** - Rich syntax highlighting via LSP

## Development

Open the `karu` workspace in VS Code and press F5 to launch the Extension Development Host.
