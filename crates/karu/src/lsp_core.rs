//! Portable LSP-like functionality for Karu.
//!
//! This module provides core language intelligence (diagnostics, hover,
//! completions, semantic tokens, inline test execution) **without** depending
//! on `tower-lsp` or `tokio`. It is always compiled (no feature gate) so it
//! can be used from the WASM playground build.

use crate::ast::{EffectAst, ExpectedOutcome};
use crate::compiler;
use crate::lexer::{Lexer, Token};
use crate::parser::{ParseError, Parser};
use crate::rule::Effect;

// ============================================================================
// Types
// ============================================================================

/// Severity level for a diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

/// A diagnostic message (parse error, lint warning, etc.).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LspDiagnostic {
    /// 0-indexed line number.
    pub line: u32,
    /// 0-indexed start column.
    pub col: u32,
    /// 0-indexed end column.
    pub end_col: u32,
    pub severity: Severity,
    pub message: String,
    /// Optional diagnostic code (e.g. lint rule ID).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// A completion item.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LspCompletion {
    pub label: String,
    pub detail: String,
    /// Text to insert (may contain snippet placeholders like `${1:name}`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insert_text: Option<String>,
    /// "keyword", "operator", "variable", "snippet"
    pub kind: String,
}

/// Semantic token types used by the Karu LSP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SemanticTokenType {
    Keyword,
    Function,
    Variable,
    Property,
    Operator,
    String,
    Number,
    Comment,
}

/// A semantic token with position and type info.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SemanticToken {
    /// 0-indexed line number.
    pub line: u32,
    /// 0-indexed start column.
    pub start: u32,
    pub length: u32,
    pub token_type: SemanticTokenType,
}

/// Result of running a single inline test.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TestResult {
    pub name: String,
    /// 0-indexed line where `test "name"` appears.
    pub line: u32,
    pub passed: bool,
    /// Failure message (empty string when passed).
    pub message: String,
}

/// Coverage status for a single rule.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RuleCoverage {
    pub name: String,
    /// 0-indexed source line of the rule.
    pub line: u32,
    pub has_positive: bool,
    pub has_negative: bool,
    /// "none", "partial", or "full"
    pub status: String,
}

/// Results from running inline tests, including coverage analysis.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InlineTestResults {
    pub tests: Vec<TestResult>,
    pub coverage: Vec<RuleCoverage>,
}

// ============================================================================
// Diagnostics
// ============================================================================

/// Maximum number of diagnostics to report.
const MAX_DIAGNOSTICS: usize = 5;

/// Parse a Karu policy and return lightweight diagnostics.
///
/// Uses the handrolled parser (no tree-sitter / `dev` feature dependency).
/// For the web playground this provides fast, always-available error reporting.
pub fn parse_diagnostics(source: &str) -> Vec<LspDiagnostic> {
    // Phase 1: detect unterminated string literals
    let string_errors = detect_unterminated_strings(source);
    if !string_errors.is_empty() {
        return string_errors;
    }

    // Phase 2: parse with tests to catch both syntax and semantic errors
    match Parser::parse_with_tests(source) {
        Ok(program) => {
            let mut diagnostics = Vec::new();

            // Semantic check: rule names in expect blocks
            let rule_names: std::collections::HashSet<&str> =
                program.rules.iter().map(|r| r.name.as_str()).collect();

            for test in &program.tests {
                if let ExpectedOutcome::PerRule(entries) = &test.expected {
                    for (_, rule_name) in entries {
                        if !rule_names.contains(rule_name.as_str()) {
                            if let Some((line, col)) =
                                find_ident_in_source(source, rule_name, &test.name)
                            {
                                diagnostics.push(LspDiagnostic {
                                    line: line as u32,
                                    col: col as u32,
                                    end_col: (col + rule_name.len()) as u32,
                                    severity: Severity::Error,
                                    message: format!(
                                        "Unknown rule '{}' in expect block",
                                        rule_name
                                    ),
                                    code: None,
                                });
                            }
                        }
                    }
                }
            }

            // Lint checks
            let lint_warnings = crate::lint::lint(&program);
            let lines: Vec<&str> = source.lines().collect();
            for warning in lint_warnings {
                let (line, col, end_col) = find_forall_in_rule(
                    source,
                    &lines,
                    &warning.rule_name,
                    warning.forall_path.as_deref(),
                )
                .unwrap_or((0, 0, 6));

                diagnostics.push(LspDiagnostic {
                    line: line as u32,
                    col: col as u32,
                    end_col: end_col as u32,
                    severity: Severity::Warning,
                    message: warning.message.clone(),
                    code: Some(warning.code.to_string()),
                });
            }

            diagnostics.truncate(MAX_DIAGNOSTICS);
            diagnostics
        }
        Err(e) => {
            vec![parse_error_to_diagnostic(&e)]
        }
    }
}

