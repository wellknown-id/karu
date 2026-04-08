// SPDX-License-Identifier: MIT

//! Language Server Protocol support for Karu.
//!
//! This module provides core LSP functionality that can be used by the
//! `karu-lsp` binary and tested independently.

use tower_lsp::lsp_types::*;

use crate::ast::{ExpectedOutcome, RuleAst};
use crate::grammar;
use crate::lexer::{Lexer, Token};
use crate::parser::ParseError;
use krust_sitter::error as ts_errors;
use krust_sitter::Language;

/// Semantic token types used by the Karu LSP.
/// The order must match SEMANTIC_TOKEN_TYPES.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum SemanticTokenType {
    Keyword = 0,
    Function = 1,
    Variable = 2,
    Property = 3,
    Operator = 4,
    String = 5,
    Number = 6,
    Comment = 7,
}

/// Token types for the semantic tokens legend (order matters!).
pub const SEMANTIC_TOKEN_TYPES: &[SemanticTokenTypeName] = &[
    SemanticTokenTypeName::Keyword,
    SemanticTokenTypeName::Function,
    SemanticTokenTypeName::Variable,
    SemanticTokenTypeName::Property,
    SemanticTokenTypeName::Operator,
    SemanticTokenTypeName::String,
    SemanticTokenTypeName::Number,
    SemanticTokenTypeName::Comment,
];

/// Semantic token type names for compatibility with LSP.
#[derive(Debug, Clone, Copy)]
pub enum SemanticTokenTypeName {
    Keyword,
    Function,
    Variable,
    Property,
    Operator,
    String,
    Number,
    Comment,
}

impl SemanticTokenTypeName {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Keyword => "keyword",
            Self::Function => "function",
            Self::Variable => "variable",
            Self::Property => "property",
            Self::Operator => "operator",
            Self::String => "string",
            Self::Number => "number",
            Self::Comment => "comment",
        }
    }
}

/// A semantic token with position and type info.
#[derive(Debug, Clone)]
pub struct SemanticToken {
    pub line: u32,  // 0-indexed
    pub start: u32, // start column, 0-indexed
    pub length: u32,
    pub token_type: SemanticTokenType,
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

        let token_type = match &spanned.token {
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

            // Identifiers - check if it's a known variable
            Token::Ident(name) => {
                match name.as_str() {
                    "principal" | "context" => Some(SemanticTokenType::Variable),
                    _ => Some(SemanticTokenType::Property), // Other identifiers are properties/fields
                }
            }

            // Literals
            Token::String(_) => Some(SemanticTokenType::String),
            Token::Number(_) => Some(SemanticTokenType::Number),

            // Operators
            Token::Eq | Token::Ne | Token::Lt | Token::Gt | Token::Le | Token::Ge => {
                Some(SemanticTokenType::Operator)
            }

            // Comments
            Token::Comment(_) => Some(SemanticTokenType::Comment),

            // Delimiters and others - skip
            _ => None,
        };

        if let Some(tt) = token_type {
            let length = token_length(&spanned.token);
            result.push(SemanticToken {
                line: (spanned.line - 1) as u32,    // Convert to 0-indexed
                start: (spanned.column - 1) as u32, // Convert to 0-indexed
                length,
                token_type: tt,
            });
        }
    }

    result
}

/// Generate semantic tokens for a Cedar source file.
///
/// Uses a lightweight inline tokenizer for Cedar keywords, scope variables,
/// identifiers, strings, numbers, operators, and `//` line comments.
#[cfg(all(feature = "dev", feature = "cedar"))]
pub fn cedar_semantic_tokens(source: &str) -> Vec<SemanticToken> {
    let mut result = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut line: u32 = 0;
    let mut col: u32 = 0;

    while i < bytes.len() {
        let b = bytes[i];

        // Line comment: // ... \n
        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            let start_col = col;
            let start = i;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
                col += 1;
            }
            result.push(SemanticToken {
                line,
                start: start_col,
                length: (i - start) as u32,
                token_type: SemanticTokenType::Comment,
            });
            continue;
        }

        // Newline
        if b == b'\n' {
            i += 1;
            line += 1;
            col = 0;
            continue;
        }

        // Whitespace
        if b.is_ascii_whitespace() {
            i += 1;
            col += 1;
            continue;
        }

        // String literal
        if b == b'"' {
            let start_col = col;
            let start = i;
            i += 1;
            col += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    col += 2;
                } else {
                    if bytes[i] == b'\n' {
                        line += 1;
                        col = 0;
                    } else {
                        col += 1;
                    }
                    i += 1;
                }
            }
            if i < bytes.len() {
                i += 1; // closing "
                col += 1;
            }
            result.push(SemanticToken {
                line,
                start: start_col,
                length: (i - start) as u32,
                token_type: SemanticTokenType::String,
            });
            continue;
        }

        // Number
        if b.is_ascii_digit() {
            let start_col = col;
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
                col += 1;
            }
            result.push(SemanticToken {
                line,
                start: start_col,
                length: (i - start) as u32,
                token_type: SemanticTokenType::Number,
            });
            continue;
        }

        // Identifier or keyword
        if b.is_ascii_alphabetic() || b == b'_' {
            let start_col = col;
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
                col += 1;
            }
            let word = &source[start..i];
            let token_type = match word {
                "permit" | "forbid" | "when" | "unless" | "if" | "then" | "else" | "in"
                | "like" | "has" | "is" | "true" | "false" => SemanticTokenType::Keyword,
                "principal" | "action" | "resource" | "context" => SemanticTokenType::Variable,
                _ => SemanticTokenType::Property,
            };
            result.push(SemanticToken {
                line,
                start: start_col,
                length: (i - start) as u32,
                token_type,
            });
            continue;
        }

        // Multi-char operators
        if i + 1 < bytes.len() {
            let two = &source[i..i + 2];
            if matches!(two, "==" | "!=" | "<=" | ">=" | "&&" | "||" | "::") {
                result.push(SemanticToken {
                    line,
                    start: col,
                    length: 2,
                    token_type: SemanticTokenType::Operator,
                });
                i += 2;
                col += 2;
                continue;
            }
        }

        // Single-char operators / punctuation — skip (let TextMate handle)
        i += 1;
        col += 1;
    }

    result
}

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
        Token::String(s) => (s.len() + 2) as u32, // +2 for quotes
        Token::Number(n) => format!("{}", n).len() as u32,
        Token::Eq | Token::Ne | Token::Le | Token::Ge => 2,
        Token::Lt | Token::Gt => 1,
        Token::Comment(s) => (s.len() + 3) as u32, // +3 for "// "
        _ => 1,
    }
}

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
        _ => None,
    }
}

/// Result of running a single inline test.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TestResult {
    /// Test name
    pub name: String,
    /// 0-indexed line where `test "name"` appears
    pub line: u32,
    /// Whether the test passed
    pub passed: bool,
    /// Message (e.g., "expected Deny, got Allow")
    pub message: String,
}

/// Coverage status for a single rule.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RuleCoverage {
    /// Rule name
    pub name: String,
    /// 0-indexed source line of the rule
    pub line: u32,
    /// Whether any test triggers this rule (rule body matches)
    pub has_positive: bool,
    /// Whether any test exists where this rule does NOT match
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

