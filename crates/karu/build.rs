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
            let out = std::path::PathBuf::from("../../editors/vscode/syntaxes/karu.tmLanguage.json");
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
            let out = std::path::PathBuf::from("../../editors/vscode/syntaxes/cedar.tmLanguage.json");
            std::fs::write(&out, serde_json::to_string_pretty(&json).unwrap()).unwrap();
        }
    }

    // Generate C header via cbindgen when the ffi feature is enabled
    #[cfg(feature = "ffi")]
    {
        let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let out_dir = std::path::PathBuf::from(&crate_dir).join("include");
        std::fs::create_dir_all(&out_dir).unwrap();

        cbindgen::Builder::new()
            .with_crate(&crate_dir)
            .with_config(cbindgen::Config::from_file("cbindgen.toml").unwrap())
            .generate()
            .expect("Unable to generate C bindings")
            .write_to_file(out_dir.join("karu.h"));
    }
}