/// Convert a `ParseError` to an `LspDiagnostic`.
fn parse_error_to_diagnostic(e: &ParseError) -> LspDiagnostic {
    let line = if e.line > 0 { e.line - 1 } else { 0 };
    let col = if e.column > 0 { e.column - 1 } else { 0 };
    LspDiagnostic {
        line: line as u32,
        col: col as u32,
        end_col: (col + 1) as u32,
        severity: Severity::Error,
        message: e.message.clone(),
        code: None,
    }
}

/// Detect unterminated string literals — these cause massive cascade errors
/// from any parser, so we catch them first.
fn detect_unterminated_strings(source: &str) -> Vec<LspDiagnostic> {
    let mut diags = Vec::new();
    for (line_idx, line) in source.lines().enumerate() {
        let bytes = line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'"' {
                let start = i;
                i += 1;
                let mut closed = false;
                while i < bytes.len() {
                    if bytes[i] == b'\\' && i + 1 < bytes.len() {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == b'"' {
                        closed = true;
                        i += 1;
                        break;
                    }
                    i += 1;
                }
                if !closed {
                    diags.push(LspDiagnostic {
                        line: line_idx as u32,
                        col: start as u32,
                        end_col: bytes.len() as u32,
                        severity: Severity::Error,
                        message: "Unterminated string literal".to_string(),
                        code: None,
                    });
                }
            } else if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                // Rest of line is a comment — skip
                break;
            } else {
                i += 1;
            }
        }
    }
    diags
}

