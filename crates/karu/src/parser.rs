// SPDX-License-Identifier: MIT

//! Parser for Karu's Polar-inspired syntax.
//!
//! Parses token streams into an AST, which can then be compiled into
//! executable `Policy` structures.
//!
//! # Grammar (simplified)
//!
//! ```text
//! program     := rule*
//! rule        := effect IDENT ('if' expr)? ';'
//! effect      := 'allow' | 'deny'
//! expr        := or_expr
//! or_expr     := and_expr ('or' and_expr)*
//! and_expr    := unary_expr ('and' unary_expr)*
//! unary_expr  := 'not' unary_expr | primary_expr
//! primary_expr:= comparison | membership | forall | '(' expr ')'
//! comparison  := path op pattern
//! membership  := pattern 'in' path
//! forall      := 'forall' IDENT 'in' path ':' expr
//! exists      := 'exists' IDENT 'in' path ':' expr
//! path        := IDENT ('.' IDENT | '[' (NUMBER | IDENT) ']')*
//! pattern     := literal | IDENT | '_' | object_pattern | array_pattern
//! object_pattern := '{' (IDENT ':' pattern (',' IDENT ':' pattern)*)? '}'
//! array_pattern  := '[' (pattern (',' pattern)*)? ']'
//! op          := '==' | '!=' | '<' | '>' | '<=' | '>='
//! ```

use crate::ast::*;
use crate::lexer::{LexError, Lexer, Spanned, Token};
use crate::schema::*;
use serde_json::json;
use std::fmt;

/// Parser error with source position.
#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub token: Option<Token>,
}