/// Run inline tests in a Karu source file and return results with coverage.
///
/// Returns None if there are no tests or if parsing/compilation fails.
pub fn run_inline_tests(source: &str) -> Option<InlineTestResults> {
    use crate::ast::{EffectAst, ExpectedOutcome};
    use crate::compiler::compile;
    use crate::parser::Parser;
    use crate::rule::Effect;

    // Parse with tests
    let program = match Parser::parse_with_tests(source) {
        Ok(p) => p,
        Err(_) => return None,
    };

    if program.tests.is_empty() {
        return None;
    }

    // Compile the policy (ignoring test blocks)
    let compiled = match compile(source) {
        Ok(c) => c,
        Err(_) => return None,
    };

    // Find line numbers for each test block
    let lines: Vec<&str> = source.lines().collect();

    // Build test inputs first (reused for both test execution and coverage)
    let mut test_inputs = Vec::new();
    for test in &program.tests {
        let mut flat = serde_json::Map::new();
        for entity in &test.entities {
            if entity.shorthand {
                if let Some((_, value)) = entity.fields.first() {
                    flat.insert(entity.kind.clone(), value.clone());
                }
            } else {
                let mut obj = serde_json::Map::new();
                for (key, value) in &entity.fields {
                    flat.insert(format!("{}.{}", entity.kind, key), value.clone());
                    obj.insert(key.clone(), value.clone());
                }
                flat.insert(entity.kind.clone(), serde_json::Value::Object(obj));
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
        // Find the rule's source line
        let rule_line = lines
            .iter()
            .enumerate()
            .find(|(_, line)| {
                line.contains(&format!("allow {}", rule.name))
                    || line.contains(&format!("deny {}", rule.name))
            })
            .map(|(i, _)| i as u32)
            .unwrap_or(0);

        let mut has_positive = false; // a test where this rule matches
        let mut has_negative = false; // a test where this rule doesn't match

        for (_test, input) in program.tests.iter().zip(test_inputs.iter()) {
            let rule_matches = rule.evaluate(input).is_some();

            if rule_matches {
                has_positive = true;
            } else {
                // Any test where the rule doesn't trigger counts as negative
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

/// Get completion items for Karu keywords.
pub fn keyword_completions() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: "allow".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Permit access rule".to_string()),
            insert_text: Some("allow ${1:rule_name} if\n    ${2:condition};".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "deny".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Deny access rule".to_string()),
            insert_text: Some("deny ${1:rule_name} if\n    ${2:condition};".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "if".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Condition clause".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "and".to_string(),
            kind: Some(CompletionItemKind::OPERATOR),
            detail: Some("Logical AND".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "or".to_string(),
            kind: Some(CompletionItemKind::OPERATOR),
            detail: Some("Logical OR".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "not".to_string(),
            kind: Some(CompletionItemKind::OPERATOR),
            detail: Some("Logical NOT".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "in".to_string(),
            kind: Some(CompletionItemKind::OPERATOR),
            detail: Some("Membership check".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "forall".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Universal quantifier".to_string()),
            insert_text: Some("forall ${1:x} in ${2:collection}: ${3:condition}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "exists".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Existential quantifier".to_string()),
            insert_text: Some("exists ${1:x} in ${2:collection}: ${3:condition}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
        CompletionItem {
            label: "principal".to_string(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some("Request principal".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "action".to_string(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some("Request action".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "resource".to_string(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some("Request resource".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "context".to_string(),
            kind: Some(CompletionItemKind::VARIABLE),
            detail: Some("Request context".to_string()),
            ..Default::default()
        },
        CompletionItem {
            label: "test".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            detail: Some("Inline test block".to_string()),
            insert_text: Some(
                "test \"${1:test name}\" {\n    ${2:input_json}\n    expect ${3|allow,deny|}\n}"
                    .to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        },
    ]
}

/// Convert a ParseError to an LSP Diagnostic.
pub fn error_to_diagnostic(error: &ParseError) -> Diagnostic {
    let line = if error.line > 0 { error.line - 1 } else { 0 };
    let col = if error.column > 0 {
        error.column - 1
    } else {
        0
    };

    Diagnostic {
        range: Range {
            start: Position {
                line: line as u32,
                character: col as u32,
            },
            end: Position {
                line: line as u32,
                character: (col + 1) as u32,
            },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        code: None,
        code_description: None,
        source: Some("karu".to_string()),
        message: error.message.clone(),
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Maximum number of diagnostics to report - beyond this, errors are
/// usually cascading noise from an earlier mistake.
const MAX_DIAGNOSTICS: usize = 5;

/// Parse a Karu policy and return diagnostics.
///
/// Uses a multi-phase approach:
/// 1. Pre-scan for structural issues (unterminated strings) that cause
///    cascade errors from the tree-sitter parser
/// 2. Tree-sitter parse for syntax errors
/// 3. Semantic diagnostics (schema field validation, unknown rule names
///    in expect blocks) via the handrolled parser
///
/// Diagnostics are capped to avoid flooding the editor with cascade noise.
pub fn parse_diagnostics(source: &str) -> Vec<Diagnostic> {
    // Phase 1: detect unterminated string literals
    let string_errors = detect_unterminated_strings(source);
    if !string_errors.is_empty() {
        // Unterminated strings cause massive cascading errors downstream,
        // so report only the string issues - tree-sitter results would be noise.
        return string_errors;
    }

    // Phase 2: tree-sitter parse
    let parse_result = grammar::grammar::Program::parse(source);

    // Filter out spurious errors caused by the word/extras interaction:
    // tree-sitter's `#[word(KaruIdent)]` matches identifier-like content inside
    // `//` comment extras, producing ERROR nodes that aren't real parse failures.
    let comment_ranges = collect_comment_byte_ranges(source);
    let real_errors: Vec<_> = parse_result
        .errors
        .iter()
        .filter(|e| !is_error_in_comment(e, &comment_ranges, source))
        .collect();

    if real_errors.is_empty() {
        // Phase 3: semantic diagnostics - pass the grammar tree for span info
        if let Some(grammar_tree) = parse_result.result {
            semantic_diagnostics(source, &grammar_tree)
        } else {
            vec![]
        }
    } else {
        let mut diagnostics = Vec::new();
        for error in &real_errors {
            flatten_parse_error(error, source, &mut diagnostics);
        }
        cap_diagnostics(diagnostics)
    }
}

/// Semantic diagnostics: checks beyond syntax.
///
/// Currently validates:
/// - Schema constructs (mod blocks, entities) via the handrolled parser
/// - Rule names referenced in `expect { ... }` blocks exist in the file
/// - Schema field access: when `use schema;` is active, checks that field
///   accesses on actor/resource are valid for the declared entity types,
///   using tree-sitter spans for accurate positions
fn semantic_diagnostics(source: &str, grammar_tree: &grammar::grammar::Program) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Use the handrolled parser to extract schema entity data and tests
    let program = match crate::parser::Parser::parse_with_tests(source) {
        Ok(p) => p,
        Err(e) => {
            // The handrolled parser catches additional schema errors.
            diagnostics.push(error_to_diagnostic(&e));
            return diagnostics;
        }
    };

    // Collect all defined rule names
    let rule_names: std::collections::HashSet<&str> =
        program.rules.iter().map(|r| r.name.as_str()).collect();

    // Check each test's PerRule expect entries
    for test in &program.tests {
        if let ExpectedOutcome::PerRule(entries) = &test.expected {
            for (_, rule_name) in entries {
                if !rule_names.contains(rule_name.as_str()) {
                    // Find the position of this rule name in the source
                    if let Some((line, col)) = find_ident_in_source(source, rule_name, &test.name) {
                        diagnostics.push(Diagnostic {
                            range: tower_lsp::lsp_types::Range {
                                start: Position {
                                    line: line as u32,
                                    character: col as u32,
                                },
                                end: Position {
                                    line: line as u32,
                                    character: (col + rule_name.len()) as u32,
                                },
                            },
                            severity: Some(DiagnosticSeverity::ERROR),
                            source: Some("karu".to_string()),
                            message: format!(
                                "Unknown rule '{}' in expect block - no rule with this name exists",
                                rule_name
                            ),
                            ..Default::default()
                        });
                    }
                }
            }
        }
    }

    // Schema field validation (only when `use schema;` is active)
    // Uses the handrolled parser for entity data, grammar tree for expression spans
    if program.use_schema {
        check_schema_field_access_from_tree(&program, source, grammar_tree, &mut diagnostics);
    }

    // Lint checks - detect common policy pitfalls
    let lint_warnings = crate::lint::lint(&program);
    let lines: Vec<&str> = source.lines().collect();
    for warning in lint_warnings {
        // Find the position of the forall keyword in the rule that triggered the warning
        let position = find_forall_in_rule(
            source,
            &lines,
            &warning.rule_name,
            warning.forall_path.as_deref(),
        );
        let (line, col, end_col) = position.unwrap_or((0, 0, 6));

        let mut diag = Diagnostic {
            range: tower_lsp::lsp_types::Range {
                start: Position {
                    line: line as u32,
                    character: col as u32,
                },
                end: Position {
                    line: line as u32,
                    character: end_col as u32,
                },
            },
            severity: Some(DiagnosticSeverity::WARNING),
            code: Some(NumberOrString::String(warning.code.to_string())),
            source: Some("karu".to_string()),
            message: warning.message.clone(),
            ..Default::default()
        };

        // Attach suggestion as data for code actions
        if let Some(ref suggestion) = warning.suggestion {
            diag.data = Some(serde_json::json!({
                "suggestion": suggestion,
                "lint_code": warning.code,
            }));
        }

        diagnostics.push(diag);
    }

    diagnostics
}

/// Build a map of entity name → { field_name → TypeRef }, including inherited
/// fields from abstract traits via `is`.
fn build_entity_field_map(
    program: &crate::ast::Program,
) -> std::collections::HashMap<String, std::collections::HashMap<String, crate::schema::TypeRef>> {
    use crate::schema::TypeRef;

    let mut field_map: std::collections::HashMap<
        String,
        std::collections::HashMap<String, TypeRef>,
    > = std::collections::HashMap::new();

    // First collect all abstract fields with their types
    let mut abstract_fields: std::collections::HashMap<String, Vec<crate::schema::FieldDef>> =
        std::collections::HashMap::new();
    for module in &program.modules {
        for abs in &module.abstracts {
            abstract_fields.insert(abs.name.clone(), abs.fields.clone());
        }
    }

    for module in &program.modules {
        for entity in &module.entities {
            let mut fields: std::collections::HashMap<String, TypeRef> =
                std::collections::HashMap::new();

            // Own fields
            for f in &entity.fields {
                fields.insert(f.name.clone(), f.ty.clone());
            }

            // Inherited fields from abstract traits
            for trait_name in &entity.traits {
                if let Some(abs_fields) = abstract_fields.get(trait_name) {
                    for f in abs_fields {
                        fields.insert(f.name.clone(), f.ty.clone());
                    }
                }
            }

            field_map.insert(entity.name.clone(), fields);
        }
    }

    field_map
}

/// Resolved type for a path or pattern expression.
#[derive(Debug, Clone, PartialEq)]
enum ResolvedType {
    /// Named entity: User, File, etc.
    Entity(String),
    /// Primitive: String, Long, Boolean
    Primitive(String),
    /// Set of some inner type
    Set(Box<ResolvedType>),
    /// Can't determine
    Unknown,
}

impl ResolvedType {
    fn display_name(&self) -> String {
        match self {
            ResolvedType::Entity(name) => name.clone(),
            ResolvedType::Primitive(name) => name.clone(),
            ResolvedType::Set(inner) => format!("Set<{}>", inner.display_name()),
            ResolvedType::Unknown => "unknown".to_string(),
        }
    }
}

/// Convert a schema `TypeRef` to a `ResolvedType`.
fn type_ref_to_resolved(ty: &crate::schema::TypeRef) -> ResolvedType {
    match ty {
        crate::schema::TypeRef::Named(name) => match name.to_lowercase().as_str() {
            "string" => ResolvedType::Primitive("String".to_string()),
            "long" => ResolvedType::Primitive("Long".to_string()),
            "boolean" => ResolvedType::Primitive("Boolean".to_string()),
            _ => ResolvedType::Entity(name.clone()),
        },
        crate::schema::TypeRef::Set(inner) => {
            ResolvedType::Set(Box::new(type_ref_to_resolved(inner)))
        }
        crate::schema::TypeRef::Record(_) | crate::schema::TypeRef::Union(_) => {
            ResolvedType::Unknown
        }
    }
}

/// Look up the type of a field on an entity, including inherited abstract fields.
fn lookup_field_type(
    entity_name: &str,
    field_name: &str,
    field_map: &std::collections::HashMap<
        String,
        std::collections::HashMap<String, crate::schema::TypeRef>,
    >,
    program: &crate::ast::Program,
) -> Option<crate::schema::TypeRef> {
    // Check entity's own fields (includes inherited abstract fields)
    if let Some(fields) = field_map.get(entity_name) {
        if let Some(ty) = fields.get(field_name) {
            return Some(ty.clone());
        }
    }
    // Also check abstract definitions directly
    for module in &program.modules {
        for abs in &module.abstracts {
            if abs.name == entity_name {
                for f in &abs.fields {
                    if f.name == field_name {
                        return Some(f.ty.clone());
                    }
                }
            }
        }
    }
    None
}

/// Resolve the type of a grammar `Path` expression by walking the field type graph.
fn resolve_path_type(
    path: &grammar::grammar::Path,
    field_map: &std::collections::HashMap<
        String,
        std::collections::HashMap<String, crate::schema::TypeRef>,
    >,
    narrowed: &std::collections::HashMap<String, String>,
    program: &crate::ast::Program,
) -> ResolvedType {
    let root = &*path.head.value;
    let canonical_root = match root {
        "principal" => "actor",
        other => other,
    };

    if canonical_root != "actor" && canonical_root != "resource" {
        return ResolvedType::Unknown;
    }

    if path.segments.is_empty() {
        // Bare `actor`/`resource` - entity ID reference.
        // These can be compared to strings (entity IDs), so don't type-check.
        return ResolvedType::Unknown;
    }

    let type_name = match narrowed.get(canonical_root) {
        Some(t) => t.clone(),
        None => return ResolvedType::Unknown,
    };

    // Walk the field chain
    let mut current_type = type_name;
    for (i, segment) in path.segments.iter().enumerate() {
        let field_name = match segment {
            grammar::grammar::PathSegment::Field(_, name) => name.as_str(),
            _ => return ResolvedType::Unknown,
        };

        if let Some(ty) = lookup_field_type(&current_type, field_name, field_map, program) {
            let resolved = type_ref_to_resolved(&ty);
            if i == path.segments.len() - 1 {
                return resolved;
            }
            match &resolved {
                ResolvedType::Entity(name) => current_type = name.clone(),
                _ => return ResolvedType::Unknown,
            }
        } else {
            return ResolvedType::Unknown;
        }
    }
    ResolvedType::Unknown
}

/// Resolve the type of a grammar `Pattern`.
fn resolve_pattern_type(
    pattern: &grammar::grammar::Pattern,
    field_map: &std::collections::HashMap<
        String,
        std::collections::HashMap<String, crate::schema::TypeRef>,
    >,
    narrowed: &std::collections::HashMap<String, String>,
    program: &crate::ast::Program,
) -> ResolvedType {
    match pattern {
        grammar::grammar::Pattern::StringLit(_) => ResolvedType::Primitive("String".to_string()),
        grammar::grammar::Pattern::NumberLit(_) => ResolvedType::Primitive("Long".to_string()),
        grammar::grammar::Pattern::True(_) | grammar::grammar::Pattern::False(_) => {
            ResolvedType::Primitive("Boolean".to_string())
        }
        grammar::grammar::Pattern::PathRef(path) => {
            resolve_path_type(path, field_map, narrowed, program)
        }
        _ => ResolvedType::Unknown,
    }
}

/// Check whether two resolved types are compatible for comparison.
/// Returns `None` if compatible, `Some(message)` if incompatible.
fn check_type_compatibility(left: &ResolvedType, right: &ResolvedType) -> Option<String> {
    if matches!(left, ResolvedType::Unknown) || matches!(right, ResolvedType::Unknown) {
        return None;
    }
    // Entity == Entity is always allowed
    if matches!(left, ResolvedType::Entity(_)) && matches!(right, ResolvedType::Entity(_)) {
        return None;
    }
    // Same primitive is fine
    if left == right {
        return None;
    }
    // Cross-primitive mismatch
    if matches!(left, ResolvedType::Primitive(_)) && matches!(right, ResolvedType::Primitive(_)) {
        return Some(format!(
            "Type mismatch: comparing {} with {}",
            left.display_name(),
            right.display_name()
        ));
    }
    // Entity vs Primitive
    if matches!(left, ResolvedType::Entity(_)) && matches!(right, ResolvedType::Primitive(_)) {
        return Some(format!(
            "Type mismatch: comparing entity type '{}' with {} literal",
            left.display_name(),
            right.display_name()
        ));
    }
    if matches!(left, ResolvedType::Primitive(_)) && matches!(right, ResolvedType::Entity(_)) {
        return Some(format!(
            "Type mismatch: comparing {} with entity type '{}'",
            left.display_name(),
            right.display_name()
        ));
    }
    None
}

/// Which entity kind a name maps to.
fn entity_kind_for_name(
    program: &crate::ast::Program,
    name: &str,
) -> Option<crate::schema::EntityKind> {
    for module in &program.modules {
        for entity in &module.entities {
            if entity.name == name {
                return Some(entity.kind);
            }
        }
        // Abstracts can be used as type constraints too
        for abs in &module.abstracts {
            if abs.name == name {
                // Abstracts act as traits, which can apply to any kind
                return None;
            }
        }
    }
    None
}

/// Check schema field accesses using the grammar tree for positions.
///
/// Uses entity data from the handrolled parser (modules, entities, fields)
/// and walks the grammar tree's `Expr` nodes to get tree-sitter spans.
fn check_schema_field_access_from_tree(
    program: &crate::ast::Program,
    source: &str,
    grammar_tree: &grammar::grammar::Program,
    diagnostics: &mut Vec<Diagnostic>,
) {
    use crate::schema::EntityKind;

    let field_map = build_entity_field_map(program);

    // Walk grammar tree items to find rule and assert bodies
    for item in &grammar_tree.items {
        match item {
            grammar::grammar::TopLevelItem::Rule(rule_def) => {
                if let Some(body) = &rule_def.body {
                    let narrowed = collect_grammar_is_type_guards(&body.expr);
                    check_grammar_expr_field_access(
                        &body.expr,
                        source,
                        &field_map,
                        &narrowed,
                        program,
                        diagnostics,
                    );
                }
            }
            grammar::grammar::TopLevelItem::Assert(assert_def) => {
                // Assert type params narrow actor/resource types
                let mut narrowed = std::collections::HashMap::new();
                if let Some(type_params) = &assert_def.type_params {
                    let mut params = vec![type_params.first.name.clone()];
                    for tail in &type_params.rest {
                        params.push(tail.name.clone());
                    }
                    for param in &params {
                        let lower = param.to_lowercase();
                        if lower == "action" || lower == "context" {
                            continue;
                        }
                        if let Some(kind) = entity_kind_for_name(program, param) {
                            match kind {
                                EntityKind::Actor => {
                                    narrowed.insert("actor".to_string(), param.clone());
                                }
                                EntityKind::Resource => {
                                    narrowed.insert("resource".to_string(), param.clone());
                                }
                            }
                        } else {
                            for module in &program.modules {
                                for abs in &module.abstracts {
                                    if abs.name == *param {
                                        narrowed.insert("resource".to_string(), param.clone());
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(body) = &assert_def.body {
                    check_grammar_expr_field_access(
                        &body.expr,
                        source,
                        &field_map,
                        &narrowed,
                        program,
                        diagnostics,
                    );
                }
            }
            _ => {}
        }
    }
}

/// Walk a grammar `And` expression tree and collect `is` type guards:
/// `actor is User` narrows `actor` to `User`.
fn collect_grammar_is_type_guards(
    expr: &grammar::grammar::Expr,
) -> std::collections::HashMap<String, String> {
    let mut guards = std::collections::HashMap::new();

    match expr {
        grammar::grammar::Expr::And(left, _, right) => {
            guards.extend(collect_grammar_is_type_guards(left));
            guards.extend(collect_grammar_is_type_guards(right));
        }
        grammar::grammar::Expr::IsType(path, _, type_name) => {
            if path.segments.is_empty() {
                let root = &*path.head.value;
                let canonical = if root == "principal" { "actor" } else { root };
                guards.insert(canonical.to_string(), type_name.clone());
            }
        }
        _ => {}
    }

    guards
}

/// Check field accesses in a grammar expression tree using tree-sitter spans.
fn check_grammar_expr_field_access(
    expr: &grammar::grammar::Expr,
    source: &str,
    field_map: &std::collections::HashMap<
        String,
        std::collections::HashMap<String, crate::schema::TypeRef>,
    >,
    narrowed: &std::collections::HashMap<String, String>,
    program: &crate::ast::Program,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match expr {
        grammar::grammar::Expr::And(l, _, r) | grammar::grammar::Expr::Or(l, _, r) => {
            check_grammar_expr_field_access(l, source, field_map, narrowed, program, diagnostics);
            check_grammar_expr_field_access(r, source, field_map, narrowed, program, diagnostics);
        }
        grammar::grammar::Expr::Not(_, inner) => {
            check_grammar_expr_field_access(
                inner,
                source,
                field_map,
                narrowed,
                program,
                diagnostics,
            );
        }
        grammar::grammar::Expr::Group(_, inner, _) => {
            check_grammar_expr_field_access(
                inner,
                source,
                field_map,
                narrowed,
                program,
                diagnostics,
            );
        }
        grammar::grammar::Expr::Compare(path, _op, pattern) => {
            // Check field existence on both sides
            check_grammar_path_field_access(
                path,
                source,
                field_map,
                narrowed,
                program,
                diagnostics,
            );
            if let grammar::grammar::Pattern::PathRef(p) = pattern {
                check_grammar_path_field_access(
                    p,
                    source,
                    field_map,
                    narrowed,
                    program,
                    diagnostics,
                );
            }

            // Type compatibility check
            let left_type = resolve_path_type(path, field_map, narrowed, program);
            let right_type = resolve_pattern_type(pattern, field_map, narrowed, program);

            if let Some(msg) = check_type_compatibility(&left_type, &right_type) {
                let (line, col) = byte_offset_to_position(source, path.head.position.bytes.start);
                let end_byte = path.head.position.bytes.start + path.head.value.len();
                let (_, end_col) = byte_offset_to_position(source, end_byte);

                diagnostics.push(Diagnostic {
                    range: tower_lsp::lsp_types::Range {
                        start: Position {
                            line: line as u32,
                            character: col as u32,
                        },
                        end: Position {
                            line: line as u32,
                            character: end_col as u32,
                        },
                    },
                    severity: Some(DiagnosticSeverity::WARNING),
                    source: Some("karu".to_string()),
                    message: msg,
                    ..Default::default()
                });
            }
        }
        grammar::grammar::Expr::InExpr(pattern, _, path) => {
            check_grammar_path_field_access(
                path,
                source,
                field_map,
                narrowed,
                program,
                diagnostics,
            );
            if let grammar::grammar::Pattern::PathRef(p) = pattern {
                check_grammar_path_field_access(
                    p,
                    source,
                    field_map,
                    narrowed,
                    program,
                    diagnostics,
                );
            }
        }
        grammar::grammar::Expr::Forall(_, _, _, path, _, body)
        | grammar::grammar::Expr::Exists(_, _, _, path, _, body) => {
            check_grammar_path_field_access(
                path,
                source,
                field_map,
                narrowed,
                program,
                diagnostics,
            );
            check_grammar_expr_field_access(
                body,
                source,
                field_map,
                narrowed,
                program,
                diagnostics,
            );
        }
        grammar::grammar::Expr::Ref(path) => {
            check_grammar_path_field_access(
                path,
                source,
                field_map,
                narrowed,
                program,
                diagnostics,
            );
        }
        grammar::grammar::Expr::Has(_, _, _) => {
            // `has` checks field existence - fine
        }
        grammar::grammar::Expr::IsType(path, _, type_name) => {
            // Validate that the type guard is kind-compatible:
            // `actor is X` requires X to be an actor-kind entity (not resource)
            // `resource is X` requires X to be a resource-kind entity (not actor)
            // Abstracts are allowed for any root (they're traits).
            if path.segments.is_empty() {
                let root = &*path.head.value;
                let canonical_root = match root {
                    "principal" => "actor",
                    other => other,
                };

                if let Some(type_kind) = entity_kind_for_name(program, type_name) {
                    use crate::schema::EntityKind;
                    let kind_mismatch = match (canonical_root, type_kind) {
                        ("actor", EntityKind::Resource) => Some(("actor", "resource")),
                        ("resource", EntityKind::Actor) => Some(("resource", "actor")),
                        _ => None,
                    };

                    if let Some((root_kind, type_kind_str)) = kind_mismatch {
                        let (line, col) =
                            byte_offset_to_position(source, path.head.position.bytes.start);
                        let end_byte = path.head.position.bytes.start
                            + root.len()
                            + " is ".len()
                            + type_name.len();
                        let (_, end_col) = byte_offset_to_position(source, end_byte);

                        diagnostics.push(Diagnostic {
                            range: tower_lsp::lsp_types::Range {
                                start: Position {
                                    line: line as u32,
                                    character: col as u32,
                                },
                                end: Position {
                                    line: line as u32,
                                    character: end_col as u32,
                                },
                            },
                            severity: Some(DiagnosticSeverity::ERROR),
                            source: Some("karu".to_string()),
                            message: format!(
                                "'{}' cannot be '{}' - '{}' is {} type, but {} is {} built-in",
                                root,
                                type_name,
                                type_name,
                                match type_kind_str {
                                    "actor" => "an actor",
                                    _ => "a resource",
                                },
                                root,
                                match root_kind {
                                    "actor" => "an actor",
                                    _ => "a resource",
                                },
                            ),
                            ..Default::default()
                        });
                    }
                } else {
                    // entity_kind_for_name returns None for both abstracts
                    // and truly undefined types - check if it's a known abstract
                    let is_abstract = program
                        .modules
                        .iter()
                        .any(|m| m.abstracts.iter().any(|abs| abs.name == *type_name));

                    if !is_abstract {
                        let (line, col) =
                            byte_offset_to_position(source, path.head.position.bytes.start);
                        let end_byte = path.head.position.bytes.start
                            + root.len()
                            + " is ".len()
                            + type_name.len();
                        let (_, end_col) = byte_offset_to_position(source, end_byte);

                        diagnostics.push(Diagnostic {
                            range: tower_lsp::lsp_types::Range {
                                start: Position {
                                    line: line as u32,
                                    character: col as u32,
                                },
                                end: Position {
                                    line: line as u32,
                                    character: end_col as u32,
                                },
                            },
                            severity: Some(DiagnosticSeverity::ERROR),
                            source: Some("karu".to_string()),
                            message: format!(
                                "Unknown type '{}' in type guard - no entity or abstract with this name is defined in the schema",
                                type_name,
                            ),
                            ..Default::default()
                        });
                    }
                }
            }
        }
    }
}

/// Check a single grammar `Path` for invalid field accesses, using tree-sitter spans.
fn check_grammar_path_field_access(
    path: &grammar::grammar::Path,
    source: &str,
    field_map: &std::collections::HashMap<
        String,
        std::collections::HashMap<String, crate::schema::TypeRef>,
    >,
    narrowed: &std::collections::HashMap<String, String>,
    program: &crate::ast::Program,
    diagnostics: &mut Vec<Diagnostic>,
) {
    // Need at least root.field to be a field access
    if path.segments.is_empty() {
        return;
    }

    let root = &*path.head.value;

    // Map principal→actor for schema purposes
    let canonical_root = match root {
        "principal" => "actor",
        other => other,
    };

    // Only check actor and resource - context and action are different
    if canonical_root != "actor" && canonical_root != "resource" {
        return;
    }

    let field = match &path.segments[0] {
        grammar::grammar::PathSegment::Field(_, name) => name.as_str(),
        _ => return,
    };

    // Skip `id` - it's always present as the entity identifier
    if field == "id" {
        return;
    }

    // Compute position from tree-sitter span on path head
    let (line, col) = byte_offset_to_position(source, path.head.position.bytes.start);
    let end_byte = if !path.segments.is_empty() {
        // Span covers root.field - calculate from head start + root + "." + field
        path.head.position.bytes.start + root.len() + 1 + field.len()
    } else {
        path.head.position.bytes.end
    };
    let (_, end_col) = byte_offset_to_position(source, end_byte);

    // Check if the root has been narrowed via `is` guard or assert type param
    if let Some(type_name) = narrowed.get(canonical_root) {
        // Check if the narrowed type has this field
        let has_field = field_map
            .get(type_name.as_str())
            .is_some_and(|fields| fields.contains_key(field));

        if !has_field {
            // Check if it's an abstract that has this field
            let found_in_abstract = program.modules.iter().any(|m| {
                m.abstracts
                    .iter()
                    .any(|abs| abs.name == *type_name && abs.fields.iter().any(|f| f.name == field))
            });

            if !found_in_abstract {
                diagnostics.push(Diagnostic {
                    range: tower_lsp::lsp_types::Range {
                        start: Position {
                            line: line as u32,
                            character: col as u32,
                        },
                        end: Position {
                            line: line as u32,
                            character: end_col as u32,
                        },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    source: Some("karu".to_string()),
                    message: format!("Field '{}' does not exist on type '{}'", field, type_name),
                    ..Default::default()
                });
            }
        }
    } else {
        // Unguarded access - base actor/resource has no fields
        diagnostics.push(Diagnostic {
            range: tower_lsp::lsp_types::Range {
                start: Position {
                    line: line as u32,
                    character: col as u32,
                },
                end: Position {
                    line: line as u32,
                    character: end_col as u32,
                },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            source: Some("karu".to_string()),
            message: format!(
                "Field '{}' on '{}' requires a type guard - base {} has no fields. \
                 Use '{} is <Type> and ...' or add type parameter to assert",
                field, root, root, root
            ),
            ..Default::default()
        });
    }
}

/// Convert a byte offset in source to (line, column), both 0-indexed.
fn byte_offset_to_position(source: &str, byte_offset: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    for (i, ch) in source.char_indices() {
        if i >= byte_offset {
            return (line, col);
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Find the position of a rule-name identifier inside an expect block,
/// scoped to the test with the given name to avoid false matches.
/// Find the position of a `forall` keyword within a specific rule in the source.
///
/// Returns `(line, start_col, end_col)` (all 0-indexed) or None.
fn find_forall_in_rule(
    source: &str,
    _lines: &[&str],
    rule_name: &str,
    _forall_path: Option<&str>,
) -> Option<(usize, usize, usize)> {
    // Find the rule declaration line
    let rule_marker_allow = format!("allow {}", rule_name);
    let rule_marker_deny = format!("deny {}", rule_name);
    let rule_start = source
        .find(&rule_marker_allow)
        .or_else(|| source.find(&rule_marker_deny))?;

    // Search for "forall" after the rule declaration
    let search_region = &source[rule_start..];
    let forall_offset = search_region.find("forall")?;
    let abs_pos = rule_start + forall_offset;

    // Convert byte offset to line/col
    let mut line = 0;
    let mut col = 0;
    for (i, ch) in source.char_indices() {
        if i == abs_pos {
            return Some((line, col, col + 6)); // "forall" is 6 chars
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    None
}

fn find_ident_in_source(source: &str, ident: &str, test_name: &str) -> Option<(usize, usize)> {
    // Find the test block first by looking for `test "name"`
    let test_marker = format!("test \"{}\"", test_name);
    let test_start = source.find(&test_marker)?;

    // Search for the ident within the test block region
    let search_region = &source[test_start..];
    // Find the expect block within this test
    let expect_start = search_region.find("expect")?;
    let expect_region = &search_region[expect_start..];

    // Find the ident in the expect region
    // Use word-boundary matching to avoid matching substrings
    let mut pos = 0;
    while pos < expect_region.len() {
        if let Some(found) = expect_region[pos..].find(ident) {
            let abs_pos = found + pos;
            // Check word boundaries
            let before_ok = abs_pos == 0
                || !expect_region.as_bytes()[abs_pos - 1].is_ascii_alphanumeric()
                    && expect_region.as_bytes()[abs_pos - 1] != b'_';
            let after_pos = abs_pos + ident.len();
            let after_ok = after_pos >= expect_region.len()
                || !expect_region.as_bytes()[after_pos].is_ascii_alphanumeric()
                    && expect_region.as_bytes()[after_pos] != b'_';
            if before_ok && after_ok {
                // Calculate absolute position in source
                let abs_in_source = test_start + expect_start + abs_pos;
                // Convert to line/col
                let mut line = 0;
                let mut col = 0;
                for (i, ch) in source.char_indices() {
                    if i == abs_in_source {
                        return Some((line, col));
                    }
                    if ch == '\n' {
                        line += 1;
                        col = 0;
                    } else {
                        col += 1;
                    }
                }
                return Some((line, col));
            }
            pos = abs_pos + 1;
        } else {
            break;
        }
    }
    None
}

/// Detect unterminated string literals in the source.
///
/// A string starts with `"` and must close with `"` on the same logical
/// unit. If we reach end-of-line (or certain structural tokens) without
/// closing, the string is unterminated.
fn detect_unterminated_strings(source: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    for (line_num, line) in source.lines().enumerate() {
        let mut in_string = false;
        let mut string_start_col = 0;
        let mut chars = line.char_indices().peekable();

        while let Some((i, ch)) = chars.next() {
            match ch {
                '"' if !in_string => {
                    in_string = true;
                    string_start_col = i;
                }
                '"' if in_string => {
                    in_string = false;
                }
                '\\' if in_string => {
                    chars.next(); // skip escaped char
                }
                _ => {}
            }
        }

        if in_string {
            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position {
                        line: line_num as u32,
                        character: string_start_col as u32,
                    },
                    end: Position {
                        line: line_num as u32,
                        character: line.len() as u32,
                    },
                },
                severity: Some(DiagnosticSeverity::ERROR),
                source: Some("karu".to_string()),
                message: "Unterminated string literal".to_string(),
                ..Default::default()
            });
        }
    }

    diagnostics
}

/// Cap diagnostics at MAX_DIAGNOSTICS, adding a hint if truncated.
fn cap_diagnostics(mut diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let total = diagnostics.len();
    if total > MAX_DIAGNOSTICS {
        diagnostics.truncate(MAX_DIAGNOSTICS);
        // Add a hint on the last error's location about remaining errors
        if let Some(last) = diagnostics.last() {
            let remaining = total - MAX_DIAGNOSTICS;
            let mut hint = last.clone();
            hint.severity = Some(DiagnosticSeverity::HINT);
            hint.message = format!(
                "...and {} more {} (fix the errors above first)",
                remaining,
                if remaining == 1 { "error" } else { "errors" }
            );
            diagnostics.push(hint);
        }
    }
    diagnostics
}

/// Recursively flatten a parse error into individual diagnostics.
///
/// `FailedNode` errors contain nested sub-errors that should each
/// become their own diagnostic rather than being collapsed into a
/// single "Parse error (N issues)" message.
fn flatten_parse_error(error: &ts_errors::ParseError, source: &str, out: &mut Vec<Diagnostic>) {
    // In the new API, errors don't have nested children in the same way.
    // We just convert each error to a diagnostic.
    out.push(ts_error_to_diagnostic(error, source));
}

/// Convert a tree-sitter parse error to an LSP Diagnostic.
fn ts_error_to_diagnostic(error: &ts_errors::ParseError, source: &str) -> Diagnostic {
    let (start_line, start_col) = byte_offset_to_line_col(source, error.error_position.bytes.start);
    let (end_line, end_col) = byte_offset_to_line_col(source, error.error_position.bytes.end);

    let message = match &error.reason {
        ts_errors::ParseErrorReason::Missing(what) => format!("Missing: {}", what),
        ts_errors::ParseErrorReason::Error => "Parse error".to_string(),
        ts_errors::ParseErrorReason::Extract {
            struct_name,
            field_name,
            reason,
        } => {
            format!(
                "Failed to extract {}.{}: {:?}",
                struct_name, field_name, reason
            )
        }
    };

    Diagnostic {
        range: Range {
            start: Position {
                line: start_line as u32,
                character: start_col as u32,
            },
            end: Position {
                line: end_line as u32,
                character: end_col as u32,
            },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        code: None,
        code_description: None,
        source: Some("karu".to_string()),
        message,
        related_information: None,
        tags: None,
        data: None,
    }
}

/// Collect byte ranges of all `//`-style comments in the source.
///
/// Returns a list of `(start_byte, end_byte)` pairs for each comment,
/// where start_byte is the position of `//` and end_byte is just past the
/// last character before `\n` (or end of source).
fn collect_comment_byte_ranges(source: &str) -> Vec<std::ops::Range<usize>> {
    let mut ranges = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            let start = i;
            // Scan to end of line
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            ranges.push(start..i);
        }
        i += 1;
    }
    ranges
}

/// Check whether a parse error falls entirely within a comment region.
///
/// The word/extras interaction in tree-sitter causes spurious ERROR nodes
/// when `#[word(KaruIdent)]` matches identifier-like content inside `//`
/// comment extras. These errors are not real parse failures.
fn is_error_in_comment(
    error: &ts_errors::ParseError,
    comment_ranges: &[std::ops::Range<usize>],
    source: &str,
) -> bool {
    // Only filter `Error` reason (not `Missing` or `Extract` which are real issues)
    if !matches!(error.reason, ts_errors::ParseErrorReason::Error) {
        return false;
    }
    let err_range = &error.error_position.bytes;
    let bytes = source.as_bytes();
    for i in err_range.start..err_range.end {
        if i >= bytes.len() {
            break;
        }
        let is_space = bytes[i].is_ascii_whitespace();
        let is_comment = comment_ranges.iter().any(|r| i >= r.start && i < r.end);
        if !is_space && !is_comment {
            return false;
        }
    }
    true
}

/// Convert a byte offset to (line, column), both 0-indexed.
fn byte_offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(source.len());
    let mut line = 0;
    let mut col = 0;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Convert a rule to an LSP DocumentSymbol.
pub fn rule_to_symbol(rule: &RuleAst, line_number: u32, line_length: u32) -> DocumentSymbol {
    #[allow(deprecated)] // DocumentSymbol::deprecated field is deprecated but required
    DocumentSymbol {
        name: rule.name.clone(),
        detail: Some(format!("{:?} rule", rule.effect)),
        kind: SymbolKind::FUNCTION,
        tags: None,
        deprecated: None,
        range: Range {
            start: Position {
                line: line_number,
                character: 0,
            },
            end: Position {
                line: line_number,
                character: line_length,
            },
        },
        selection_range: Range {
            start: Position {
                line: line_number,
                character: 0,
            },
            end: Position {
                line: line_number,
                character: rule.name.len() as u32,
            },
        },
        children: None,
    }
}

/// Extract document symbols from a Karu policy.
///
/// Uses tree-sitter for error-tolerant parsing - symbols are returned
/// even when the document has syntax errors. Rules and tests are grouped
/// under "Rules" and "Tests" parent nodes in the outline.
pub fn document_symbols(source: &str) -> Vec<DocumentSymbol> {
    use crate::grammar::grammar as g;

    // Use tree-sitter parser for error-tolerant parsing.
    // Use .result instead of .into_result() to tolerate spurious comment errors
    // from the word/extras interaction.
    let parsed = match g::Program::parse(source).result {
        Some(prog) => prog,
        None => return vec![],
    };

    let program = parsed.to_ast();
    let lines: Vec<&str> = source.lines().collect();

    // Collect rule symbols
    let mut rule_symbols = Vec::new();
    for rule in &program.rules {
        for (i, line) in lines.iter().enumerate() {
            if line.contains(&format!("allow {}", rule.name))
                || line.contains(&format!("deny {}", rule.name))
            {
                rule_symbols.push(rule_to_symbol(rule, i as u32, line.len() as u32));
                break;
            }
        }
    }

    // Collect test symbols from the grammar AST (which has TopLevelItem::Test)
    let mut test_symbols = Vec::new();
    for item in &parsed.items {
        if let g::TopLevelItem::Test(test_block) = item {
            // Find the source line
            let test_name = &test_block.name;
            let (start_line, end_line) = find_test_line_range(&lines, test_name);

            #[allow(deprecated)]
            test_symbols.push(DocumentSymbol {
                name: test_name.clone(),
                detail: Some("test".to_string()),
                kind: SymbolKind::METHOD,
                tags: None,
                deprecated: None,
                range: Range {
                    start: Position {
                        line: start_line,
                        character: 0,
                    },
                    end: Position {
                        line: end_line,
                        character: lines
                            .get(end_line as usize)
                            .map(|l| l.len() as u32)
                            .unwrap_or(0),
                    },
                },
                selection_range: Range {
                    start: Position {
                        line: start_line,
                        character: 0,
                    },
                    end: Position {
                        line: start_line,
                        // Select just `test "name"` portion
                        character: (6 + test_name.len() + 1) as u32, // test + space + "name"
                    },
                },
                children: None,
            });
        }
    }

    let mut top_level = Vec::new();

    // Schema group - when "use schema;" is present
    if source.lines().any(|l| l.trim().starts_with("use schema")) {
        let schema_children = extract_schema_symbols(&lines);
        let (_first_line, last_line) =
            if let (Some(first), Some(last)) = (schema_children.first(), schema_children.last()) {
                (first.range.start.line, last.range.end.line)
            } else {
                (0, 0)
            };

        #[allow(deprecated)]
        top_level.push(DocumentSymbol {
            name: "Schema".to_string(),
            detail: Some(format!("{} items", schema_children.len())),
            kind: SymbolKind::NAMESPACE,
            tags: None,
            deprecated: None,
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: last_line,
                    character: lines
                        .get(last_line as usize)
                        .map(|l| l.len() as u32)
                        .unwrap_or(0),
                },
            },
            selection_range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: "use schema;".len() as u32,
                },
            },
            children: Some(schema_children),
        });
    }

    // Rules group
    if !rule_symbols.is_empty() {
        let first_line = rule_symbols
            .first()
            .map(|s| s.range.start.line)
            .unwrap_or(0);
        let last_line = rule_symbols.last().map(|s| s.range.end.line).unwrap_or(0);

        #[allow(deprecated)]
        top_level.push(DocumentSymbol {
            name: "Rules".to_string(),
            detail: Some(format!("{} rules", rule_symbols.len())),
            kind: SymbolKind::NAMESPACE,
            tags: None,
            deprecated: None,
            range: Range {
                start: Position {
                    line: first_line,
                    character: 0,
                },
                end: Position {
                    line: last_line,
                    character: lines
                        .get(last_line as usize)
                        .map(|l| l.len() as u32)
                        .unwrap_or(0),
                },
            },
            selection_range: Range {
                start: Position {
                    line: first_line,
                    character: 0,
                },
                end: Position {
                    line: first_line,
                    character: 5, // "Rules"
                },
            },
            children: Some(rule_symbols),
        });
    }

    // Tests group
    if !test_symbols.is_empty() {
        let first_line = test_symbols
            .first()
            .map(|s| s.range.start.line)
            .unwrap_or(0);
        let last_line = test_symbols.last().map(|s| s.range.end.line).unwrap_or(0);

        #[allow(deprecated)]
        top_level.push(DocumentSymbol {
            name: "Tests".to_string(),
            detail: Some(format!("{} tests", test_symbols.len())),
            kind: SymbolKind::NAMESPACE,
            tags: None,
            deprecated: None,
            range: Range {
                start: Position {
                    line: first_line,
                    character: 0,
                },
                end: Position {
                    line: last_line,
                    character: lines
                        .get(last_line as usize)
                        .map(|l| l.len() as u32)
                        .unwrap_or(0),
                },
            },
            selection_range: Range {
                start: Position {
                    line: first_line,
                    character: 0,
                },
                end: Position {
                    line: first_line,
                    character: 5, // "Tests"
                },
            },
            children: Some(test_symbols),
        });
    }

    // Defensively clamp all selection_ranges to be within their range.
    // The LSP protocol requires selectionRange ⊆ range, and some of our
    // heuristic line/column math can violate this with trimmed lengths or
    // synthetic names.
    for sym in &mut top_level {
        clamp_selection_range(sym);
    }

    top_level
}

/// Recursively ensure every DocumentSymbol's `selection_range` is contained
/// within its `range`, as required by the LSP protocol.
fn clamp_selection_range(sym: &mut DocumentSymbol) {
    let r = &sym.range;
    let s = &mut sym.selection_range;

    // Clamp start
    if s.start.line < r.start.line
        || (s.start.line == r.start.line && s.start.character < r.start.character)
    {
        s.start = r.start;
    }
    // Clamp end
    if s.end.line > r.end.line || (s.end.line == r.end.line && s.end.character > r.end.character) {
        s.end = r.end;
    }
    // Ensure start <= end after clamping
    if s.start.line > s.end.line
        || (s.start.line == s.end.line && s.start.character > s.end.character)
    {
        s.end = s.start;
    }

    if let Some(children) = &mut sym.children {
        for child in children {
            clamp_selection_range(child);
        }
    }
}

/// Extract schema-related symbols (mod blocks, assert statements) from source.
fn extract_schema_symbols(lines: &[&str]) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let trimmed = lines[i].trim();

        // mod Name { ... } or mod { ... }
        if trimmed.starts_with("mod ") || trimmed.starts_with("mod{") {
            let start_line = i as u32;
            let name = {
                let after_mod = trimmed.strip_prefix("mod").unwrap().trim();
                if after_mod.starts_with('{') {
                    "(unnamed)".to_string()
                } else {
                    after_mod
                        .split(|c: char| c == '{' || c.is_whitespace())
                        .next()
                        .unwrap_or("mod")
                        .to_string()
                }
            };

            // Find the end of the mod block
            let mut depth = 0i32;
            let start = i;
            loop {
                if i >= lines.len() {
                    break;
                }
                for ch in lines[i].chars() {
                    if ch == '{' {
                        depth += 1;
                    } else if ch == '}' {
                        depth -= 1;
                    }
                }
                i += 1;
                if depth == 0 && i > start + 1 {
                    // Include trailing semicolon
                    if i < lines.len() && lines[i].trim() == ";" {
                        i += 1;
                    }
                    break;
                }
            }
            let end_line = (i - 1) as u32;

            // Extract child symbols: actor, resource, action, abstract
            let entity_children =
                extract_entity_symbols(lines, start_line as usize, end_line as usize);

            #[allow(deprecated)]
            symbols.push(DocumentSymbol {
                name: format!("mod {}", name),
                detail: None,
                kind: SymbolKind::MODULE,
                tags: None,
                deprecated: None,
                range: Range {
                    start: Position {
                        line: start_line,
                        character: 0,
                    },
                    end: Position {
                        line: end_line,
                        character: lines
                            .get(end_line as usize)
                            .map(|l| l.len() as u32)
                            .unwrap_or(0),
                    },
                },
                selection_range: Range {
                    start: Position {
                        line: start_line,
                        character: 0,
                    },
                    end: Position {
                        line: start_line,
                        character: lines
                            .get(start_line as usize)
                            .map(|l| l.len() as u32)
                            .unwrap_or(0),
                    },
                },
                children: if entity_children.is_empty() {
                    None
                } else {
                    Some(entity_children)
                },
            });
            continue;
        }

        // assert name<...> if ...;
        if trimmed.starts_with("assert ") && trimmed.contains(';') {
            let name = trimmed
                .strip_prefix("assert ")
                .unwrap()
                .split(|c: char| c == '<' || c.is_whitespace())
                .next()
                .unwrap_or("assert")
                .to_string();

            #[allow(deprecated)]
            symbols.push(DocumentSymbol {
                name: format!("assert {}", name),
                detail: None,
                kind: SymbolKind::CONSTANT,
                tags: None,
                deprecated: None,
                range: Range {
                    start: Position {
                        line: i as u32,
                        character: 0,
                    },
                    end: Position {
                        line: i as u32,
                        character: trimmed.len() as u32,
                    },
                },
                selection_range: Range {
                    start: Position {
                        line: i as u32,
                        character: 0,
                    },
                    end: Position {
                        line: i as u32,
                        character: (7 + name.len()) as u32, // "assert " + name
                    },
                },
                children: None,
            });
        }

        i += 1;
    }

    symbols
}

/// Extract entity symbols (actor, resource, action, abstract) from within a mod block.
fn extract_entity_symbols(lines: &[&str], start: usize, end: usize) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();

    for i in start..=end {
        if i >= lines.len() {
            break;
        }
        let trimmed = lines[i].trim();

        // Look for entity declarations: actor Name, resource Name, action "Name", abstract Name
        let (kind_str, entity_kind) = if trimmed.starts_with("actor ") {
            ("actor", SymbolKind::CLASS)
        } else if trimmed.starts_with("resource ") {
            ("resource", SymbolKind::STRUCT)
        } else if trimmed.starts_with("action ") {
            ("action", SymbolKind::EVENT)
        } else if trimmed.starts_with("abstract ") {
            ("abstract", SymbolKind::INTERFACE)
        } else {
            continue;
        };

        let after_keyword = trimmed.strip_prefix(kind_str).unwrap().trim();
        let name = after_keyword
            .split(['{', ' ', ';'])
            .next()
            .unwrap_or("")
            .trim_matches('"')
            .to_string();

        if name.is_empty() {
            continue;
        }

        // Find the end of this entity (might span multiple lines with { })
        let entity_start = i as u32;
        let entity_end = if trimmed.contains('{') && !trimmed.contains('}') {
            // Multi-line: find closing brace
            let mut depth = 0i32;
            let mut ej = i;
            for (j, line) in lines.iter().enumerate().skip(i).take(end - i + 1) {
                for ch in line.chars() {
                    if ch == '{' {
                        depth += 1;
                    } else if ch == '}' {
                        depth -= 1;
                    }
                }
                ej = j;
                if depth <= 0 {
                    break;
                }
            }
            ej as u32
        } else {
            entity_start
        };

        #[allow(deprecated)]
        symbols.push(DocumentSymbol {
            name: format!("{} {}", kind_str, name),
            detail: None,
            kind: entity_kind,
            tags: None,
            deprecated: None,
            range: Range {
                start: Position {
                    line: entity_start,
                    character: 0,
                },
                end: Position {
                    line: entity_end,
                    character: lines
                        .get(entity_end as usize)
                        .map(|l| l.len() as u32)
                        .unwrap_or(0),
                },
            },
            selection_range: Range {
                start: Position {
                    line: entity_start,
                    character: 0,
                },
                end: Position {
                    line: entity_start,
                    character: (kind_str.len() + 1 + name.len()) as u32,
                },
            },
            children: None,
        });
    }

    symbols
}

/// Find the start and end lines of a test block by name.
fn find_test_line_range(lines: &[&str], test_name: &str) -> (u32, u32) {
    let needle = format!(r#"test "{}""#, test_name);
    let mut start = 0u32;
    let mut end = 0u32;

    for (i, line) in lines.iter().enumerate() {
        if line.contains(&needle) {
            start = i as u32;
            // Scan forward for the closing brace
            end = start;
            let mut depth = 0i32;
            for (j, subsequent) in lines.iter().enumerate().skip(i) {
                if subsequent.contains('{') {
                    depth += 1;
                }
                if subsequent.contains('}') {
                    depth -= 1;
                    if depth <= 0 {
                        end = j as u32;
                        break;
                    }
                }
            }
            break;
        }
    }

    (start, end)
}

/// Location of a rule definition in source.
#[derive(Debug, Clone)]
pub struct RuleLocation {
    pub name: String,
    pub line: u32,   // 0-indexed
    pub column: u32, // 0-indexed
    pub end_column: u32,
}

/// Find all rule locations in a Karu source file.
pub fn find_rule_locations(source: &str) -> Vec<RuleLocation> {
    // Use .result instead of .into_result() to tolerate spurious comment errors
    // from the word/extras interaction.
    let program = match grammar::grammar::Program::parse(source).result {
        Some(prog) => prog.to_ast(),
        None => return vec![],
    };

    let lines: Vec<&str> = source.lines().collect();
    let mut locations = Vec::new();

    for rule in &program.rules {
        for (i, line) in lines.iter().enumerate() {
            if let Some(pos) = line.find(&format!("allow {}", rule.name)) {
                let col = pos + 6; // "allow " is 6 chars
                locations.push(RuleLocation {
                    name: rule.name.clone(),
                    line: i as u32,
                    column: col as u32,
                    end_column: (col + rule.name.len()) as u32,
                });
                break;
            } else if let Some(pos) = line.find(&format!("deny {}", rule.name)) {
                let col = pos + 5; // "deny " is 5 chars
                locations.push(RuleLocation {
                    name: rule.name.clone(),
                    line: i as u32,
                    column: col as u32,
                    end_column: (col + rule.name.len()) as u32,
                });
                break;
            }
        }
    }

    locations
}

// ============================================================================
// Cedar file support
// ============================================================================

#[cfg(feature = "cedar")]
/// Check if a URI refers to a Cedar policy file.
pub fn is_cedar_uri(uri: &str) -> bool {
    uri.ends_with(".cedar")
}

#[cfg(feature = "cedar")]
/// Check if a URI refers to a Cedar schema file.
pub fn is_cedarschema_uri(uri: &str) -> bool {
    uri.ends_with(".cedarschema")
}

#[cfg(feature = "cedar")]
/// Parse a Cedar policy and return diagnostics.
///
/// Uses the handrolled Cedar parser which gives precise error messages.
pub fn cedar_parse_diagnostics(source: &str) -> Vec<Diagnostic> {
    match crate::cedar_parser::parse(source) {
        Ok(_) => {
            // Cedar parses OK - now try the import to check Karu compatibility
            match crate::cedar_import::from_cedar(source) {
                Ok(_) => vec![], // All good
                Err(e) => vec![Diagnostic {
                    range: Range {
                        start: Position {
                            line: e.line.map(|l| l.saturating_sub(1) as u32).unwrap_or(0),
                            character: 0,
                        },
                        end: Position {
                            line: e.line.map(|l| l.saturating_sub(1) as u32).unwrap_or(0),
                            character: 80,
                        },
                    },
                    severity: Some(DiagnosticSeverity::WARNING),
                    code: None,
                    code_description: None,
                    source: Some("cedar".to_string()),
                    message: format!("Karu import: {}", e.message),
                    related_information: None,
                    tags: None,
                    data: None,
                }],
            }
        }
        Err(e) => vec![Diagnostic {
            range: Range {
                start: Position {
                    line: e.line.saturating_sub(1) as u32,
                    character: e.column.saturating_sub(1) as u32,
                },
                end: Position {
                    line: e.line.saturating_sub(1) as u32,
                    character: (e.column + 10) as u32,
                },
            },
            severity: Some(DiagnosticSeverity::ERROR),
            code: None,
            code_description: None,
            source: Some("cedar".to_string()),
            message: e.message,
            related_information: None,
            tags: None,
            data: None,
        }],
    }
}

#[cfg(feature = "cedar")]
/// Extract document symbols from a Cedar policy file.
pub fn cedar_document_symbols(source: &str) -> Vec<DocumentSymbol> {
    let policy_set = match crate::cedar_parser::parse(source) {
        Ok(ps) => ps,
        Err(_) => return vec![],
    };

    let lines: Vec<&str> = source.lines().collect();
    let mut symbols = Vec::new();

    for (i, policy) in policy_set.policies.iter().enumerate() {
        let name = policy
            .annotations
            .iter()
            .find(|a| a.key == "id")
            .and_then(|a| a.value.clone())
            .unwrap_or_else(|| format!("policy_{}", i));

        let effect = match policy.effect {
            crate::cedar_parser::CedarEffect::Permit => "permit",
            crate::cedar_parser::CedarEffect::Forbid => "forbid",
        };

        // Find the line containing this policy's effect keyword
        let mut found_line = 0u32;
        let mut found_end = 0u32;
        for (line_idx, line) in lines.iter().enumerate() {
            if line.contains(effect) && (line.contains("principal") || line.contains("(")) {
                found_line = line_idx as u32;
                found_end = line.len() as u32;
                break;
            }
        }

        #[allow(deprecated)]
        symbols.push(DocumentSymbol {
            name: name.clone(),
            detail: Some(format!("{} policy", effect)),
            kind: SymbolKind::FUNCTION,
            tags: None,
            deprecated: None,
            range: Range {
                start: Position {
                    line: found_line,
                    character: 0,
                },
                end: Position {
                    line: found_line,
                    character: found_end,
                },
            },
            selection_range: Range {
                start: Position {
                    line: found_line,
                    character: 0,
                },
                end: Position {
                    line: found_line,
                    character: name.len() as u32,
                },
            },
            children: None,
        });
    }

    for sym in &mut symbols {
        clamp_selection_range(sym);
    }
    symbols
}

/// Parse a Cedar policy using the tree-sitter grammar (error-tolerant).
///
/// Provides diagnostics even when the handrolled parser fails, enabling
/// partial feedback during editing.
#[cfg(all(feature = "dev", feature = "cedar"))]
pub fn cedar_ts_parse_diagnostics(source: &str) -> Vec<Diagnostic> {
    let result = crate::cedar_grammar::grammar::PolicySet::parse(source);
    // Filter out spurious errors caused by the word/extras interaction:
    // tree-sitter's `#[word(CedarIdent)]` matches identifier-like content inside
    // `//` comment extras, producing ERROR nodes that aren't real parse failures.
    let comment_ranges = collect_comment_byte_ranges(source);
    result
        .errors
        .iter()
        .filter(|e| !is_error_in_comment(e, &comment_ranges, source))
        .map(|e| ts_error_to_diagnostic(e, source))
        .collect()
}

/// Parse a Cedar schema and return diagnostics.
///
/// Uses the tree-sitter grammar for error-tolerant parsing of `.cedarschema` files.
#[cfg(all(feature = "dev", feature = "cedar"))]
pub fn cedarschema_parse_diagnostics(source: &str) -> Vec<Diagnostic> {
    let result = crate::cedar_schema_grammar::grammar::Schema::parse(source);
    // Filter out spurious errors caused by the word/extras interaction
    let comment_ranges = collect_comment_byte_ranges(source);
    result
        .errors
        .iter()
        .filter(|e| !is_error_in_comment(e, &comment_ranges, source))
        .map(|e| {
            let mut diag = ts_error_to_diagnostic(e, source);
            diag.source = Some("cedarschema".to_string());
            diag
        })
        .collect()
}

/// Extract document symbols from a Cedar schema file.
///
/// Returns namespace, entity, action, and type symbols for the outline view.
#[cfg(all(feature = "dev", feature = "cedar"))]
pub fn cedarschema_document_symbols(source: &str) -> Vec<DocumentSymbol> {
    use crate::cedar_schema_grammar::grammar;

    let schema = match grammar::Schema::parse(source).into_result() {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let lines: Vec<&str> = source.lines().collect();
    let mut symbols = Vec::new();

    for item in &schema.items {
        match item {
            grammar::SchemaItem::Namespace(ns) => {
                let (line, end) = find_keyword_line(&lines, "namespace", &ns.path);
                let mut children = Vec::new();

                for decl in &ns.decls {
                    if let Some(sym) = inner_decl_to_symbol(decl, &lines) {
                        children.push(sym);
                    }
                }

                #[allow(deprecated)]
                symbols.push(DocumentSymbol {
                    name: ns.path.clone(),
                    detail: Some("namespace".to_string()),
                    kind: SymbolKind::NAMESPACE,
                    tags: None,
                    deprecated: None,
                    range: Range {
                        start: Position { line, character: 0 },
                        end: Position {
                            line,
                            character: end,
                        },
                    },
                    selection_range: Range {
                        start: Position { line, character: 0 },
                        end: Position {
                            line,
                            character: end,
                        },
                    },
                    children: if children.is_empty() {
                        None
                    } else {
                        Some(children)
                    },
                });
            }
            grammar::SchemaItem::Entity(ref e) => {
                if let Some(sym) = entity_to_symbol(e, &lines) {
                    symbols.push(sym);
                }
            }
            grammar::SchemaItem::Action(ref a) => {
                if let Some(sym) = action_to_symbol(a, &lines) {
                    symbols.push(sym);
                }
            }
            grammar::SchemaItem::TypeDecl(t) => {
                let (line, end) = find_keyword_line(&lines, "type", &t.name);
                #[allow(deprecated)]
                symbols.push(DocumentSymbol {
                    name: t.name.clone(),
                    detail: Some("type alias".to_string()),
                    kind: SymbolKind::TYPE_PARAMETER,
                    tags: None,
                    deprecated: None,
                    range: Range {
                        start: Position { line, character: 0 },
                        end: Position {
                            line,
                            character: end,
                        },
                    },
                    selection_range: Range {
                        start: Position { line, character: 0 },
                        end: Position {
                            line,
                            character: end,
                        },
                    },
                    children: None,
                });
            }
        }
    }

    for sym in &mut symbols {
        clamp_selection_range(sym);
    }
    symbols
}

#[cfg(all(feature = "dev", feature = "cedar"))]
fn inner_decl_to_symbol(
    decl: &crate::cedar_schema_grammar::grammar::NamespaceInnerDecl,
    lines: &[&str],
) -> Option<DocumentSymbol> {
    use crate::cedar_schema_grammar::grammar::NamespaceInnerDecl;
    match decl {
        NamespaceInnerDecl::Entity(e) => entity_to_symbol(e, lines),
        NamespaceInnerDecl::Action(a) => action_to_symbol(a, lines),
        NamespaceInnerDecl::TypeDecl(t) => {
            let (line, end) = find_keyword_line(lines, "type", &t.name);
            #[allow(deprecated)]
            Some(DocumentSymbol {
                name: t.name.clone(),
                detail: Some("type alias".to_string()),
                kind: SymbolKind::TYPE_PARAMETER,
                tags: None,
                deprecated: None,
                range: Range {
                    start: Position { line, character: 0 },
                    end: Position {
                        line,
                        character: end,
                    },
                },
                selection_range: Range {
                    start: Position { line, character: 0 },
                    end: Position {
                        line,
                        character: end,
                    },
                },
                children: None,
            })
        }
    }
}

#[cfg(all(feature = "dev", feature = "cedar"))]
fn entity_to_symbol(
    e: &crate::cedar_schema_grammar::grammar::EntityDecl,
    lines: &[&str],
) -> Option<DocumentSymbol> {
    let (line, end) = find_keyword_line(lines, "entity", &e.name);
    #[allow(deprecated)]
    Some(DocumentSymbol {
        name: e.name.clone(),
        detail: Some("entity".to_string()),
        kind: SymbolKind::CLASS,
        tags: None,
        deprecated: None,
        range: Range {
            start: Position { line, character: 0 },
            end: Position {
                line,
                character: end,
            },
        },
        selection_range: Range {
            start: Position { line, character: 0 },
            end: Position {
                line,
                character: end,
            },
        },
        children: None,
    })
}

#[cfg(all(feature = "dev", feature = "cedar"))]
fn action_to_symbol(
    a: &crate::cedar_schema_grammar::grammar::ActionDecl,
    lines: &[&str],
) -> Option<DocumentSymbol> {
    let (line, end) = find_keyword_line(lines, "action", &a.name);
    #[allow(deprecated)]
    Some(DocumentSymbol {
        name: a.name.clone(),
        detail: Some("action".to_string()),
        kind: SymbolKind::EVENT,
        tags: None,
        deprecated: None,
        range: Range {
            start: Position { line, character: 0 },
            end: Position {
                line,
                character: end,
            },
        },
        selection_range: Range {
            start: Position { line, character: 0 },
            end: Position {
                line,
                character: end,
            },
        },
        children: None,
    })
}

#[cfg(all(feature = "dev", feature = "cedar"))]
fn find_keyword_line(lines: &[&str], keyword: &str, name: &str) -> (u32, u32) {
    for (i, line) in lines.iter().enumerate() {
        if line.contains(keyword) && line.contains(name) {
            return (i as u32, line.len() as u32);
        }
    }
    (0, 0)
}

/// Convert Cedar policy source to Karu source text.
///
/// Returns the Karu source or a user-friendly error message.
#[cfg(feature = "cedar")]
pub fn convert_cedar_to_karu(cedar_source: &str) -> Result<String, String> {
    crate::cedar_import::from_cedar_to_source(cedar_source)
        .map_err(|e| format!("Failed to convert: {}", e))
}

/// Find the definition location for the word at the given position.
///
/// Returns the location if the position is on a rule name.
pub fn find_definition(source: &str, line: u32, col: u32) -> Option<RuleLocation> {
    let lines: Vec<&str> = source.lines().collect();
    let target_line = lines.get(line as usize)?;

    // Get the word at the cursor position
    let chars: Vec<char> = target_line.chars().collect();
    let col_usize = col as usize;

    if col_usize >= chars.len() {
        return None;
    }

    // Find word boundaries
    let mut start = col_usize;
    while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
        start -= 1;
    }
    let mut end = col_usize;
    while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
        end += 1;
    }

    let word: String = chars[start..end].iter().collect();
    if word.is_empty() {
        return None;
    }

    // Find this rule in the policy
    let locations = find_rule_locations(source);
    locations.into_iter().find(|loc| loc.name == word)
}

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
    fn test_keyword_hover_deny() {
        let hover = keyword_hover("deny");
        assert!(hover.is_some());
        assert!(hover.unwrap().contains("precedence"));
    }

    #[test]
    fn test_keyword_hover_unknown() {
        assert!(keyword_hover("foobar").is_none());
    }

    #[test]
    fn test_keyword_hover_context() {
        let hover = keyword_hover("context");
        assert!(hover.is_some());
        assert!(hover.unwrap().contains("Request-time"));
    }

    #[test]
    fn test_parse_diagnostics_valid() {
        let policy = r#"allow access if principal == "alice";"#;
        let diagnostics = parse_diagnostics(policy);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_parse_diagnostics_invalid() {
        // Missing semicolon triggers a parse error
        let policy = r#"allow access"#;
        let diagnostics = parse_diagnostics(policy);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].severity, Some(DiagnosticSeverity::ERROR));
    }

    #[test]
    fn test_keyword_completions() {
        let completions = keyword_completions();
        assert!(completions.len() >= 10);

        let labels: Vec<&str> = completions.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"allow"));
        assert!(labels.contains(&"deny"));
        assert!(labels.contains(&"forall"));
        assert!(labels.contains(&"context"));
    }

    #[test]
    fn test_document_symbols() {
        let policy = r#"
            allow view if action == "view";
            deny delete if action == "delete";
            allow admin;
        "#;
        let symbols = document_symbols(policy);
        assert_eq!(symbols.len(), 1); // One "Rules" group
        assert_eq!(symbols[0].name, "Rules");

        let children = symbols[0].children.as_ref().unwrap();
        assert_eq!(children.len(), 3);
        let names: Vec<&str> = children.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"view"));
        assert!(names.contains(&"delete"));
        assert!(names.contains(&"admin"));
    }

    #[test]
    fn test_document_symbols_invalid_policy() {
        let policy = r#"allow broken if"#;
        let symbols = document_symbols(policy);
        assert!(symbols.is_empty()); // Invalid policy returns no symbols
    }

    #[test]
    fn test_semantic_tokens_basic() {
        let policy = r#"allow access if principal == "alice";"#;
        let tokens = semantic_tokens(policy);

        // Should have: allow (keyword), access (property), if (keyword),
        // principal (variable), == (operator), "alice" (string)
        assert!(tokens.len() >= 5);

        // Check first token is 'allow' keyword
        assert_eq!(tokens[0].token_type, SemanticTokenType::Keyword);
        assert_eq!(tokens[0].line, 0);
        assert_eq!(tokens[0].start, 0);
        assert_eq!(tokens[0].length, 5);

        // Find the string token
        let string_tok = tokens
            .iter()
            .find(|t| t.token_type == SemanticTokenType::String);
        assert!(string_tok.is_some());
    }

    #[test]
    fn test_semantic_tokens_variables() {
        let policy = r#"allow test if principal == action;"#;
        let tokens = semantic_tokens(policy);

        // Both 'principal' and 'action' should be Variable type
        let vars: Vec<_> = tokens
            .iter()
            .filter(|t| t.token_type == SemanticTokenType::Variable)
            .collect();
        assert_eq!(vars.len(), 2);
    }

    #[test]
    fn test_semantic_tokens_comment() {
        let policy = "// This is a comment\nallow test;";
        let tokens = semantic_tokens(policy);

        let comment = tokens
            .iter()
            .find(|t| t.token_type == SemanticTokenType::Comment);
        assert!(comment.is_some());
        assert_eq!(comment.unwrap().line, 0);
    }

    #[test]
    fn test_find_rule_locations() {
        let policy = r#"
allow view if action == "view";
deny delete if action == "delete";
allow admin;
"#;
        let locations = find_rule_locations(policy);
        assert_eq!(locations.len(), 3);

        assert_eq!(locations[0].name, "view");
        assert_eq!(locations[0].line, 1); // 0-indexed

        assert_eq!(locations[1].name, "delete");
        assert_eq!(locations[1].line, 2);

        assert_eq!(locations[2].name, "admin");
        assert_eq!(locations[2].line, 3);
    }

    #[test]
    fn test_find_definition_rule_name() {
        let policy = r#"allow my_rule if action == "test";"#;

        // Cursor on "my_rule" (starts at column 6)
        let def = find_definition(policy, 0, 7);
        assert!(def.is_some());

        let loc = def.unwrap();
        assert_eq!(loc.line, 0);
    }
}