/// Find the position of an identifier within a test block (for expect diagnostics).
fn find_ident_in_source(source: &str, ident: &str, test_name: &str) -> Option<(usize, usize)> {
    let mut in_test = false;
    for (line_idx, line) in source.lines().enumerate() {
        if line.contains("test") && line.contains(&format!(r#""{}""#, test_name)) {
            in_test = true;
        }
        if in_test {
            if let Some(col) = line.find(ident) {
                // Ensure it's a whole word (not substring of another identifier)
                let before_ok = col == 0
                    || !line.as_bytes()[col - 1].is_ascii_alphanumeric()
                        && line.as_bytes()[col - 1] != b'_';
                let after = col + ident.len();
                let after_ok = after >= line.len()
                    || !line.as_bytes()[after].is_ascii_alphanumeric()
                        && line.as_bytes()[after] != b'_';
                if before_ok && after_ok {
                    return Some((line_idx, col));
                }
            }
        }
    }
    None
}

/// Find the position of a `forall` keyword within a specific rule (for lint diagnostics).
fn find_forall_in_rule(
    _source: &str,
    lines: &[&str],
    rule_name: &str,
    forall_path: Option<&str>,
) -> Option<(usize, usize, usize)> {
    // Find the start of the rule
    let rule_start = lines.iter().enumerate().find(|(_, line)| {
        line.contains(&format!("allow {}", rule_name))
            || line.contains(&format!("deny {}", rule_name))
    });

    let start_idx = match rule_start {
        Some((idx, _)) => idx,
        None => return None,
    };

    // Find the forall keyword in or after the rule definition
    for (i, line) in lines.iter().enumerate().skip(start_idx) {
        if let Some(col) = line.find("forall") {
            // If a specific path is given, check this is the right forall
            if let Some(path) = forall_path {
                if !line.contains(path) {
                    continue;
                }
            }
            return Some((i, col, col + 6));
        }
    }
    None
}

// ============================================================================
// Hover
// ============================================================================

/// Get hover documentation for a Karu keyword.
///
/// Returns markdown-formatted documentation if the word is a known keyword.
pub fn keyword_hover(word: &str) -> Option<&'static str> {
    match word {
        "allow" => Some("**allow** - Declares a rule that permits access when conditions match."),
        "deny" => Some("**deny** - Declares a rule that denies access when conditions match. Deny rules take precedence over allow rules."),
        "if" => Some("**if** - Introduces the condition clause of a rule."),
        "and" => Some("**and** - Logical AND operator. All conditions must be true."),
        "or" => Some("**or** - Logical OR operator. At least one condition must be true."),
        "not" => Some("**not** - Logical NOT operator. Negates the following condition."),
        "in" => Some("**in** - Membership operator. Checks if a pattern exists in a collection."),
        "forall" => Some("**forall** - Universal quantifier. Checks if all items in a collection match a condition."),
        "exists" => Some("**exists** - Existential quantifier. Checks if any item in a collection matches a condition. Supports variable binding: `exists x in path: condition[x]`."),
        "true" => Some("**true** - Boolean literal."),
        "false" => Some("**false** - Boolean literal."),
        "null" => Some("**null** - Null literal."),
        "principal" => Some("**principal** - The entity making the request (e.g., a user)."),
        "action" => Some("**action** - The operation being requested (e.g., \"read\", \"write\")."),
        "resource" => Some("**resource** - The target of the operation (e.g., a document, folder)."),
        "context" => Some("**context** - Request-time attributes (e.g., IP address, time, device)."),
        "test" => Some("**test** - Declares an inline test block. Tests are run with `karu test` and shown live in the editor."),
        "expect" => Some("**expect** - Specifies the expected effect (`allow` or `deny`) for a test case."),
        "has" => Some("**has** - Attribute existence check. Returns true if the path exists in the input."),
        "like" => Some("**like** - Glob-style pattern matching operator for string values."),
        "is" => Some("**is** - Type check operator. Verifies an entity's type."),
        "use" => Some("**use** - Enables a feature for the file, e.g. `use schema;`."),
        "schema" => Some("**schema** - Schema mode. When enabled with `use schema;`, activates type-checked entity access."),
        "mod" => Some("**mod** - Declares a schema module containing entity and action definitions."),
        "import" => Some("**import** - Imports definitions from another file."),
        "assert" => Some("**assert** - Declares a static assertion that must hold for all evaluations."),
        _ => None,
    }
}

// ============================================================================
// Completions
// ============================================================================