impl ParseError {
    /// Format the error with source context for display.
    pub fn format_with_source(&self, source: &str) -> String {
        let source_line = source
            .lines()
            .nth(self.line.saturating_sub(1))
            .unwrap_or("");
        let pointer = format!("{}^", " ".repeat(self.column.saturating_sub(1)));

        format!(
            "error: {}\n --> line {}:{}\n  |\n{} | {}\n  | {}",
            self.message, self.line, self.column, self.line, source_line, pointer
        )
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for ParseError {}

impl From<LexError> for ParseError {
    fn from(e: LexError) -> Self {
        ParseError {
            message: e.message,
            line: e.line,
            column: e.column,
            token: None,
        }
    }
}

/// Parser for Karu source code.
pub struct Parser {
    tokens: Vec<Spanned>,
    pos: usize,
}

impl Parser {
    /// Parse source code into a program AST.
    ///
    /// Fails fast on the first error. Error-tolerant parsing for dev
    /// tooling (LSP) is handled by the tree-sitter parser instead.
    /// Test blocks are silently skipped.
    pub fn parse(source: &str) -> Result<Program, ParseError> {
        let tokens = Lexer::tokenize_spanned(source)?;
        let mut parser = Parser { tokens, pos: 0 };
        parser.parse_program(false)
    }

    /// Parse source code including test blocks.
    ///
    /// Like `parse()`, but also extracts inline test declarations into
    /// `Program::tests`. Used by the `karu test` CLI command.
    pub fn parse_with_tests(source: &str) -> Result<Program, ParseError> {
        let tokens = Lexer::tokenize_spanned(source)?;
        let mut parser = Parser { tokens, pos: 0 };
        parser.parse_program(true)
    }

    fn current(&self) -> &Spanned {
        static EOF: Spanned = Spanned {
            token: Token::Eof,
            line: 0,
            column: 0,
        };
        self.tokens.get(self.pos).unwrap_or(&EOF)
    }

    fn current_token(&self) -> &Token {
        &self.current().token
    }

    fn current_pos(&self) -> (usize, usize) {
        let s = self.current();
        (s.line, s.column)
    }

    fn advance(&mut self) -> Token {
        let tok = self.current().token.clone();
        self.pos += 1;
        tok
    }

    /// Create an error at the current position.
    fn err(&self, message: impl Into<String>) -> ParseError {
        let (line, column) = self.current_pos();
        ParseError {
            message: message.into(),
            line,
            column,
            token: Some(self.current_token().clone()),
        }
    }

    fn expect(&mut self, expected: Token) -> Result<(), ParseError> {
        if self.current_token() == &expected {
            self.advance();
            Ok(())
        } else {
            Err(self.err(format!(
                "Expected {}, found {}",
                expected,
                self.current_token()
            )))
        }
    }

    fn parse_program(&mut self, include_tests: bool) -> Result<Program, ParseError> {
        let mut use_schema = false;
        let mut imports = Vec::new();
        let mut modules = Vec::new();
        let mut assertions = Vec::new();
        let mut rules = Vec::new();
        let mut tests = Vec::new();

        // Check for `use schema;`
        if self.current_token() == &Token::Use {
            self.advance();
            self.expect(Token::Schema)?;
            self.expect(Token::Semi)?;
            use_schema = true;
        }

        // Parse imports (must appear before rules/mods/asserts)
        while self.current_token() == &Token::Import {
            self.advance();
            let path = match self.current_token() {
                Token::String(s) => {
                    let s = s.clone();
                    self.advance();
                    if s.is_empty() {
                        return Err(self.err("Import path must not be empty"));
                    }
                    s
                }
                tok => {
                    return Err(self.err(format!(
                        "Expected string path after 'import', found {}",
                        tok
                    )))
                }
            };
            self.expect(Token::Semi)?;
            imports.push(path);
        }

        while self.current_token() != &Token::Eof {
            match self.current_token() {
                Token::Import => {
                    return Err(self.err(
                        "import statements must appear at the top of the file, before rules and other declarations"
                            .to_string(),
                    ));
                }
                Token::Mod => {
                    modules.push(self.parse_module()?);
                }
                Token::Assert => {
                    assertions.push(self.parse_assert()?);
                }
                Token::Allow | Token::Deny => {
                    rules.push(self.parse_rule()?);
                }
                Token::Test => {
                    if include_tests {
                        tests.push(self.parse_test_block()?);
                    } else {
                        self.skip_test_block()?;
                    }
                }
                tok => {
                    return Err(self.err(format!(
                        "Expected 'allow', 'deny', 'mod', 'assert', or 'test', found {}",
                        tok
                    )))
                }
            }
        }

        Ok(Program {
            use_schema,
            imports,
            modules,
            assertions,
            rules,
            tests,
        })
    }

    fn parse_rule(&mut self) -> Result<RuleAst, ParseError> {
        // effect
        let effect = match self.current_token() {
            Token::Allow => {
                self.advance();
                EffectAst::Allow
            }
            Token::Deny => {
                self.advance();
                EffectAst::Deny
            }
            tok => return Err(self.err(format!("Expected 'allow' or 'deny', found {}", tok))),
        };

        // Rule name (optional - `allow if ...` has no name)
        let name = match self.current_token() {
            Token::Ident(_) | Token::String(_) | Token::Actor | Token::Resource | Token::Action => {
                let n = self.expect_ident_or_string()?;
                if n.is_empty() {
                    return Err(self.err("Rule name must not be empty"));
                }
                n
            }
            _ => format!("{:?}", effect).to_lowercase(),
        };

        // body
        let body = if self.current_token() == &Token::If {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };

        self.expect(Token::Semi)?;

        Ok(RuleAst { name, effect, body })
    }

    // ====================================================================
    // Schema parsing
    // ====================================================================

    /// Parse `mod Name { ... };` or `mod { ... };` (unnamed)
    fn parse_module(&mut self) -> Result<ModuleDef, ParseError> {
        self.expect(Token::Mod)?;

        // Optional name (unnamed modules for file-local types)
        let name = if self.current_token() != &Token::LBrace {
            Some(self.expect_ident()?)
        } else {
            None
        };

        self.expect(Token::LBrace)?;

        let mut entities = Vec::new();
        let mut actions = Vec::new();
        let mut abstracts = Vec::new();

        while self.current_token() != &Token::RBrace {
            match self.current_token() {
                Token::Actor | Token::Resource => {
                    entities.push(self.parse_entity()?);
                }
                Token::Action => {
                    actions.push(self.parse_action()?);
                }
                Token::Abstract => {
                    abstracts.push(self.parse_abstract()?);
                }
                tok => {
                    return Err(self.err(format!(
                        "Expected 'actor', 'resource', 'action', or 'abstract' in mod block, found {}",
                        tok
                    )));
                }
            }
        }

        self.expect(Token::RBrace)?;
        // Optional trailing semicolon after mod block
        if self.current_token() == &Token::Semi {
            self.advance();
        }

        Ok(ModuleDef {
            name,
            entities,
            actions,
            abstracts,
        })
    }

    /// Parse `actor Name { ... };` or `resource Name in Parent { ... };`
    fn parse_entity(&mut self) -> Result<EntityDef, ParseError> {
        let kind = match self.current_token() {
            Token::Actor => {
                self.advance();
                EntityKind::Actor
            }
            Token::Resource => {
                self.advance();
                EntityKind::Resource
            }
            tok => return Err(self.err(format!("Expected 'actor' or 'resource', found {}", tok))),
        };

        let name = self.expect_ident()?;

        // Optional `in Parent` hierarchy
        let mut parents = Vec::new();
        if self.current_token() == &Token::In {
            self.advance();
            parents.push(self.expect_ident()?);
            // Allow `in [A, B]` syntax too
            // For now just single parent
        }

        // Optional `is Trait` composition
        let mut traits = Vec::new();
        if self.current_token() == &Token::Is {
            self.advance();
            traits.push(self.expect_ident()?);
            // TODO: Allow `is A, B` multiple traits
        }

        // Fields: `{ name Type, ... }` or `{}`
        let mut fields = Vec::new();
        if self.current_token() == &Token::LBrace {
            self.advance();
            while self.current_token() != &Token::RBrace {
                fields.push(self.parse_field_def()?);
                // Comma or trailing comma is optional
                if self.current_token() == &Token::Comma {
                    self.advance();
                }
            }
            self.expect(Token::RBrace)?;
        }

        // Trailing semicolon
        if self.current_token() == &Token::Semi {
            self.advance();
        }

        Ok(EntityDef {
            kind,
            name,
            parents,
            traits,
            fields,
        })
    }

    /// Parse `abstract Name { fields };`
    fn parse_abstract(&mut self) -> Result<AbstractDef, ParseError> {
        self.expect(Token::Abstract)?;
        let name = self.expect_ident()?;

        // Fields: `{ name Type, ... }` or `{}`
        let mut fields = Vec::new();
        if self.current_token() == &Token::LBrace {
            self.advance();
            while self.current_token() != &Token::RBrace {
                fields.push(self.parse_field_def()?);
                if self.current_token() == &Token::Comma {
                    self.advance();
                }
            }
            self.expect(Token::RBrace)?;
        }

        // Trailing semicolon
        if self.current_token() == &Token::Semi {
            self.advance();
        }

        Ok(AbstractDef { name, fields })
    }

    /// Parse `action "Name" appliesTo { ... };`
    fn parse_action(&mut self) -> Result<ActionDef, ParseError> {
        self.expect(Token::Action)?;
        let name = self.expect_ident_or_string()?;
        if name.is_empty() {
            return Err(self.err("Action name must not be empty"));
        }

        // Optional appliesTo block
        let applies_to = if let Token::Ident(kw) = self.current_token() {
            if kw == "appliesTo" {
                self.advance();
                Some(self.parse_action_applies_to()?)
            } else {
                None
            }
        } else {
            None
        };

        // Trailing semicolon
        if self.current_token() == &Token::Semi {
            self.advance();
        }

        Ok(ActionDef { name, applies_to })
    }

    /// Parse the `{ actor User, resource File | Folder, context { ... } }` block.
    fn parse_action_applies_to(&mut self) -> Result<ActionAppliesTo, ParseError> {
        self.expect(Token::LBrace)?;

        let mut actors = Vec::new();
        let mut resources = Vec::new();
        let mut context = None;

        while self.current_token() != &Token::RBrace {
            match self.current_token() {
                Token::Actor => {
                    self.advance();
                    // Parse union of actor types: User | Admin
                    actors.push(self.expect_ident()?);
                    while self.current_token() == &Token::Pipe {
                        self.advance();
                        actors.push(self.expect_ident()?);
                    }
                }
                Token::Resource => {
                    self.advance();
                    // Parse union of resource types: File | Folder
                    resources.push(self.expect_ident()?);
                    while self.current_token() == &Token::Pipe {
                        self.advance();
                        resources.push(self.expect_ident()?);
                    }
                }
                Token::Ident(kw) if kw == "context" => {
                    self.advance();
                    // Parse context shape: { field Type, ... }
                    self.expect(Token::LBrace)?;
                    let mut ctx_fields = Vec::new();
                    while self.current_token() != &Token::RBrace {
                        ctx_fields.push(self.parse_field_def()?);
                        if self.current_token() == &Token::Comma {
                            self.advance();
                        }
                    }
                    self.expect(Token::RBrace)?;
                    context = Some(ctx_fields);
                }
                tok => {
                    return Err(self.err(format!(
                        "Expected 'actor', 'resource', or 'context' in appliesTo block, found {}",
                        tok
                    )));
                }
            }
            // Comma between items
            if self.current_token() == &Token::Comma {
                self.advance();
            }
        }

        self.expect(Token::RBrace)?;

        Ok(ActionAppliesTo {
            actors,
            resources,
            context,
        })
    }

    /// Parse `assert name<Types> if expr;` or `assert name is expr;`
    fn parse_assert(&mut self) -> Result<AssertDef, ParseError> {
        self.expect(Token::Assert)?;
        let name = self.expect_ident()?;

        // Optional type params: <User, action, File>
        let mut type_params = Vec::new();
        if self.current_token() == &Token::Lt {
            self.advance();
            loop {
                type_params.push(self.expect_ident()?);
                if self.current_token() == &Token::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
            self.expect(Token::Gt)?;
        }

        // `if expr` or `is expr`
        let body = if matches!(self.current_token(), Token::If | Token::Is) {
            self.advance();
            self.parse_expr()?
        } else {
            return Err(self.err("Expected 'if' or 'is' after assert name"));
        };

        self.expect(Token::Semi)?;

        Ok(AssertDef {
            name,
            type_params,
            body,
        })
    }

    /// Parse a field definition: `name Type` or `name? Type` or `name Type | null`
    fn parse_field_def(&mut self) -> Result<FieldDef, ParseError> {
        let name = self.expect_ident()?;

        // Optional `?` for optional field
        let optional = if self.current_token() == &Token::Question {
            self.advance();
            true
        } else {
            false
        };

        let ty = self.parse_type_ref()?;

        Ok(FieldDef { name, ty, optional })
    }

    /// Parse a type reference: `String`, `Set<User>`, `{ ... }`, `A | B`
    fn parse_type_ref(&mut self) -> Result<TypeRef, ParseError> {
        let base = match self.current_token() {
            Token::LBrace => {
                // Inline record type: { field Type, ... }
                self.advance();
                let mut fields = Vec::new();
                while self.current_token() != &Token::RBrace {
                    fields.push(self.parse_field_def()?);
                    if self.current_token() == &Token::Comma {
                        self.advance();
                    }
                }
                self.expect(Token::RBrace)?;
                TypeRef::Record(fields)
            }
            Token::Ident(_) | Token::Null => {
                let name = if self.current_token() == &Token::Null {
                    self.advance();
                    "null".to_string()
                } else {
                    self.expect_ident()?
                };

                // Check for Set<T> syntax
                if name == "Set" && self.current_token() == &Token::Lt {
                    self.advance();
                    let inner = self.parse_type_ref()?;
                    self.expect(Token::Gt)?;
                    TypeRef::Set(Box::new(inner))
                } else {
                    TypeRef::Named(name)
                }
            }
            tok => {
                return Err(self.err(format!("Expected type name, found {}", tok)));
            }
        };

        // Check for union: `A | B | C`
        if self.current_token() == &Token::Pipe {
            let mut types = vec![base];
            while self.current_token() == &Token::Pipe {
                self.advance();
                let next = match self.current_token() {
                    Token::Null => {
                        self.advance();
                        TypeRef::Named("null".to_string())
                    }
                    Token::Ident(_) => {
                        let name = self.expect_ident()?;
                        TypeRef::Named(name)
                    }
                    tok => {
                        return Err(
                            self.err(format!("Expected type name after '|', found {}", tok))
                        );
                    }
                };
                types.push(next);
            }
            Ok(TypeRef::Union(types))
        } else {
            Ok(base)
        }
    }

    // ====================================================================
    // Helpers
    // ====================================================================

    /// Try to interpret the current token as an identifier name.
    /// Returns Some(name) for Ident and schema keywords that can also be identifiers.
    fn token_as_ident(tok: &Token) -> Option<String> {
        match tok {
            Token::Ident(name) => Some(name.clone()),
            Token::Actor => Some("actor".to_string()),
            Token::Resource => Some("resource".to_string()),
            Token::Action => Some("action".to_string()),
            Token::Schema => Some("schema".to_string()),
            _ => None,
        }
    }

    /// Check if the current token can be an identifier.
    fn is_ident(&self) -> bool {
        Self::token_as_ident(self.current_token()).is_some()
    }

    /// Expect and consume an identifier, returning its name.
    fn expect_ident(&mut self) -> Result<String, ParseError> {
        if let Some(name) = Self::token_as_ident(self.current_token()) {
            self.advance();
            Ok(name)
        } else {
            Err(self.err(format!(
                "Expected identifier, found {}",
                self.current_token()
            )))
        }
    }

    /// Expect and consume an identifier or quoted string.
    fn expect_ident_or_string(&mut self) -> Result<String, ParseError> {
        if let Some(name) = Self::token_as_ident(self.current_token()) {
            self.advance();
            Ok(name)
        } else if let Token::String(s) = self.current_token().clone() {
            self.advance();
            Ok(s)
        } else {
            Err(self.err(format!(
                "Expected identifier or string, found {}",
                self.current_token()
            )))
        }
    }

    fn parse_expr(&mut self) -> Result<ExprAst, ParseError> {
        self.parse_or_expr()
    }

    fn parse_or_expr(&mut self) -> Result<ExprAst, ParseError> {
        let mut left = self.parse_and_expr()?;

        while self.current_token() == &Token::Or {
            self.advance();
            let right = self.parse_and_expr()?;
            left = match left {
                ExprAst::Or(mut exprs) => {
                    exprs.push(right);
                    ExprAst::Or(exprs)
                }
                other => ExprAst::Or(vec![other, right]),
            };
        }

        Ok(left)
    }

    fn parse_and_expr(&mut self) -> Result<ExprAst, ParseError> {
        let mut left = self.parse_unary_expr()?;

        while self.current_token() == &Token::And {
            self.advance();
            let right = self.parse_unary_expr()?;
            left = match left {
                ExprAst::And(mut exprs) => {
                    exprs.push(right);
                    ExprAst::And(exprs)
                }
                other => ExprAst::And(vec![other, right]),
            };
        }

        Ok(left)
    }

    fn parse_unary_expr(&mut self) -> Result<ExprAst, ParseError> {
        if self.current_token() == &Token::Not {
            self.advance();
            let expr = self.parse_unary_expr()?;
            return Ok(ExprAst::Not(Box::new(expr)));
        }

        self.parse_primary_expr()
    }

    fn parse_primary_expr(&mut self) -> Result<ExprAst, ParseError> {
        // Parenthesized expression
        if self.current_token() == &Token::LParen {
            self.advance();
            let expr = self.parse_expr()?;
            self.expect(Token::RParen)?;
            return Ok(expr);
        }

        // Has (attribute existence check)
        if self.current_token() == &Token::Has {
            self.advance();
            let path = self.parse_path()?;
            return Ok(ExprAst::Has { path });
        }

        // Forall
        if self.current_token() == &Token::Forall {
            self.advance();
            let var = match self.current_token() {
                Token::Ident(v) => {
                    let v = v.clone();
                    self.advance();
                    v
                }
                tok => {
                    return Err(self.err(format!("Expected variable name in forall, found {}", tok)))
                }
            };
            self.expect(Token::In)?;
            let path = self.parse_path()?;
            self.expect(Token::Colon)?;
            let body = self.parse_expr()?;
            return Ok(ExprAst::Forall {
                var,
                path,
                body: Box::new(body),
            });
        }

        // Exists (mirrors Forall)
        if self.current_token() == &Token::Exists {
            self.advance();
            let var = match self.current_token() {
                Token::Ident(v) => {
                    let v = v.clone();
                    self.advance();
                    v
                }
                tok => {
                    return Err(self.err(format!("Expected variable name in exists, found {}", tok)))
                }
            };
            self.expect(Token::In)?;
            let path = self.parse_path()?;
            self.expect(Token::Colon)?;
            let body = self.parse_expr()?;
            return Ok(ExprAst::Exists {
                var,
                path,
                body: Box::new(body),
            });
        }

        // Try to parse as membership (pattern in path) or comparison (path op pattern)
        // This is tricky because both can start with an identifier
        // Strategy: Try to parse a pattern first. If followed by 'in', it's membership.
        // Otherwise, reparse as path and comparison.

        // Check if it starts with something that's clearly a pattern (object/array/literal)
        if matches!(
            self.current_token(),
            Token::LBrace
                | Token::LBracket
                | Token::String(_)
                | Token::Number(_)
                | Token::True
                | Token::False
                | Token::Null
                | Token::Underscore
        ) {
            // This is a pattern - expect membership
            let pattern = self.parse_pattern()?;
            self.expect(Token::In)?;
            let path = self.parse_path()?;
            return Ok(ExprAst::In { pattern, path });
        }

        // Must be a path (starts with identifier)
        let path = self.parse_path()?;

        // Check for comparison operator
        if let Some(op) = self.try_parse_op() {
            let pattern = self.parse_pattern()?;
            return Ok(ExprAst::Compare {
                left: path,
                op,
                right: pattern,
            });
        }

        // Check for 'like' - glob pattern matching
        if self.current_token() == &Token::Like_ {
            self.advance();
            if let Token::String(pat) = self.current_token() {
                let pattern = pat.clone();
                self.advance();
                return Ok(ExprAst::Like { path, pattern });
            } else {
                return Err(self.err("Expected string pattern after 'like'"));
            }
        }

        // Check for 'in' - either path-in-path membership or path-in-array-literal
        if self.current_token() == &Token::In {
            self.advance();

            // If followed by '[', parse inline array literal
            if self.current_token() == &Token::LBracket {
                self.advance();
                let mut values = Vec::new();
                while self.current_token() != &Token::RBracket {
                    values.push(self.parse_pattern()?);
                    if self.current_token() == &Token::Comma {
                        self.advance();
                    }
                }
                self.expect(Token::RBracket)?;
                return Ok(ExprAst::InLiteral { path, values });
            }

            let container_path = self.parse_path()?;
            return Ok(ExprAst::In {
                pattern: PatternAst::PathRef(path),
                path: container_path,
            });
        }
        // Check for 'has' after path: `actor has roles`
        if self.current_token() == &Token::Has {
            self.advance();
            let attr_path = self.parse_path()?;
            // Combine into a Has expression: the path is prefix + attr
            let mut full_segments = path.segments;
            full_segments.extend(attr_path.segments);
            return Ok(ExprAst::Has {
                path: PathAst {
                    segments: full_segments,
                },
            });
        }

        // Check for ':' - type reference: `MyCedarNamespace:Delete`
        if self.current_token() == &Token::Colon {
            // The path so far should be a single-segment namespace name
            if path.segments.len() == 1 {
                if let PathSegmentAst::Field(namespace) = &path.segments[0] {
                    let namespace = namespace.clone();
                    self.advance(); // consume ':'
                    let type_name = self.expect_ident()?;
                    return Ok(ExprAst::TypeRef {
                        namespace: Some(namespace),
                        name: type_name,
                    });
                }
            }
        }

        // Check for 'is' - type membership: `resource is File`
        if self.current_token() == &Token::Is {
            self.advance();
            let type_name = self.expect_ident()?;
            return Ok(ExprAst::IsType { path, type_name });
        }

        // Standalone path expression (assertion reference or boolean variable)
        // Treated as `path == true` for now; compile_expr inlines assertions.
        Ok(ExprAst::Compare {
            left: path,
            op: OpAst::Eq,
            right: PatternAst::Literal(serde_json::json!(true)),
        })
    }

    fn parse_path(&mut self) -> Result<PathAst, ParseError> {
        let mut segments = Vec::new();

        // First segment must be identifier
        if let Some(name) = Self::token_as_ident(self.current_token()) {
            segments.push(PathSegmentAst::Field(name));
            self.advance();
        } else {
            return Err(self.err("Expected path identifier"));
        }

        // Continue with dots and brackets
        loop {
            if self.current_token() == &Token::Dot {
                self.advance();
                if let Some(name) = Self::token_as_ident(self.current_token()) {
                    segments.push(PathSegmentAst::Field(name));
                    self.advance();
                } else if let Token::Number(n) = self.current_token() {
                    segments.push(PathSegmentAst::Index(*n as usize));
                    self.advance();
                } else {
                    return Err(self.err("Expected field name after dot"));
                }
            } else if self.current_token() == &Token::LBracket {
                self.advance();
                if let Token::Number(n) = self.current_token() {
                    segments.push(PathSegmentAst::Index(*n as usize));
                    self.advance();
                } else if self.is_ident() {
                    // Could be a simple variable or a path expression like resource.id
                    let first_ident = Self::token_as_ident(self.current_token()).unwrap();
                    self.advance();

                    // Check if followed by a dot (making it a path) or ] (just a variable)
                    if self.current_token() == &Token::Dot {
                        // It's a path expression - parse the rest
                        let mut path_segments = vec![PathSegmentAst::Field(first_ident)];
                        while self.current_token() == &Token::Dot {
                            self.advance();
                            if let Some(name) = Self::token_as_ident(self.current_token()) {
                                path_segments.push(PathSegmentAst::Field(name));
                                self.advance();
                            } else if let Token::Number(n) = self.current_token() {
                                path_segments.push(PathSegmentAst::Index(*n as usize));
                                self.advance();
                            } else {
                                return Err(
                                    self.err("Expected field name after dot in bracket path")
                                );
                            }
                        }
                        // Store as a path reference - we'll use the Variable variant with a special encoding
                        // Actually, we need a new PathSegmentAst variant for this
                        // For now, encode as "path:segment.segment" to distinguish from simple variables
                        let path_str = path_segments
                            .iter()
                            .map(|s| match s {
                                PathSegmentAst::Field(name) => name.clone(),
                                PathSegmentAst::Index(idx) => idx.to_string(),
                                PathSegmentAst::Variable(v) => v.clone(),
                            })
                            .collect::<Vec<_>>()
                            .join(".");
                        segments.push(PathSegmentAst::Variable(format!("@path:{}", path_str)));
                    } else {
                        // Just a simple variable reference
                        segments.push(PathSegmentAst::Variable(first_ident));
                    }
                } else {
                    return Err(self.err("Expected number or path in bracket index"));
                }
                self.expect(Token::RBracket)?;
            } else {
                break;
            }
        }

        Ok(PathAst { segments })
    }

    fn parse_pattern(&mut self) -> Result<PatternAst, ParseError> {
        match self.current_token() {
            Token::String(s) => {
                let s = s.clone();
                self.advance();
                Ok(PatternAst::Literal(json!(s)))
            }
            Token::Number(n) => {
                let n = *n;
                self.advance();
                // Use integer if the value is a whole number for proper JSON matching
                if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
                    Ok(PatternAst::Literal(json!(n as i64)))
                } else {
                    Ok(PatternAst::Literal(json!(n)))
                }
            }
            Token::True => {
                self.advance();
                Ok(PatternAst::Literal(json!(true)))
            }
            Token::False => {
                self.advance();
                Ok(PatternAst::Literal(json!(false)))
            }
            Token::Null => {
                self.advance();
                Ok(PatternAst::Literal(json!(null)))
            }
            Token::Underscore => {
                self.advance();
                Ok(PatternAst::Wildcard)
            }
            Token::Ident(_) | Token::Actor | Token::Resource | Token::Action | Token::Schema => {
                // Could be variable OR path reference (for path-to-path comparison)
                // Check if followed by dot or bracket - if so, it's a path reference
                let path = self.parse_path()?;
                if path.segments.len() == 1 {
                    if let PathSegmentAst::Field(name) = &path.segments[0] {
                        // Known entity names are path references, not variables
                        match name.as_str() {
                            "actor" | "resource" | "principal" | "action" | "context" => {
                                Ok(PatternAst::PathRef(path))
                            }
                            _ => Ok(PatternAst::Variable(name.clone())),
                        }
                    } else {
                        // Index-only path not valid as pattern
                        Err(self.err("Expected pattern"))
                    }
                } else {
                    // Multi-segment = path reference
                    Ok(PatternAst::PathRef(path))
                }
            }
            Token::LBrace => self.parse_object_pattern(),
            Token::LBracket => self.parse_array_pattern(),
            tok => Err(self.err(format!("Expected pattern, found {}", tok))),
        }
    }

    fn parse_object_pattern(&mut self) -> Result<PatternAst, ParseError> {
        self.expect(Token::LBrace)?;
        let mut fields = Vec::new();

        if self.current_token() != &Token::RBrace {
            loop {
                let key = if let Some(k) = Self::token_as_ident(self.current_token()) {
                    self.advance();
                    k
                } else if let Token::String(k) = self.current_token() {
                    let k = k.clone();
                    self.advance();
                    k
                } else {
                    return Err(self.err(format!(
                        "Expected field name, found {}",
                        self.current_token()
                    )));
                };
                self.expect(Token::Colon)?;
                let value = self.parse_pattern()?;
                fields.push((key, value));

                if self.current_token() == &Token::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
        }

        self.expect(Token::RBrace)?;
        Ok(PatternAst::Object(fields))
    }

    fn parse_array_pattern(&mut self) -> Result<PatternAst, ParseError> {
        self.expect(Token::LBracket)?;
        let mut elements = Vec::new();

        if self.current_token() != &Token::RBracket {
            loop {
                elements.push(self.parse_pattern()?);
                if self.current_token() == &Token::Comma {
                    self.advance();
                } else {
                    break;
                }
            }
        }

        self.expect(Token::RBracket)?;
        Ok(PatternAst::Array(elements))
    }

    fn try_parse_op(&mut self) -> Option<OpAst> {
        let op = match self.current_token() {
            Token::Eq => Some(OpAst::Eq),
            Token::Ne => Some(OpAst::Ne),
            Token::Lt => Some(OpAst::Lt),
            Token::Gt => Some(OpAst::Gt),
            Token::Le => Some(OpAst::Le),
            Token::Ge => Some(OpAst::Ge),
            _ => None,
        };
        if op.is_some() {
            self.advance();
        }
        op
    }

    // ====================================================================
    // Test blocks
    // ====================================================================

    /// Skip a `test "name" { ... }` block without building AST.
    /// Just consume the test keyword, name string, and balanced braces.
    fn skip_test_block(&mut self) -> Result<(), ParseError> {
        self.expect(Token::Test)?;
        // Consume the test name (a string literal)
        match self.current_token() {
            Token::String(s) => {
                if s.is_empty() {
                    return Err(self.err("Test name must not be empty"));
                }
                self.advance();
            }
            tok => {
                return Err(self.err(format!("Expected test name string, found {}", tok)));
            }
        }
        // Consume balanced `{ ... }`
        self.expect(Token::LBrace)?;
        let mut depth = 1;
        while depth > 0 {
            match self.current_token() {
                Token::LBrace => {
                    depth += 1;
                    self.advance();
                }
                Token::RBrace => {
                    depth -= 1;
                    self.advance();
                }
                Token::Eof => {
                    return Err(self.err("Unexpected EOF inside test block".to_string()));
                }
                _ => {
                    self.advance();
                }
            }
        }
        Ok(())
    }

    /// Parse a `test "name" { ... }` block into a `TestDef` AST node.
    fn parse_test_block(&mut self) -> Result<TestDef, ParseError> {
        self.expect(Token::Test)?;

        // Test name
        let name = match self.current_token().clone() {
            Token::String(s) => {
                if s.is_empty() {
                    return Err(self.err("Test name must not be empty"));
                }
                self.advance();
                s
            }
            tok => {
                return Err(self.err(format!("Expected test name string, found {}", tok)));
            }
        };

        self.expect(Token::LBrace)?;

        let mut entities = Vec::new();
        let mut expected = None;

        while self.current_token() != &Token::RBrace {
            match self.current_token() {
                Token::Expect => {
                    self.advance();
                    expected = Some(match self.current_token() {
                        // Block form: expect { allow ruleName, deny otherRule, ... }
                        Token::LBrace => {
                            self.advance();
                            let mut entries = Vec::new();
                            while self.current_token() != &Token::RBrace {
                                let effect = match self.current_token() {
                                    Token::Allow => {
                                        self.advance();
                                        EffectAst::Allow
                                    }
                                    Token::Deny => {
                                        self.advance();
                                        EffectAst::Deny
                                    }
                                    tok => {
                                        return Err(self.err(format!(
                                            "Expected 'allow' or 'deny' in expect block, found {}",
                                            tok
                                        )));
                                    }
                                };
                                let rule_name = self.expect_ident()?;
                                entries.push((effect, rule_name));
                                // Optional trailing comma
                                if self.current_token() == &Token::Comma {
                                    self.advance();
                                }
                            }
                            self.expect(Token::RBrace)?;
                            ExpectedOutcome::PerRule(entries)
                        }
                        // Simple form: expect allow / expect deny
                        Token::Allow => {
                            self.advance();
                            ExpectedOutcome::Simple(EffectAst::Allow)
                        }
                        Token::Deny => {
                            self.advance();
                            ExpectedOutcome::Simple(EffectAst::Deny)
                        }
                        tok => {
                            return Err(self.err(format!(
                                "Expected 'allow', 'deny', or '{{' after 'expect', found {}",
                                tok
                            )));
                        }
                    });
                }
                _ => {
                    // Parse an entity: either a full block or shorthand
                    //   Full:      kind { key: value, ... }
                    //   Shorthand: kind "value"  →  { id: "value" }
                    let kind = self.expect_ident()?;

                    let (fields, shorthand) = if self.current_token() == &Token::LBrace {
                        // Full record form
                        self.advance();
                        let mut fields = Vec::new();
                        while self.current_token() != &Token::RBrace {
                            let key = self.expect_ident()?;
                            self.expect(Token::Colon)?;
                            let value = self.parse_test_value()?;
                            fields.push((key, value));
                            // Optional trailing comma
                            if self.current_token() == &Token::Comma {
                                self.advance();
                            }
                        }
                        self.expect(Token::RBrace)?;
                        (fields, false)
                    } else {
                        // Shorthand: treat the next value as the `id` field
                        let value = self.parse_test_value()?;
                        (vec![("id".to_string(), value)], true)
                    };

                    entities.push(TestEntity {
                        kind,
                        fields,
                        shorthand,
                    });
                }
            }
        }

        self.expect(Token::RBrace)?;

        let expected = expected.ok_or_else(|| {
            self.err("Test block missing 'expect allow' or 'expect deny'".to_string())
        })?;

        Ok(TestDef {
            name,
            entities,
            expected,
        })
    }

    /// Parse a JSON-like value inside a test entity block.
    fn parse_test_value(&mut self) -> Result<serde_json::Value, ParseError> {
        match self.current_token().clone() {
            Token::String(s) => {
                self.advance();
                Ok(serde_json::Value::String(s))
            }
            Token::Number(n) => {
                self.advance();
                Ok(serde_json::json!(n))
            }
            Token::True => {
                self.advance();
                Ok(serde_json::Value::Bool(true))
            }
            Token::False => {
                self.advance();
                Ok(serde_json::Value::Bool(false))
            }
            Token::Null => {
                self.advance();
                Ok(serde_json::Value::Null)
            }
            Token::LBrace => {
                // Nested object: { key: value, ... }
                self.advance();
                let mut map = serde_json::Map::new();
                while self.current_token() != &Token::RBrace {
                    let key = self.expect_ident()?;
                    self.expect(Token::Colon)?;
                    let value = self.parse_test_value()?;
                    map.insert(key, value);
                    if self.current_token() == &Token::Comma {
                        self.advance();
                    }
                }
                self.expect(Token::RBrace)?;
                Ok(serde_json::Value::Object(map))
            }
            Token::LBracket => {
                // Array: [ value, ... ]
                self.advance();
                let mut arr = Vec::new();
                while self.current_token() != &Token::RBracket {
                    arr.push(self.parse_test_value()?);
                    if self.current_token() == &Token::Comma {
                        self.advance();
                    }
                }
                self.expect(Token::RBracket)?;
                Ok(serde_json::Value::Array(arr))
            }
            tok => Err(self.err(format!("Expected value in test entity, found {}", tok))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_allow() {
        let prog = Parser::parse("allow access;").unwrap();
        assert_eq!(prog.rules.len(), 1);
        assert_eq!(prog.rules[0].name, "access");
        assert_eq!(prog.rules[0].effect, EffectAst::Allow);
        assert!(prog.rules[0].body.is_none());
    }

    #[test]
    fn test_parse_with_condition() {
        let prog = Parser::parse(r#"allow read if action == "read";"#).unwrap();
        assert!(prog.rules[0].body.is_some());
        if let Some(ExprAst::Compare { left, op, right }) = &prog.rules[0].body {
            assert_eq!(left.segments.len(), 1);
            assert_eq!(op, &OpAst::Eq);
            assert!(matches!(right, PatternAst::Literal(_)));
        } else {
            panic!("Expected Compare expression");
        }
    }

    #[test]
    fn test_parse_and_expression() {
        let prog =
            Parser::parse(r#"allow access if action == "read" and role == "admin";"#).unwrap();
        if let Some(ExprAst::And(exprs)) = &prog.rules[0].body {
            assert_eq!(exprs.len(), 2);
        } else {
            panic!("Expected And expression");
        }
    }

    #[test]
    fn test_parse_or_expression() {
        let prog =
            Parser::parse(r#"allow access if role == "admin" or role == "superuser";"#).unwrap();
        if let Some(ExprAst::Or(exprs)) = &prog.rules[0].body {
            assert_eq!(exprs.len(), 2);
        } else {
            panic!("Expected Or expression");
        }
    }

    #[test]
    fn test_parse_not_expression() {
        let prog = Parser::parse(r#"deny blocked if not active == true;"#).unwrap();
        if let Some(ExprAst::Not(_)) = &prog.rules[0].body {
            // ok
        } else {
            panic!("Expected Not expression");
        }
    }

    #[test]
    fn test_parse_in_expression() {
        let prog = Parser::parse(r#"allow access if {name: "admin"} in roles;"#).unwrap();
        if let Some(ExprAst::In { pattern, path }) = &prog.rules[0].body {
            assert!(matches!(pattern, PatternAst::Object(_)));
            assert_eq!(path.segments.len(), 1);
        } else {
            panic!("Expected In expression");
        }
    }

    #[test]
    fn test_parse_nested_path() {
        let prog = Parser::parse(r#"allow access if resource.context.args == "test";"#).unwrap();
        if let Some(ExprAst::Compare { left, .. }) = &prog.rules[0].body {
            assert_eq!(left.segments.len(), 3);
        } else {
            panic!("Expected Compare expression");
        }
    }

    #[test]
    fn test_parse_object_pattern() {
        let prog = Parser::parse(r#"allow access if {name: "lhs", value: x} in args;"#).unwrap();
        if let Some(ExprAst::In { pattern, .. }) = &prog.rules[0].body {
            if let PatternAst::Object(fields) = pattern {
                assert_eq!(fields.len(), 2);
                assert!(matches!(&fields[0].1, PatternAst::Literal(_)));
                assert!(matches!(&fields[1].1, PatternAst::Variable(_)));
            } else {
                panic!("Expected Object pattern");
            }
        } else {
            panic!("Expected In expression");
        }
    }

    #[test]
    fn test_parse_wildcard() {
        let prog = Parser::parse(r#"allow access if value == _;"#).unwrap();
        if let Some(ExprAst::Compare { right, .. }) = &prog.rules[0].body {
            assert!(matches!(right, PatternAst::Wildcard));
        } else {
            panic!("Expected Compare expression");
        }
    }

    #[test]
    fn test_parse_comparison_operators() {
        let prog = Parser::parse(r#"allow access if age >= 18 and level < 100;"#).unwrap();
        if let Some(ExprAst::And(exprs)) = &prog.rules[0].body {
            assert_eq!(exprs.len(), 2);
            // Check operators
            if let ExprAst::Compare { op, .. } = &exprs[0] {
                assert_eq!(op, &OpAst::Ge);
            }
            if let ExprAst::Compare { op, .. } = &exprs[1] {
                assert_eq!(op, &OpAst::Lt);
            }
        } else {
            panic!("Expected And expression");
        }
    }

    #[test]
    fn test_parse_multiple_rules() {
        let prog = Parser::parse(
            r#"
            allow read_access if action == "read";
            deny delete_access if action == "delete";
            "#,
        )
        .unwrap();
        assert_eq!(prog.rules.len(), 2);
        assert_eq!(prog.rules[0].effect, EffectAst::Allow);
        assert_eq!(prog.rules[1].effect, EffectAst::Deny);
    }

    #[test]
    fn test_parse_readme_example() {
        let src = r#"
            // The Rule
            allow access if
                action == "call" and
                // The Pattern Match
                { name: "lhs", value: 10 } in resource.context.namedArguments;
        "#;
        let prog = Parser::parse(src).unwrap();
        assert_eq!(prog.rules.len(), 1);
        assert_eq!(prog.rules[0].name, "access");
    }

    #[test]
    fn test_error_position_line_column() {
        // Missing semicolon at end of line 1
        let src = "allow access";
        let err = Parser::parse(src).unwrap_err();
        assert_eq!(err.line, 1);
        assert_eq!(err.column, 13); // Points to EOF after 'access'
        assert!(err.message.contains("Expected"));
    }

    #[test]
    fn test_error_position_multiline() {
        // Error on line 2
        let src = "allow one;\nbad";
        let err = Parser::parse(src).unwrap_err();
        assert_eq!(err.line, 2);
        assert_eq!(err.column, 1); // 'bad' starts at column 1
    }

    #[test]
    fn test_error_format_with_source() {
        let src = "allow access";
        let err = Parser::parse(src).unwrap_err();
        let formatted = err.format_with_source(src);
        assert!(formatted.contains("error:"));
        assert!(formatted.contains("--> line 1"));
        assert!(formatted.contains("allow access"));
        assert!(formatted.contains("^")); // Pointer
    }

    #[test]
    fn test_fail_fast_on_first_error() {
        // Parser fails fast on first bad rule, no recovery
        let src = r#"
            allow broken if ==;
            allow valid if action == "read";
        "#;
        let result = Parser::parse(src);
        assert!(result.is_err());
    }

    #[test]
    fn test_fail_fast_multiple_errors() {
        // Parser stops at first error
        let src = r#"
            allow bad1 if ==;
            deny bad2 if;
            allow good;
        "#;
        let result = Parser::parse(src);
        assert!(result.is_err());
    }

    #[test]
    fn test_no_errors_on_valid_program() {
        let src = r#"
            allow one;
            deny two;
        "#;
        let prog = Parser::parse(src).unwrap();
        assert_eq!(prog.rules.len(), 2);
    }

    #[test]
    fn test_parse_path_in_path() {
        // Path-in-path membership: principal.id in resource.adminIds
        let prog = Parser::parse(r#"allow admin if principal.id in resource.adminIds;"#).unwrap();
        if let Some(ExprAst::In { pattern, path }) = &prog.rules[0].body {
            // Left side should be a PathRef pattern
            assert!(matches!(pattern, PatternAst::PathRef(_)));
            if let PatternAst::PathRef(p) = pattern {
                assert_eq!(p.segments.len(), 2); // principal.id
            }
            // Right side should be the container path
            assert_eq!(path.segments.len(), 2); // resource.adminIds
        } else {
            panic!("Expected In expression");
        }
    }
}

#[test]
fn test_parse_multiline_condition() {
    let policy = r#"allow view if
        principal == "alice" and
        action == "view";"#;
    let prog = Parser::parse(policy).unwrap();
    assert_eq!(prog.rules.len(), 1);
    assert_eq!(prog.rules[0].name, "view");
}

#[test]
fn test_parse_assert_untyped_mode() {
    // assert should work without `use schema;`
    let src = concat!(
        "assert is_admin if principal.role == \"admin\";\n",
        "allow manage if is_admin and action == \"manage\";\n",
    );
    let prog = Parser::parse(src).unwrap();
    assert!(!prog.use_schema);
    assert_eq!(prog.assertions.len(), 1);
    assert_eq!(prog.assertions[0].name, "is_admin");
    assert_eq!(prog.rules.len(), 1);
}

#[test]
fn test_parse_skips_test_blocks() {
    let policy = r#"
        allow view if principal == "alice";
        test "something" {
            resource { id: "doc1", type: "document", }
            principal { id: "alice", type: "user", }
            action { id: "view", type: "action", }
            expect allow
        }
        deny delete if action == "delete";
    "#;
    let prog = Parser::parse(policy).unwrap();
    assert_eq!(prog.rules.len(), 2);
    assert_eq!(prog.tests.len(), 0); // Tests skipped
}

#[test]
fn test_parse_with_tests_extracts_tests() {
    let policy = r#"
        allow view if principal == "alice";
        test "alice can view" {
            resource { id: "doc1", type: "document", }
            principal { id: "alice", type: "user", }
            action { id: "view", type: "action", }
            expect allow
        }
    "#;
    let prog = Parser::parse_with_tests(policy).unwrap();
    assert_eq!(prog.rules.len(), 1);
    assert_eq!(prog.tests.len(), 1);
    assert_eq!(prog.tests[0].name, "alice can view");
    assert_eq!(prog.tests[0].entities.len(), 3);
    assert!(matches!(
        prog.tests[0].expected,
        ExpectedOutcome::Simple(EffectAst::Allow)
    ));
}

#[test]
fn test_parse_with_tests_entity_fields() {
    let policy = r#"
        allow access;
        test "field test" {
            principal { id: "bob", role: "admin", }
            expect allow
        }
    "#;
    let prog = Parser::parse_with_tests(policy).unwrap();
    assert_eq!(prog.tests[0].entities.len(), 1);
    let entity = &prog.tests[0].entities[0];
    assert_eq!(entity.kind, "principal");
    assert_eq!(entity.fields.len(), 2);
    assert_eq!(entity.fields[0].0, "id");
    assert_eq!(entity.fields[0].1, serde_json::json!("bob"));
    assert_eq!(entity.fields[1].0, "role");
    assert_eq!(entity.fields[1].1, serde_json::json!("admin"));
}

#[test]
fn test_parse_no_tests() {
    let policy = r#"allow view; deny delete;"#;
    let prog = Parser::parse_with_tests(policy).unwrap();
    assert_eq!(prog.rules.len(), 2);
    assert_eq!(prog.tests.len(), 0);
}

#[cfg(test)]
mod schema_tests {
    use super::*;
    use crate::schema::{EntityKind, TypeRef};

    #[test]
    fn test_parse_use_schema() {
        let prog = Parser::parse("use schema;\nallow access;").unwrap();
        assert!(prog.use_schema);
        assert_eq!(prog.rules.len(), 1);
    }

    #[test]
    fn test_parse_without_use_schema() {
        let prog = Parser::parse("allow access;").unwrap();
        assert!(!prog.use_schema);
    }

    #[test]
    fn test_parse_empty_module() {
        let src = "use schema;\nmod MyNs {};";
        let prog = Parser::parse(src).unwrap();
        assert_eq!(prog.modules.len(), 1);
        assert_eq!(prog.modules[0].name.as_deref(), Some("MyNs"));
        assert!(prog.modules[0].entities.is_empty());
        assert!(prog.modules[0].actions.is_empty());
    }

    #[test]
    fn test_parse_entity_actor() {
        let src = "use schema;\nmod NS {\n    actor User {\n        name String,\n        email String\n    };\n};";
        let prog = Parser::parse(src).unwrap();
        assert_eq!(prog.modules[0].entities.len(), 1);
        let entity = &prog.modules[0].entities[0];
        assert_eq!(entity.kind, EntityKind::Actor);
        assert_eq!(entity.name, "User");
        assert_eq!(entity.fields.len(), 2);
        assert_eq!(entity.fields[0].name, "name");
        assert!(!entity.fields[0].optional);
    }

    #[test]
    fn test_parse_entity_with_parent() {
        let src = "use schema;\nmod NS { resource File in Folder { name String }; };";
        let prog = Parser::parse(src).unwrap();
        let entity = &prog.modules[0].entities[0];
        assert_eq!(entity.kind, EntityKind::Resource);
        assert_eq!(entity.name, "File");
        assert_eq!(entity.parents, vec!["Folder"]);
    }

    #[test]
    fn test_parse_entity_empty_fields() {
        let src = "use schema;\nmod NS { resource Folder {}; };";
        let prog = Parser::parse(src).unwrap();
        let entity = &prog.modules[0].entities[0];
        assert_eq!(entity.name, "Folder");
        assert!(entity.fields.is_empty());
    }

    #[test]
    fn test_parse_optional_field() {
        let src = "use schema;\nmod NS { actor User { email? String }; };";
        let prog = Parser::parse(src).unwrap();
        let field = &prog.modules[0].entities[0].fields[0];
        assert_eq!(field.name, "email");
        assert!(field.optional);
    }

    #[test]
    fn test_parse_nullable_field() {
        let src = "use schema;\nmod NS { actor User { email String | null }; };";
        let prog = Parser::parse(src).unwrap();
        let field = &prog.modules[0].entities[0].fields[0];
        assert!(!field.optional);
        assert!(matches!(&field.ty, TypeRef::Union(types) if types.len() == 2));
    }

    #[test]
    fn test_parse_action_basic() {
        let src = "use schema;\nmod NS {\n    action \"Delete\" appliesTo {\n        actor User,\n        resource File | Folder\n    };\n};";
        let prog = Parser::parse(src).unwrap();
        let action = &prog.modules[0].actions[0];
        assert_eq!(action.name, "Delete");
        let at = action.applies_to.as_ref().unwrap();
        assert_eq!(at.actors, vec!["User"]);
        assert_eq!(at.resources, vec!["File", "Folder"]);
        assert!(at.context.is_none());
    }

    #[test]
    fn test_parse_action_with_context() {
        let src = "use schema;\nmod NS {\n    action \"Update\" appliesTo {\n        actor User,\n        resource Doc,\n        context {\n            authenticated Boolean,\n            tags? Set<String>\n        }\n    };\n};";
        let prog = Parser::parse(src).unwrap();
        let ctx = prog.modules[0].actions[0]
            .applies_to
            .as_ref()
            .unwrap()
            .context
            .as_ref()
            .unwrap();
        assert_eq!(ctx.len(), 2);
        assert_eq!(ctx[0].name, "authenticated");
        assert!(!ctx[0].optional);
        assert_eq!(ctx[1].name, "tags");
        assert!(ctx[1].optional);
        assert!(matches!(&ctx[1].ty, TypeRef::Set(_)));
    }

    #[test]
    fn test_parse_set_type() {
        let src = "use schema;\nmod NS { actor User { roles Set<String> }; };";
        let prog = Parser::parse(src).unwrap();
        let ty = &prog.modules[0].entities[0].fields[0].ty;
        if let TypeRef::Set(inner) = ty {
            assert!(matches!(inner.as_ref(), TypeRef::Named(n) if n == "String"));
        } else {
            panic!("Expected Set type, got {:?}", ty);
        }
    }

    #[test]
    fn test_parse_assert_with_type_params() {
        let src = "use schema;\nassert user_is_owner<User, action, File> if actor.name == resource.owner.name;";
        let prog = Parser::parse(src).unwrap();
        assert_eq!(prog.assertions.len(), 1);
        let a = &prog.assertions[0];
        assert_eq!(a.name, "user_is_owner");
        assert_eq!(a.type_params, vec!["User", "action", "File"]);
        assert!(matches!(&a.body, ExprAst::Compare { .. }));
    }

    #[test]
    fn test_parse_assert_is() {
        let src = "use schema;\nassert user_has_roles is actor has roles;";
        let prog = Parser::parse(src).unwrap();
        let a = &prog.assertions[0];
        assert_eq!(a.name, "user_has_roles");
        assert!(a.type_params.is_empty());
    }

    #[test]
    fn test_parse_type_ref_rule() {
        // New format uses type references as conditions instead of scoped rules
        let src =
            "use schema;\nallow delete_file if MyCedarNamespace:Delete and context.authenticated;";
        let prog = Parser::parse(src).unwrap();
        let rule = &prog.rules[0];
        assert_eq!(rule.name, "delete_file");
        assert_eq!(rule.effect, EffectAst::Allow);
        assert!(rule.body.is_some());
    }

    #[test]
    fn test_parse_full_typed_schema() {
        let src = concat!(
            "use schema;\n",
            "mod MyCedarNamespace {\n",
            "    actor User { name String };\n",
            "    resource Folder {};\n",
            "    resource File in Folder { owner User, name String, modified String };\n",
            "    action \"Delete\" appliesTo {\n",
            "        actor User,\n",
            "        resource File | Folder,\n",
            "        context { authenticated Boolean, somethingOptional? String, somethingNullable String | null }\n",
            "    };\n",
            "    abstract Ownable { owner User };\n",
            "};\n",
            "assert user_is_owner<User, action, File> if actor.name == resource.owner.name;\n",
            "assert user_has_roles is actor has roles;\n",
            "allow delete_file if MyCedarNamespace:Delete and context.authenticated and user_is_owner;\n",
        );
        let prog = Parser::parse(src).unwrap();
        assert!(prog.use_schema);
        assert_eq!(prog.modules.len(), 1);
        assert_eq!(prog.modules[0].entities.len(), 3);
        assert_eq!(prog.modules[0].actions.len(), 1);
        assert_eq!(prog.modules[0].abstracts.len(), 1);
        assert_eq!(prog.modules[0].abstracts[0].name, "Ownable");
        assert_eq!(prog.assertions.len(), 2);
        assert_eq!(prog.rules.len(), 1);
    }

    #[test]
    fn test_parse_unnamed_module() {
        let src = "use schema;\nmod { actor User { name String }; };";
        let prog = Parser::parse(src).unwrap();
        assert_eq!(prog.modules.len(), 1);
        assert!(prog.modules[0].name.is_none());
        assert_eq!(prog.modules[0].entities.len(), 1);
    }

    #[test]
    fn test_parse_entity_with_trait() {
        let src = concat!(
            "use schema;\n",
            "mod Ns {\n",
            "    abstract Ownable { owner String };\n",
            "    resource File is Ownable { name String };\n",
            "};\n",
        );
        let prog = Parser::parse(src).unwrap();
        let file_entity = &prog.modules[0].entities[0];
        assert_eq!(file_entity.name, "File");
        assert_eq!(file_entity.traits, vec!["Ownable"]);
        assert_eq!(file_entity.fields.len(), 1);
    }

    #[test]
    fn test_parse_type_ref_expression() {
        // Type references like `MyCedarNamespace:Delete` in rule conditions
        let src = "use schema;\nallow do_delete if MyCedarNamespace:Delete;";
        let prog = Parser::parse(src).unwrap();
        assert_eq!(prog.rules.len(), 1);
        let body = prog.rules[0].body.as_ref().unwrap();
        // Should parse as TypeRef with namespace and name
        match body {
            ExprAst::TypeRef { namespace, name } => {
                assert_eq!(namespace.as_deref(), Some("MyCedarNamespace"));
                assert_eq!(name, "Delete");
            }
            _ => panic!("Expected TypeRef, got {:?}", body),
        }
    }

    #[test]
    fn test_parse_import() {
        let prog = Parser::parse(
            r#"import "rules.karu";
allow view;"#,
        )
        .unwrap();
        assert_eq!(prog.imports.len(), 1);
        assert_eq!(prog.imports[0], "rules.karu");
        assert_eq!(prog.rules.len(), 1);
    }

    #[test]
    fn test_parse_multiple_imports() {
        let prog = Parser::parse(
            r#"import "a.karu";
import "b.karu";
import "c.karu";
allow view;"#,
        )
        .unwrap();
        assert_eq!(prog.imports.len(), 3);
        assert_eq!(prog.imports[0], "a.karu");
        assert_eq!(prog.imports[1], "b.karu");
        assert_eq!(prog.imports[2], "c.karu");
    }

    #[test]
    fn test_parse_import_with_schema() {
        let prog = Parser::parse(
            r#"use schema;
import "types.karu";
mod { actor User {}; };
allow view;"#,
        )
        .unwrap();
        assert!(prog.use_schema);
        assert_eq!(prog.imports.len(), 1);
        assert_eq!(prog.imports[0], "types.karu");
    }

    #[test]
    fn test_parse_import_after_rule_fails() {
        let result = Parser::parse(
            r#"allow view;
import "late.karu";"#,
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.message
                .contains("import statements must appear at the top"),
            "Expected ordering error, got: {}",
            err.message
        );
    }

    #[test]
    fn test_no_imports_field_empty() {
        let prog = Parser::parse("allow view;").unwrap();
        assert!(prog.imports.is_empty());
    }
}
