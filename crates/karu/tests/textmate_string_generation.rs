// SPDX-License-Identifier: MIT

#![cfg(feature = "dev")]

use std::path::PathBuf;

use serde_json::Value;

fn generated_karu_textmate() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../editors/vscode/syntaxes/karu.tmLanguage.json");
    let text = std::fs::read_to_string(path).expect("generated karu tmLanguage grammar");
    serde_json::from_str(&text).expect("valid karu tmLanguage json")
}

#[test]
fn generated_strings_use_begin_end_and_escape_scopes() {
    let grammar = generated_karu_textmate();
    let string_rule = &grammar["repository"]["strings"]["patterns"][0];

    assert_eq!(string_rule["begin"], "\"");
    assert_eq!(string_rule["end"], "\"");
    assert_eq!(
        string_rule["beginCaptures"]["0"]["name"],
        "punctuation.definition.string.begin.karu"
    );
    assert_eq!(
        string_rule["endCaptures"]["0"]["name"],
        "punctuation.definition.string.end.karu"
    );
    assert_eq!(string_rule["name"], "string.quoted.double.karu");
    assert_eq!(string_rule["patterns"][0]["match"], r#"\\."#);
    assert_eq!(
        string_rule["patterns"][0]["name"],
        "constant.character.escape.karu"
    );
}
