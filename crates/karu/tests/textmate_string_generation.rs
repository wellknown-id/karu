// SPDX-License-Identifier: MIT

#![cfg(feature = "dev")]

use std::path::PathBuf;

use krust_sitter_tool::TextMateBuilder;
use serde_json::Value;

fn generated_karu_textmate() -> Value {
    TextMateBuilder::default()
        .scope_name("karu")
        .build(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/grammar.rs"))
        .expect("generated karu tmLanguage grammar")
}

fn field<'a>(value: &'a Value, key: &str) -> &'a Value {
    value
        .get(key)
        .unwrap_or_else(|| panic!("expected JSON field `{key}` in generated karu tmLanguage"))
}

#[test]
fn generated_strings_use_begin_end_and_escape_scopes() {
    let grammar = generated_karu_textmate();
    let repository = field(&grammar, "repository");
    let strings = field(repository, "strings");
    let patterns = field(strings, "patterns")
        .as_array()
        .expect("generated `repository.strings.patterns` should be an array");
    let string_rule = patterns
        .first()
        .expect("generated `repository.strings.patterns` should contain a string rule");
    let nested_patterns = field(string_rule, "patterns")
        .as_array()
        .expect("generated string rule `patterns` should be an array");
    let escape_rule = nested_patterns
        .first()
        .expect("generated string rule should contain an escape pattern");

    assert_eq!(field(string_rule, "begin"), "\"");
    assert_eq!(field(string_rule, "end"), "\"");
    assert_eq!(
        field(field(field(string_rule, "beginCaptures"), "0"), "name"),
        "punctuation.definition.string.begin.karu"
    );
    assert_eq!(
        field(field(field(string_rule, "endCaptures"), "0"), "name"),
        "punctuation.definition.string.end.karu"
    );
    assert_eq!(field(string_rule, "name"), "string.quoted.double.karu");
    assert!(string_rule.get("match").is_none());
    assert_eq!(field(escape_rule, "match"), r#"\\."#);
    assert_eq!(field(escape_rule, "name"), "constant.character.escape.karu");
}
