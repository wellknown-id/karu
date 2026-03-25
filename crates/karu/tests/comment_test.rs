/// After the fix, cedar_ts_parse_diagnostics should return no diagnostics
/// for source with valid Cedar + comments.
#[test]
fn cedar_ts_no_comment_errors() {
    let source = "// comment\npermit(principal, action, resource);\n";
    let diags = karu::lsp::cedar_ts_parse_diagnostics(source);
    assert!(diags.is_empty(), "expected no diagnostics for comments, got: {:?}", 
        diags.iter().map(|d| &d.message).collect::<Vec<_>>());
}

/// Comments should not produce errors in varying positions.
#[test]
fn cedar_ts_multiple_comment_styles() {
    let source = r#"
// Leading comment
permit(principal, action, resource); // inline comment
// Comment between policies
forbid(principal, action, resource);
// Trailing comment
"#;
    let diags = karu::lsp::cedar_ts_parse_diagnostics(source);
    assert!(diags.is_empty(), "expected no diagnostics for comments, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>());
}

/// Real errors should still be reported.
#[test]
fn cedar_ts_real_errors_still_reported() {
    let source = "permit(principal, action, resource) missing_semicolon\n";
    let diags = karu::lsp::cedar_ts_parse_diagnostics(source);
    assert!(!diags.is_empty(), "expected real parse errors to be reported");
}

/// Comment-only source should produce no errors.
#[test]
fn cedar_ts_comment_only() {
    let source = "// just a comment\n";
    let diags = karu::lsp::cedar_ts_parse_diagnostics(source);
    assert!(diags.is_empty(), "comment-only file should have no errors, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>());
}

/// Karu parse_diagnostics should also have no comment errors.
#[test]
fn karu_no_comment_errors() {
    let source = "// comment\nallow access;\n";
    let diags = karu::lsp::parse_diagnostics(source);
    assert!(diags.is_empty(), "expected no diagnostics for karu with comments, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>());
}
