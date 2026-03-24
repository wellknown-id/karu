# karu-lsp

Language server for the Karu policy engine.

The `karu-lsp` crate provides IDE and editor support for the Karu policy language.

## Key Features

- **Policy Diagnostics**: Real-time syntax and semantic validation of Karu policy files.
- **Integration with Karu Engine**: Utilizes the official Karu engine to ensure that editor diagnostics match the runtime behavior.
- **Standard LSP Protocol**: Leverages `tower-lsp` for a consistent experience across different editors.
- **Structural Analysis**: Provides feedback on Karu's structural pattern matching rules and policy logic.

## Components

- `main.rs`: The LSP server entry point and message handling logic.