/// Get completion items for Karu keywords (portable, no tower-lsp types).
pub fn keyword_completions() -> Vec<LspCompletion> {
    vec![
        LspCompletion {
            label: "allow".into(),
            detail: "Permit access rule".into(),
            insert_text: Some("allow ${1:rule_name} if\n    ${2:condition};".into()),
            kind: "keyword".into(),
        },
        LspCompletion {
            label: "deny".into(),
            detail: "Deny access rule".into(),
            insert_text: Some("deny ${1:rule_name} if\n    ${2:condition};".into()),
            kind: "keyword".into(),
        },
        LspCompletion {
            label: "if".into(),
            detail: "Condition clause".into(),
            insert_text: None,
            kind: "keyword".into(),
        },
        LspCompletion {
            label: "and".into(),
            detail: "Logical AND".into(),
            insert_text: None,
            kind: "operator".into(),
        },
        LspCompletion {
            label: "or".into(),
            detail: "Logical OR".into(),
            insert_text: None,
            kind: "operator".into(),
        },
        LspCompletion {
            label: "not".into(),
            detail: "Logical NOT".into(),
            insert_text: None,
            kind: "operator".into(),
        },
        LspCompletion {
            label: "in".into(),
            detail: "Membership check".into(),
            insert_text: None,
            kind: "operator".into(),
        },
        LspCompletion {
            label: "forall".into(),
            detail: "Universal quantifier".into(),
            insert_text: Some("forall ${1:x} in ${2:collection}: ${3:condition}".into()),
            kind: "keyword".into(),
        },
        LspCompletion {
            label: "exists".into(),
            detail: "Existential quantifier".into(),
            insert_text: Some("exists ${1:x} in ${2:collection}: ${3:condition}".into()),
            kind: "keyword".into(),
        },
        LspCompletion {
            label: "principal".into(),
            detail: "Request principal".into(),
            insert_text: None,
            kind: "variable".into(),
        },
        LspCompletion {
            label: "action".into(),
            detail: "Request action".into(),
            insert_text: None,
            kind: "variable".into(),
        },
        LspCompletion {
            label: "resource".into(),
            detail: "Request resource".into(),
            insert_text: None,
            kind: "variable".into(),
        },
        LspCompletion {
            label: "context".into(),
            detail: "Request context".into(),
            insert_text: None,
            kind: "variable".into(),
        },
        LspCompletion {
            label: "test".into(),
            detail: "Inline test block".into(),
            insert_text: Some(
                "test \"${1:test name}\" {\n    ${2:input_json}\n    expect ${3:allow}\n}".into(),
            ),
            kind: "snippet".into(),
        },
        LspCompletion {
            label: "has".into(),
            detail: "Attribute existence check".into(),
            insert_text: Some("has ${1:path}".into()),
            kind: "keyword".into(),
        },
    ]
}

// ============================================================================
// Semantic Tokens
// ============================================================================

/// Get the display length of a token.
fn token_length(token: &Token) -> u32 {
    match token {
        Token::Allow => 5,
        Token::Deny => 4,
        Token::If => 2,
        Token::And => 3,
        Token::Or => 2,
        Token::Not => 3,
        Token::In => 2,
        Token::Forall => 6,
        Token::Exists => 6,
        Token::Has => 3,
        Token::Like_ => 4,
        Token::True => 4,
        Token::False => 5,
        Token::Null => 4,
        Token::Use => 3,
        Token::Schema => 6,
        Token::Mod => 3,
        Token::Actor => 5,
        Token::Resource => 8,
        Token::Action => 6,
        Token::Assert => 6,
        Token::Abstract => 8,
        Token::Import => 6,
        Token::On => 2,
        Token::Is => 2,
        Token::Test => 4,
        Token::Expect => 6,
        Token::Ident(s) => s.len() as u32,
        Token::String(s) => (s.len() + 2) as u32,
        Token::Number(n) => format!("{}", n).len() as u32,
        Token::Eq | Token::Ne | Token::Le | Token::Ge => 2,
        Token::Lt | Token::Gt => 1,
        Token::Comment(s) => (s.len() + 3) as u32,
        _ => 1,
    }
}

/// Generate semantic tokens for a Karu source file.
///
/// Uses the lexer to tokenize and map each token to a semantic type.
/// Includes comments which are normally stripped by the parser.
pub fn semantic_tokens(source: &str) -> Vec<SemanticToken> {
    let mut result = Vec::new();
    let mut lexer = Lexer::new(source);

    while let Ok(spanned) = lexer.next_spanned() {
        if spanned.token == Token::Eof {
            break;
        }

        let tt = match &spanned.token {
            // Keywords
            Token::Allow
            | Token::Deny
            | Token::If
            | Token::And
            | Token::Or
            | Token::Not
            | Token::In
            | Token::Forall
            | Token::Exists
            | Token::True
            | Token::False
            | Token::Null
            | Token::Use
            | Token::Schema
            | Token::Mod
            | Token::Assert
            | Token::Import
            | Token::On
            | Token::Is
            | Token::Test
            | Token::Expect => Some(SemanticTokenType::Keyword),

            // Schema keywords that are also Cedar variables
            Token::Actor | Token::Resource | Token::Action => Some(SemanticTokenType::Variable),

            // Identifiers
            Token::Ident(name) => match name.as_str() {
                "principal" | "context" => Some(SemanticTokenType::Variable),
                _ => Some(SemanticTokenType::Property),
            },

            // Literals
            Token::String(_) => Some(SemanticTokenType::String),
            Token::Number(_) => Some(SemanticTokenType::Number),

            // Operators
            Token::Eq | Token::Ne | Token::Lt | Token::Gt | Token::Le | Token::Ge => {
                Some(SemanticTokenType::Operator)
            }

            // Comments
            Token::Comment(_) => Some(SemanticTokenType::Comment),

            // Delimiters and others — skip
            _ => None,
        };

        if let Some(token_type) = tt {
            let length = token_length(&spanned.token);
            result.push(SemanticToken {
                line: (spanned.line - 1) as u32,
                start: (spanned.column - 1) as u32,
                length,
                token_type,
            });
        }
    }

    result
}

