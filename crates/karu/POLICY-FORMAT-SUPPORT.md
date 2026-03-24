# Policy Format Support

Karu supports importing from and interoperating with multiple policy formats. Full format support includes a fail-fast runtime loader and a dev-time tree-sitter parser built using `rust-sitter`. Each format has its own tracking document with detailed status.

## Formats

| Format                             | File     | Module                               | Maturity      |
| ---------------------------------- | -------- | ------------------------------------ | ------------- |
| [Cedar](CEDAR-SUPPORT-TRACKING.md) | `.cedar` | `cedar_parser.rs`, `cedar_import.rs` | 🟢 Production |

## Maturity Levels

- 🟢 **Production** - Full fail-fast runtime loader, full tree-sitter dev time parser, AST conversion, LSP support, VS Code extension
- 🟡 **Basic** - Text-level conversion or runtime data model, limited features
- 🔴 **Planned** - Not yet implemented

## Feature Matrix

| Capability              | Cedar                |
| ----------------------- | -------------------- |
| Proper parser           | ✅ Full grammar      |
| AST types               | ✅                   |
| → Karu AST import       | ✅ AST-to-AST        |
| Karu → export           | ✅ `transpile.rs`    |
| Compile & evaluate      | ✅ `compile_cedar()` |
| LSP diagnostics         | ✅                   |
| LSP document symbols    | ✅                   |
| VS Code highlighting    | ✅ TextMate grammar  |
| VS Code language config | ✅                   |
| Unit tests              | 25+                  |
