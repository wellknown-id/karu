fn main() {
    println!("cargo:rerun-if-changed=src");

    // Only build tree-sitter parsers when the dev feature is enabled.
    // This avoids requiring clang for WASM builds that don't use tree-sitter.
    #[cfg(feature = "dev")]
    rust_sitter_tool::build_parser(&std::path::PathBuf::from("src/grammar.rs"));

    // Cedar-specific tree-sitter parsers (require both dev and cedar features)
    #[cfg(all(feature = "dev", feature = "cedar"))]
    {
        rust_sitter_tool::build_parser(&std::path::PathBuf::from("src/cedar_grammar.rs"));
        rust_sitter_tool::build_parser(&std::path::PathBuf::from("src/cedar_schema_grammar.rs"));
    }

    // Generate TextMate grammars from the same grammar sources
    #[cfg(feature = "dev")]
    {
        use rust_sitter_tool::TextMateBuilder;

        if let Some(json) = TextMateBuilder::default()
            .scope_name("karu")
            .build(&std::path::PathBuf::from("src/grammar.rs"))
        {
            let out = std::path::PathBuf::from("editors/vscode/syntaxes/karu.tmLanguage.json");
            std::fs::write(&out, serde_json::to_string_pretty(&json).unwrap()).unwrap();
        }
    }

    #[cfg(all(feature = "dev", feature = "cedar"))]
    {
        use rust_sitter_tool::TextMateBuilder;

        if let Some(json) = TextMateBuilder::default()
            .scope_name("cedar")
            .build(&std::path::PathBuf::from("src/cedar_grammar.rs"))
        {
            let out = std::path::PathBuf::from("editors/vscode/syntaxes/cedar.tmLanguage.json");
            std::fs::write(&out, serde_json::to_string_pretty(&json).unwrap()).unwrap();
        }
    }
}