// ============================================================================
// Inline Test Runner
// ============================================================================

/// Run inline tests in a Karu source file and return results with coverage.
///
/// Returns None if there are no tests or if parsing/compilation fails.
pub fn run_inline_tests(source: &str) -> Option<InlineTestResults> {
    // Parse with tests
    let program = match Parser::parse_with_tests(source) {
        Ok(p) => p,
        Err(_) => return None,
    };

    if program.tests.is_empty() {
        return None;
    }

    // Compile the policy (ignoring test blocks)
    let compiled = match compiler::compile(source) {
        Ok(c) => c,
        Err(_) => return None,
    };

    let lines: Vec<&str> = source.lines().collect();

    // Build test inputs
    let mut test_inputs = Vec::new();
    for test in &program.tests {
        let mut flat = serde_json::Map::new();
        for entity in &test.entities {
            let mut id_value = None;
            for (key, value) in &entity.fields {
                if key == "id" {
                    id_value = Some(value.clone());
                }
                flat.insert(format!("{}.{}", entity.kind, key), value.clone());
            }
            if let Some(id) = id_value {
                flat.insert(entity.kind.clone(), id);
            }
        }
        test_inputs.push(serde_json::Value::Object(flat));
    }

    // Run tests
    let mut results = Vec::new();
    for (test, input) in program.tests.iter().zip(test_inputs.iter()) {
        let test_line = lines
            .iter()
            .enumerate()
            .find(|(_, line)| {
                line.contains("test") && line.contains(&format!(r#""{}""#, test.name))
            })
            .map(|(i, _)| i as u32)
            .unwrap_or(0);

        let result = compiled.evaluate(input);
        let (passed, message) = match &test.expected {
            ExpectedOutcome::Simple(eff) => {
                let expected = match eff {
                    EffectAst::Allow => Effect::Allow,
                    EffectAst::Deny => Effect::Deny,
                };
                let p = result == expected;
                let m = if p {
                    String::new()
                } else {
                    format!("expected {:?}, got {:?}", expected, result)
                };
                (p, m)
            }
            ExpectedOutcome::PerRule(entries) => {
                let mut msgs = Vec::new();
                let mut all_pass = true;
                for (eff, rule_name) in entries {
                    let expected_eff = match eff {
                        EffectAst::Allow => Effect::Allow,
                        EffectAst::Deny => Effect::Deny,
                    };
                    if let Some(rule) = compiled.rules.iter().find(|r| r.name == *rule_name) {
                        let rule_result = if rule.evaluate(input).is_some() {
                            rule.effect
                        } else {
                            Effect::Deny
                        };
                        if rule_result != expected_eff {
                            msgs.push(format!(
                                "rule '{}': expected {:?}, got {:?}",
                                rule_name, expected_eff, rule_result
                            ));
                            all_pass = false;
                        }
                    } else {
                        msgs.push(format!("rule '{}' not found", rule_name));
                        all_pass = false;
                    }
                }
                (all_pass, msgs.join("; "))
            }
        };

        results.push(TestResult {
            name: test.name.clone(),
            line: test_line,
            passed,
            message,
        });
    }

    // Compute per-rule coverage
    let mut coverage = Vec::new();
    for rule in &compiled.rules {
        let rule_line = lines
            .iter()
            .enumerate()
            .find(|(_, line)| {
                line.contains(&format!("allow {}", rule.name))
                    || line.contains(&format!("deny {}", rule.name))
            })
            .map(|(i, _)| i as u32)
            .unwrap_or(0);

        let mut has_positive = false;
        let mut has_negative = false;

        for (_test, input) in program.tests.iter().zip(test_inputs.iter()) {
            if rule.evaluate(input).is_some() {
                has_positive = true;
            } else {
                has_negative = true;
            }
        }

        let status = match (has_positive, has_negative) {
            (true, true) => "full",
            (true, false) | (false, true) => "partial",
            (false, false) => "none",
        };

        coverage.push(RuleCoverage {
            name: rule.name.clone(),
            line: rule_line,
            has_positive,
            has_negative,
            status: status.to_string(),
        });
    }

    Some(InlineTestResults {
        tests: results,
        coverage,
    })
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyword_hover_allow() {
        let hover = keyword_hover("allow");
        assert!(hover.is_some());
        assert!(hover.unwrap().contains("permit"));
    }

    #[test]
    fn test_keyword_hover_unknown() {
        assert!(keyword_hover("foobar").is_none());
    }

    #[test]
    fn test_parse_diagnostics_valid() {
        let source = r#"allow access if role == "admin";"#;
        let diags = parse_diagnostics(source);
        assert!(diags.is_empty(), "expected no diagnostics, got: {:?}", diags);
    }

    #[test]
    fn test_parse_diagnostics_invalid() {
        let source = "allow access if;";
        let diags = parse_diagnostics(source);
        assert!(!diags.is_empty(), "expected diagnostics for invalid policy");
        assert_eq!(diags[0].severity, Severity::Error);
    }

    #[test]
    fn test_parse_diagnostics_unterminated_string() {
        let source = r#"allow access if role == "admin;"#;
        let diags = parse_diagnostics(source);
        assert!(!diags.is_empty());
        assert!(diags[0].message.contains("Unterminated string"));
    }

    #[test]
    fn test_keyword_completions_not_empty() {
        let completions = keyword_completions();
        assert!(!completions.is_empty());
        assert!(completions.iter().any(|c| c.label == "allow"));
        assert!(completions.iter().any(|c| c.label == "deny"));
        assert!(completions.iter().any(|c| c.label == "forall"));
    }

    #[test]
    fn test_semantic_tokens_basic() {
        let source = r#"allow access if role == "admin";"#;
        let tokens = semantic_tokens(source);
        assert!(!tokens.is_empty());
        // First token should be "allow" keyword
        assert_eq!(tokens[0].token_type, SemanticTokenType::Keyword);
        assert_eq!(tokens[0].start, 0);
        assert_eq!(tokens[0].length, 5);
    }

    #[test]
    fn test_semantic_tokens_comment() {
        let source = "// this is a comment\nallow access;";
        let tokens = semantic_tokens(source);
        let comment = tokens.iter().find(|t| t.token_type == SemanticTokenType::Comment);
        assert!(comment.is_some(), "should find a comment token");
    }

    #[test]
    fn test_run_inline_tests_pass() {
        let source = r#"
allow view if
    principal == "alice" and
    action == "view";

test "alice can view" {
    principal {
        id: "alice",
    }
    action {
        id: "view",
    }
    resource {
        id: "doc1",
    }
    expect allow
}
"#;
        let results = run_inline_tests(source);
        assert!(results.is_some());
        let results = results.unwrap();
        assert_eq!(results.tests.len(), 1);
        assert!(results.tests[0].passed, "test should pass: {}", results.tests[0].message);
    }

    #[test]
    fn test_run_inline_tests_no_tests() {
        let source = r#"allow access if role == "admin";"#;
        assert!(run_inline_tests(source).is_none());
    }
}
