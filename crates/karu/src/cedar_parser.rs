// SPDX-License-Identifier: MIT

//! Parser for Cedar policy language.
//!
//! Provides a proper recursive-descent parser for the full Cedar grammar
//! as defined in <https://docs.cedarpolicy.com/policies/syntax-grammar.html>.
//!
//! This parser is fail-fast: it returns `Err` on the first syntax error.
//! For error-tolerant parsing (LSP), use the tree-sitter Cedar grammar instead.

use std::fmt;

// ============================================================================
// Cedar AST Types
// ============================================================================

/// A Cedar policy file containing one or more policies.
#[derive(Debug, Clone)]
pub struct CedarPolicySet {
    pub policies: Vec<CedarPolicy>,
}

/// A single Cedar policy.
#[derive(Debug, Clone)]
pub struct CedarPolicy {
    pub annotations: Vec<CedarAnnotation>,
    pub effect: CedarEffect,
    pub scope: CedarScope,
    pub conditions: Vec<CedarCondition>,
}

/// Policy effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CedarEffect {
    Permit,
    Forbid,
}

/// Annotation on a policy (`@key("value")`).
#[derive(Debug, Clone)]
pub struct CedarAnnotation {
    pub key: String,
    pub value: Option<String>,
}

/// Policy scope (principal, action, resource constraints).
#[derive(Debug, Clone)]
pub struct CedarScope {
    pub principal: CedarScopeConstraint,
    pub action: CedarActionConstraint,
    pub resource: CedarScopeConstraint,
}

/// A scope constraint for principal or resource.
#[derive(Debug, Clone)]
pub enum CedarScopeConstraint {
    /// Matches anything (just `principal` or `resource`).
    Any,
    /// `== Entity`
    Eq(CedarEntityRef),
    /// `in Entity`
    In(CedarEntityRef),
    /// `is Path`
    Is(String),
    /// `is Path in Entity`
    IsIn(String, CedarEntityRef),
    /// `== ?principal` or `== ?resource` (template slot)
    Slot(String),
}

/// Action constraint.
#[derive(Debug, Clone)]
pub enum CedarActionConstraint {
    /// Matches any action.
    Any,
    /// `== Entity`
    Eq(CedarEntityRef),
    /// `in Entity`
    In(CedarEntityRef),
    /// `in [Entity, Entity, ...]`
    InList(Vec<CedarEntityRef>),
}

/// Entity reference like `Type::"id"`.
#[derive(Debug, Clone)]
pub struct CedarEntityRef {
    pub path: Vec<String>,
    pub id: String,
}

/// When/unless condition.
#[derive(Debug, Clone)]
pub struct CedarCondition {
    pub is_when: bool, // true = when, false = unless
    pub expr: CedarExpr,
}

/// Cedar expression.
#[derive(Debug, Clone)]
pub enum CedarExpr {
    /// `if cond then a else b`
    IfThenElse {
        cond: Box<CedarExpr>,
        then_expr: Box<CedarExpr>,
        else_expr: Box<CedarExpr>,
    },
    /// `a || b`
    Or(Box<CedarExpr>, Box<CedarExpr>),
    /// `a && b`
    And(Box<CedarExpr>, Box<CedarExpr>),
    /// Relational: `a == b`, `a != b`, `a < b`, etc.
    Relation {
        lhs: Box<CedarExpr>,
        op: CedarRelOp,
        rhs: Box<CedarExpr>,
    },
    /// `a in b`
    InExpr {
        lhs: Box<CedarExpr>,
        rhs: Box<CedarExpr>,
    },
    /// `a has field`
    Has(Box<CedarExpr>, String),
    /// `a like "pattern*"`
    Like(Box<CedarExpr>, String),
    /// `a is Type` or `a is Type in b`
    Is {
        expr: Box<CedarExpr>,
        type_name: String,
        in_expr: Option<Box<CedarExpr>>,
    },
    /// `a + b`, `a - b`
    Add(Box<CedarExpr>, CedarAddOp, Box<CedarExpr>),
    /// `a * b`
    Mul(Box<CedarExpr>, Box<CedarExpr>),
    /// `!a`
    Not(Box<CedarExpr>),
    /// `-a`
    Neg(Box<CedarExpr>),
    /// Member access: `a.field`
    Access(Box<CedarExpr>, String),
    /// Method call: `a.method(args)`
    MethodCall(Box<CedarExpr>, String, Vec<CedarExpr>),
    /// Index: `a["key"]`
    Index(Box<CedarExpr>, String),
    /// Variable: `principal`, `action`, `resource`, `context`
    Var(String),
    /// Integer literal
    Int(i64),
    /// String literal
    Str(String),
    /// Boolean literal
    Bool(bool),
    /// Entity reference: `Type::"id"`
    Entity(CedarEntityRef),
    /// Extension function: `ip("10.0.0.1")`, `decimal("1.23")`
    ExtFun(String, Vec<CedarExpr>),
    /// Set literal: `[a, b, c]`
    Set(Vec<CedarExpr>),
    /// Record literal: `{key: val, ...}`
    Record(Vec<(String, CedarExpr)>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CedarRelOp {
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CedarAddOp {
    Add,
    Sub,
}

// ============================================================================
// Cedar Lexer
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
enum Token {
    // Keywords
    Permit,
    Forbid,
    When,
    Unless,
    If,
    Then,
    Else,
    In,
    Like,
    Has,
    Is,
    True,
    False,
    Principal,
    Action,
    Resource,
    Context,
    // Symbols
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Semi,
    Dot,
    ColonColon,
    At,
    Colon,
    // Operators
    Eq,    // ==
    Neq,   // !=
    Lt,    // <
    Lte,   // <=
    Gt,    // >
    Gte,   // >=
    And,   // &&
    Or,    // ||
    Not,   // !
    Plus,  // +
    Minus, // -
    Star,  // *
    // Literals
    Ident(String),
    Str(String),
    Int(i64),
    // Template slots
    SlotPrincipal, // ?principal
    SlotResource,  // ?resource
    // EOF
    Eof,
}

/// A token with source position.
#[derive(Debug, Clone)]
struct Spanned {
    token: Token,
    line: usize,
    column: usize,
}

/// Parse error.
#[derive(Debug, Clone)]
pub struct CedarParseError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl fmt::Display for CedarParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Cedar parse error at line {}:{}: {}",
            self.line, self.column, self.message
        )
    }
}

impl std::error::Error for CedarParseError {}

/// Cedar lexer.
struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    column: usize,
}

impl Lexer {
    fn new(source: &str) -> Self {
        Self {
            chars: source.chars().collect(),
            pos: 0,
            line: 1,
            column: 1,
        }
    }

    fn tokenize(mut self) -> Result<Vec<Spanned>, CedarParseError> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            if self.pos >= self.chars.len() {
                tokens.push(Spanned {
                    token: Token::Eof,
                    line: self.line,
                    column: self.column,
                });
                break;
            }
            let line = self.line;
            let column = self.column;
            let tok = self.next_token()?;
            tokens.push(Spanned {
                token: tok,
                line,
                column,
            });
        }
        Ok(tokens)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<char> {
        self.chars.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> char {
        let ch = self.chars[self.pos];
        self.pos += 1;
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        ch
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            // Skip whitespace
            while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
                self.advance();
            }
            // Skip line comments
            if self.pos + 1 < self.chars.len()
                && self.chars[self.pos] == '/'
                && self.chars[self.pos + 1] == '/'
            {
                while self.pos < self.chars.len() && self.chars[self.pos] != '\n' {
                    self.advance();
                }
                continue;
            }
            // Skip block comments
            if self.pos + 1 < self.chars.len()
                && self.chars[self.pos] == '/'
                && self.chars[self.pos + 1] == '*'
            {
                self.advance(); // /
                self.advance(); // *
                loop {
                    if self.pos + 1 >= self.chars.len() {
                        break;
                    }
                    if self.chars[self.pos] == '*' && self.chars[self.pos + 1] == '/' {
                        self.advance(); // *
                        self.advance(); // /
                        break;
                    }
                    self.advance();
                }
                continue;
            }
            break;
        }
    }

    fn next_token(&mut self) -> Result<Token, CedarParseError> {
        let ch = self.peek().unwrap();

        match ch {
            '(' => {
                self.advance();
                Ok(Token::LParen)
            }
            ')' => {
                self.advance();
                Ok(Token::RParen)
            }
            '[' => {
                self.advance();
                Ok(Token::LBracket)
            }
            ']' => {
                self.advance();
                Ok(Token::RBracket)
            }
            '{' => {
                self.advance();
                Ok(Token::LBrace)
            }
            '}' => {
                self.advance();
                Ok(Token::RBrace)
            }
            ',' => {
                self.advance();
                Ok(Token::Comma)
            }
            ';' => {
                self.advance();
                Ok(Token::Semi)
            }
            '.' => {
                self.advance();
                Ok(Token::Dot)
            }
            '@' => {
                self.advance();
                Ok(Token::At)
            }
            '+' => {
                self.advance();
                Ok(Token::Plus)
            }
            '-' => {
                self.advance();
                // Could be negative number or minus operator
                // Leave disambiguation to parser
                Ok(Token::Minus)
            }
            '*' => {
                self.advance();
                Ok(Token::Star)
            }
            ':' if self.peek2() == Some(':') => {
                self.advance();
                self.advance();
                Ok(Token::ColonColon)
            }
            ':' => {
                self.advance();
                Ok(Token::Colon)
            }
            '=' if self.peek2() == Some('=') => {
                self.advance();
                self.advance();
                Ok(Token::Eq)
            }
            '!' if self.peek2() == Some('=') => {
                self.advance();
                self.advance();
                Ok(Token::Neq)
            }
            '!' => {
                self.advance();
                Ok(Token::Not)
            }
            '<' if self.peek2() == Some('=') => {
                self.advance();
                self.advance();
                Ok(Token::Lte)
            }
            '<' => {
                self.advance();
                Ok(Token::Lt)
            }
            '>' if self.peek2() == Some('=') => {
                self.advance();
                self.advance();
                Ok(Token::Gte)
            }
            '>' => {
                self.advance();
                Ok(Token::Gt)
            }
            '&' if self.peek2() == Some('&') => {
                self.advance();
                self.advance();
                Ok(Token::And)
            }
            '|' if self.peek2() == Some('|') => {
                self.advance();
                self.advance();
                Ok(Token::Or)
            }
            '?' => {
                self.advance();
                let ident = self.read_ident();
                match ident.as_str() {
                    "principal" => Ok(Token::SlotPrincipal),
                    "resource" => Ok(Token::SlotResource),
                    _ => Err(self.err(format!("Unknown template slot: ?{}", ident))),
                }
            }
            '"' => self.read_string(),
            c if c.is_ascii_digit() => self.read_int(),
            c if c.is_ascii_alphabetic() || c == '_' => {
                let ident = self.read_ident();
                Ok(match ident.as_str() {
                    "permit" => Token::Permit,
                    "forbid" => Token::Forbid,
                    "when" => Token::When,
                    "unless" => Token::Unless,
                    "if" => Token::If,
                    "then" => Token::Then,
                    "else" => Token::Else,
                    "in" => Token::In,
                    "like" => Token::Like,
                    "has" => Token::Has,
                    "is" => Token::Is,
                    "true" => Token::True,
                    "false" => Token::False,
                    "principal" => Token::Principal,
                    "action" => Token::Action,
                    "resource" => Token::Resource,
                    "context" => Token::Context,
                    _ => Token::Ident(ident),
                })
            }
            _ => Err(self.err(format!("Unexpected character: '{}'", ch))),
        }
    }

    fn read_ident(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                s.push(self.advance());
            } else {
                break;
            }
        }
        s
    }

    fn read_string(&mut self) -> Result<Token, CedarParseError> {
        self.advance(); // opening "
        let mut s = String::new();
        loop {
            match self.peek() {
                Some('"') => {
                    self.advance();
                    return Ok(Token::Str(s));
                }
                Some('\\') => {
                    self.advance();
                    match self.peek() {
                        Some('n') => {
                            self.advance();
                            s.push('\n');
                        }
                        Some('t') => {
                            self.advance();
                            s.push('\t');
                        }
                        Some('\\') => {
                            self.advance();
                            s.push('\\');
                        }
                        Some('"') => {
                            self.advance();
                            s.push('"');
                        }
                        Some('0') => {
                            self.advance();
                            s.push('\0');
                        }
                        Some('*') => {
                            self.advance();
                            s.push('*'); // for like patterns
                        }
                        Some(c) => {
                            self.advance();
                            s.push('\\');
                            s.push(c);
                        }
                        None => return Err(self.err("Unterminated string")),
                    }
                }
                Some(c) => {
                    self.advance();
                    s.push(c);
                }
                None => return Err(self.err("Unterminated string")),
            }
        }
    }

    fn read_int(&mut self) -> Result<Token, CedarParseError> {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                s.push(self.advance());
            } else {
                break;
            }
        }
        let val: i64 = s
            .parse()
            .map_err(|_| self.err(format!("Invalid integer: {}", s)))?;
        Ok(Token::Int(val))
    }

    fn err(&self, message: impl Into<String>) -> CedarParseError {
        CedarParseError {
            message: message.into(),
            line: self.line,
            column: self.column,
        }
    }
}

// ============================================================================
// Parser
// ============================================================================

/// Cedar parser.
struct Parser {
    tokens: Vec<Spanned>,
    pos: usize,
}

/// Parse Cedar source into a policy set.
pub fn parse(source: &str) -> Result<CedarPolicySet, CedarParseError> {
    let tokens = Lexer::new(source).tokenize()?;
    let mut parser = Parser { tokens, pos: 0 };
    parser.parse_policy_set()
}

impl Parser {
    fn current(&self) -> &Spanned {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn current_token(&self) -> &Token {
        &self.current().token
    }

    fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.pos].token;
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, expected: &Token) -> Result<(), CedarParseError> {
        if self.current_token() == expected {
            self.advance();
            Ok(())
        } else {
            Err(self.err(format!(
                "Expected {:?}, found {:?}",
                expected,
                self.current_token()
            )))
        }
    }

    fn err(&self, message: impl Into<String>) -> CedarParseError {
        let sp = self.current();
        CedarParseError {
            message: message.into(),
            line: sp.line,
            column: sp.column,
        }
    }

    // --- Policy Set ---

    fn parse_policy_set(&mut self) -> Result<CedarPolicySet, CedarParseError> {
        let mut policies = Vec::new();
        while *self.current_token() != Token::Eof {
            policies.push(self.parse_policy()?);
        }
        Ok(CedarPolicySet { policies })
    }

    // --- Policy ---

    fn parse_policy(&mut self) -> Result<CedarPolicy, CedarParseError> {
        let annotations = self.parse_annotations()?;
        let effect = self.parse_effect()?;
        self.expect(&Token::LParen)?;
        let scope = self.parse_scope()?;
        self.expect(&Token::RParen)?;
        let conditions = self.parse_conditions()?;
        self.expect(&Token::Semi)?;
        Ok(CedarPolicy {
            annotations,
            effect,
            scope,
            conditions,
        })
    }

    // --- Annotations ---

    fn parse_annotations(&mut self) -> Result<Vec<CedarAnnotation>, CedarParseError> {
        let mut annots = Vec::new();
        while *self.current_token() == Token::At {
            self.advance(); // @
            let key = self.expect_ident()?;
            let value = if *self.current_token() == Token::LParen {
                self.advance();
                let val = self.expect_str()?;
                self.expect(&Token::RParen)?;
                Some(val)
            } else {
                None
            };
            annots.push(CedarAnnotation { key, value });
        }
        Ok(annots)
    }

    // --- Effect ---

    fn parse_effect(&mut self) -> Result<CedarEffect, CedarParseError> {
        match self.current_token() {
            Token::Permit => {
                self.advance();
                Ok(CedarEffect::Permit)
            }
            Token::Forbid => {
                self.advance();
                Ok(CedarEffect::Forbid)
            }
            _ => Err(self.err("Expected 'permit' or 'forbid'")),
        }
    }

    // --- Scope ---

    fn parse_scope(&mut self) -> Result<CedarScope, CedarParseError> {
        let principal = self.parse_principal_constraint()?;
        self.expect(&Token::Comma)?;
        let action = self.parse_action_constraint()?;
        self.expect(&Token::Comma)?;
        let resource = self.parse_resource_constraint()?;
        Ok(CedarScope {
            principal,
            action,
            resource,
        })
    }

    fn parse_principal_constraint(&mut self) -> Result<CedarScopeConstraint, CedarParseError> {
        self.expect(&Token::Principal)?;
        self.parse_entity_constraint("principal")
    }

    fn parse_resource_constraint(&mut self) -> Result<CedarScopeConstraint, CedarParseError> {
        self.expect(&Token::Resource)?;
        self.parse_entity_constraint("resource")
    }

    fn parse_entity_constraint(
        &mut self,
        slot_name: &str,
    ) -> Result<CedarScopeConstraint, CedarParseError> {
        match self.current_token() {
            Token::Eq => {
                self.advance();
                if *self.current_token() == Token::SlotPrincipal
                    || *self.current_token() == Token::SlotResource
                {
                    let slot = format!("?{}", slot_name);
                    self.advance();
                    Ok(CedarScopeConstraint::Slot(slot))
                } else {
                    let entity = self.parse_entity_ref()?;
                    Ok(CedarScopeConstraint::Eq(entity))
                }
            }
            Token::In => {
                self.advance();
                if *self.current_token() == Token::SlotPrincipal
                    || *self.current_token() == Token::SlotResource
                {
                    let slot = format!("?{}", slot_name);
                    self.advance();
                    Ok(CedarScopeConstraint::Slot(slot))
                } else {
                    let entity = self.parse_entity_ref()?;
                    Ok(CedarScopeConstraint::In(entity))
                }
            }
            Token::Is => {
                self.advance();
                let type_name = self.parse_path_string()?;
                if *self.current_token() == Token::In {
                    self.advance();
                    let entity = self.parse_entity_ref()?;
                    Ok(CedarScopeConstraint::IsIn(type_name, entity))
                } else {
                    Ok(CedarScopeConstraint::Is(type_name))
                }
            }
            _ => Ok(CedarScopeConstraint::Any),
        }
    }

    fn parse_action_constraint(&mut self) -> Result<CedarActionConstraint, CedarParseError> {
        self.expect(&Token::Action)?;
        match self.current_token() {
            Token::Eq => {
                self.advance();
                let entity = self.parse_entity_ref()?;
                Ok(CedarActionConstraint::Eq(entity))
            }
            Token::In => {
                self.advance();
                if *self.current_token() == Token::LBracket {
                    self.advance();
                    let mut entities = Vec::new();
                    if *self.current_token() != Token::RBracket {
                        entities.push(self.parse_entity_ref()?);
                        while *self.current_token() == Token::Comma {
                            self.advance();
                            entities.push(self.parse_entity_ref()?);
                        }
                    }
                    self.expect(&Token::RBracket)?;
                    Ok(CedarActionConstraint::InList(entities))
                } else {
                    let entity = self.parse_entity_ref()?;
                    Ok(CedarActionConstraint::In(entity))
                }
            }
            _ => Ok(CedarActionConstraint::Any),
        }
    }

    // --- Entity Reference ---

    fn parse_entity_ref(&mut self) -> Result<CedarEntityRef, CedarParseError> {
        let mut path = Vec::new();
        path.push(self.expect_ident()?);
        while *self.current_token() == Token::ColonColon {
            self.advance();
            // Next could be a string (the entity ID) or another ident (namespace)
            match self.current_token().clone() {
                Token::Str(s) => {
                    self.advance();
                    return Ok(CedarEntityRef { path, id: s });
                }
                Token::Ident(_) => {
                    path.push(self.expect_ident()?);
                }
                _ => return Err(self.err("Expected identifier or string after '::'")),
            }
        }
        Err(self.err(format!(
            "Expected entity reference (Type::\"id\"), got path {:?} without string ID",
            path
        )))
    }

    fn parse_path_string(&mut self) -> Result<String, CedarParseError> {
        let mut path = self.expect_ident()?;
        while *self.current_token() == Token::ColonColon {
            self.advance();
            if let Token::Ident(_) = self.current_token() {
                path.push_str("::");
                path.push_str(&self.expect_ident()?);
            } else {
                break;
            }
        }
        Ok(path)
    }

    // --- Conditions ---

    fn parse_conditions(&mut self) -> Result<Vec<CedarCondition>, CedarParseError> {
        let mut conditions = Vec::new();
        loop {
            match self.current_token() {
                Token::When => {
                    self.advance();
                    self.expect(&Token::LBrace)?;
                    let expr = self.parse_expr()?;
                    self.expect(&Token::RBrace)?;
                    conditions.push(CedarCondition {
                        is_when: true,
                        expr,
                    });
                }
                Token::Unless => {
                    self.advance();
                    self.expect(&Token::LBrace)?;
                    let expr = self.parse_expr()?;
                    self.expect(&Token::RBrace)?;
                    conditions.push(CedarCondition {
                        is_when: false,
                        expr,
                    });
                }
                _ => break,
            }
        }
        Ok(conditions)
    }

    // --- Expressions ---

    fn parse_expr(&mut self) -> Result<CedarExpr, CedarParseError> {
        if *self.current_token() == Token::If {
            self.advance();
            let cond = self.parse_expr()?;
            self.expect(&Token::Then)?;
            let then_expr = self.parse_expr()?;
            self.expect(&Token::Else)?;
            let else_expr = self.parse_expr()?;
            Ok(CedarExpr::IfThenElse {
                cond: Box::new(cond),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
            })
        } else {
            self.parse_or()
        }
    }

    fn parse_or(&mut self) -> Result<CedarExpr, CedarParseError> {
        let mut lhs = self.parse_and()?;
        while *self.current_token() == Token::Or {
            self.advance();
            let rhs = self.parse_and()?;
            lhs = CedarExpr::Or(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<CedarExpr, CedarParseError> {
        let mut lhs = self.parse_relation()?;
        while *self.current_token() == Token::And {
            self.advance();
            let rhs = self.parse_relation()?;
            lhs = CedarExpr::And(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_relation(&mut self) -> Result<CedarExpr, CedarParseError> {
        let lhs = self.parse_add()?;

        match self.current_token() {
            Token::Eq => {
                self.advance();
                let rhs = self.parse_add()?;
                Ok(CedarExpr::Relation {
                    lhs: Box::new(lhs),
                    op: CedarRelOp::Eq,
                    rhs: Box::new(rhs),
                })
            }
            Token::Neq => {
                self.advance();
                let rhs = self.parse_add()?;
                Ok(CedarExpr::Relation {
                    lhs: Box::new(lhs),
                    op: CedarRelOp::Neq,
                    rhs: Box::new(rhs),
                })
            }
            Token::Lt => {
                self.advance();
                let rhs = self.parse_add()?;
                Ok(CedarExpr::Relation {
                    lhs: Box::new(lhs),
                    op: CedarRelOp::Lt,
                    rhs: Box::new(rhs),
                })
            }
            Token::Lte => {
                self.advance();
                let rhs = self.parse_add()?;
                Ok(CedarExpr::Relation {
                    lhs: Box::new(lhs),
                    op: CedarRelOp::Lte,
                    rhs: Box::new(rhs),
                })
            }
            Token::Gt => {
                self.advance();
                let rhs = self.parse_add()?;
                Ok(CedarExpr::Relation {
                    lhs: Box::new(lhs),
                    op: CedarRelOp::Gt,
                    rhs: Box::new(rhs),
                })
            }
            Token::Gte => {
                self.advance();
                let rhs = self.parse_add()?;
                Ok(CedarExpr::Relation {
                    lhs: Box::new(lhs),
                    op: CedarRelOp::Gte,
                    rhs: Box::new(rhs),
                })
            }
            Token::In => {
                self.advance();
                let rhs = self.parse_add()?;
                Ok(CedarExpr::InExpr {
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                })
            }
            Token::Has => {
                self.advance();
                let field = match self.current_token().clone() {
                    Token::Ident(s) => {
                        self.advance();
                        s
                    }
                    Token::Str(s) => {
                        self.advance();
                        s
                    }
                    _ => return Err(self.err("Expected identifier or string after 'has'")),
                };
                Ok(CedarExpr::Has(Box::new(lhs), field))
            }
            Token::Like => {
                self.advance();
                let pattern = self.expect_str()?;
                Ok(CedarExpr::Like(Box::new(lhs), pattern))
            }
            Token::Is => {
                self.advance();
                let type_name = self.parse_path_string()?;
                let in_expr = if *self.current_token() == Token::In {
                    self.advance();
                    Some(Box::new(self.parse_add()?))
                } else {
                    None
                };
                Ok(CedarExpr::Is {
                    expr: Box::new(lhs),
                    type_name,
                    in_expr,
                })
            }
            _ => Ok(lhs),
        }
    }

    fn parse_add(&mut self) -> Result<CedarExpr, CedarParseError> {
        let mut lhs = self.parse_mult()?;
        loop {
            match self.current_token() {
                Token::Plus => {
                    self.advance();
                    let rhs = self.parse_mult()?;
                    lhs = CedarExpr::Add(Box::new(lhs), CedarAddOp::Add, Box::new(rhs));
                }
                Token::Minus => {
                    self.advance();
                    let rhs = self.parse_mult()?;
                    lhs = CedarExpr::Add(Box::new(lhs), CedarAddOp::Sub, Box::new(rhs));
                }
                _ => break,
            }
        }
        Ok(lhs)
    }

    fn parse_mult(&mut self) -> Result<CedarExpr, CedarParseError> {
        let mut lhs = self.parse_unary()?;
        while *self.current_token() == Token::Star {
            self.advance();
            let rhs = self.parse_unary()?;
            lhs = CedarExpr::Mul(Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<CedarExpr, CedarParseError> {
        match self.current_token() {
            Token::Not => {
                self.advance();
                let expr = self.parse_unary()?;
                Ok(CedarExpr::Not(Box::new(expr)))
            }
            Token::Minus => {
                self.advance();
                // Check if next is an integer (negative literal)
                if let Token::Int(n) = self.current_token() {
                    let n = *n;
                    self.advance();
                    Ok(CedarExpr::Int(-n))
                } else {
                    let expr = self.parse_unary()?;
                    Ok(CedarExpr::Neg(Box::new(expr)))
                }
            }
            _ => self.parse_member(),
        }
    }

    fn parse_member(&mut self) -> Result<CedarExpr, CedarParseError> {
        let mut expr = self.parse_primary()?;
        loop {
            match self.current_token() {
                Token::Dot => {
                    self.advance();
                    let field = self.expect_ident()?;
                    // Check for method call
                    if *self.current_token() == Token::LParen {
                        self.advance();
                        let args = self.parse_expr_list()?;
                        self.expect(&Token::RParen)?;
                        expr = CedarExpr::MethodCall(Box::new(expr), field, args);
                    } else {
                        expr = CedarExpr::Access(Box::new(expr), field);
                    }
                }
                Token::LBracket => {
                    self.advance();
                    let key = self.expect_str()?;
                    self.expect(&Token::RBracket)?;
                    expr = CedarExpr::Index(Box::new(expr), key);
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<CedarExpr, CedarParseError> {
        match self.current_token().clone() {
            Token::True => {
                self.advance();
                Ok(CedarExpr::Bool(true))
            }
            Token::False => {
                self.advance();
                Ok(CedarExpr::Bool(false))
            }
            Token::Int(n) => {
                self.advance();
                Ok(CedarExpr::Int(n))
            }
            Token::Str(s) => {
                self.advance();
                Ok(CedarExpr::Str(s))
            }
            Token::Principal => {
                self.advance();
                Ok(CedarExpr::Var("principal".into()))
            }
            Token::Action => {
                self.advance();
                Ok(CedarExpr::Var("action".into()))
            }
            Token::Resource => {
                self.advance();
                Ok(CedarExpr::Var("resource".into()))
            }
            Token::Context => {
                self.advance();
                Ok(CedarExpr::Var("context".into()))
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            Token::LBracket => {
                self.advance();
                let items = if *self.current_token() != Token::RBracket {
                    self.parse_expr_list()?
                } else {
                    vec![]
                };
                self.expect(&Token::RBracket)?;
                Ok(CedarExpr::Set(items))
            }
            Token::LBrace => {
                self.advance();
                let mut fields = Vec::new();
                if *self.current_token() != Token::RBrace {
                    loop {
                        let key = match self.current_token().clone() {
                            Token::Ident(s) => {
                                self.advance();
                                s
                            }
                            Token::Str(s) => {
                                self.advance();
                                s
                            }
                            _ => return Err(self.err("Expected field name in record")),
                        };
                        self.expect(&Token::Colon)?;
                        fields.push((key, self.parse_expr()?));
                        if *self.current_token() != Token::Comma {
                            break;
                        }
                        self.advance();
                    }
                }
                self.expect(&Token::RBrace)?;
                Ok(CedarExpr::Record(fields))
            }
            Token::Ident(name) => {
                self.advance();
                // Could be: entity ref (Name::...), ext function, or plain ident
                if *self.current_token() == Token::ColonColon {
                    // Entity ref or namespaced function
                    let mut path = vec![name];
                    while *self.current_token() == Token::ColonColon {
                        self.advance();
                        match self.current_token().clone() {
                            Token::Str(s) => {
                                self.advance();
                                return Ok(CedarExpr::Entity(CedarEntityRef { path, id: s }));
                            }
                            Token::Ident(s) => {
                                self.advance();
                                // Check if next is '(' for ext function
                                if *self.current_token() == Token::LParen {
                                    let fn_name = format!("{}::{}", path.join("::"), s);
                                    self.advance();
                                    let args = self.parse_expr_list()?;
                                    self.expect(&Token::RParen)?;
                                    return Ok(CedarExpr::ExtFun(fn_name, args));
                                }
                                path.push(s);
                            }
                            _ => return Err(self.err("Expected identifier or string after '::'")),
                        }
                    }
                    // If we get here with a path but no string, check for function call
                    if *self.current_token() == Token::LParen {
                        let fn_name = path.join("::");
                        self.advance();
                        let args = self.parse_expr_list()?;
                        self.expect(&Token::RParen)?;
                        Ok(CedarExpr::ExtFun(fn_name, args))
                    } else {
                        Err(self.err(format!("Unexpected path {:?} without entity ID", path)))
                    }
                } else if *self.current_token() == Token::LParen {
                    // Extension function call: name(args)
                    self.advance();
                    let args = self.parse_expr_list()?;
                    self.expect(&Token::RParen)?;
                    Ok(CedarExpr::ExtFun(name, args))
                } else {
                    // Plain identifier - treat as variable
                    Ok(CedarExpr::Var(name))
                }
            }
            _ => Err(self.err(format!("Unexpected token: {:?}", self.current_token()))),
        }
    }

    fn parse_expr_list(&mut self) -> Result<Vec<CedarExpr>, CedarParseError> {
        let mut exprs = Vec::new();
        if *self.current_token() == Token::RParen || *self.current_token() == Token::RBracket {
            return Ok(exprs);
        }
        exprs.push(self.parse_expr()?);
        while *self.current_token() == Token::Comma {
            self.advance();
            exprs.push(self.parse_expr()?);
        }
        Ok(exprs)
    }

    // --- Helpers ---

    fn expect_ident(&mut self) -> Result<String, CedarParseError> {
        match self.current_token().clone() {
            Token::Ident(s) => {
                self.advance();
                Ok(s)
            }
            // Allow keywords as identifiers in some contexts (like annotation names)
            Token::Principal => {
                self.advance();
                Ok("principal".into())
            }
            Token::Action => {
                self.advance();
                Ok("action".into())
            }
            Token::Resource => {
                self.advance();
                Ok("resource".into())
            }
            Token::Context => {
                self.advance();
                Ok("context".into())
            }
            _ => Err(self.err(format!(
                "Expected identifier, found {:?}",
                self.current_token()
            ))),
        }
    }

    fn expect_str(&mut self) -> Result<String, CedarParseError> {
        match self.current_token().clone() {
            Token::Str(s) => {
                self.advance();
                Ok(s)
            }
            _ => Err(self.err(format!("Expected string, found {:?}", self.current_token()))),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_permit() {
        let p = parse(r#"permit(principal, action, resource);"#).unwrap();
        assert_eq!(p.policies.len(), 1);
        assert_eq!(p.policies[0].effect, CedarEffect::Permit);
    }

    #[test]
    fn test_simple_forbid() {
        let p = parse(r#"forbid(principal, action, resource);"#).unwrap();
        assert_eq!(p.policies.len(), 1);
        assert_eq!(p.policies[0].effect, CedarEffect::Forbid);
    }

    #[test]
    fn test_entity_constraint() {
        let p = parse(r#"permit(principal == User::"alice", action, resource);"#).unwrap();
        if let CedarScopeConstraint::Eq(ref entity) = p.policies[0].scope.principal {
            assert_eq!(entity.path, vec!["User"]);
            assert_eq!(entity.id, "alice");
        } else {
            panic!("Expected Eq constraint");
        }
    }

    #[test]
    fn test_action_eq() {
        let p = parse(r#"permit(principal, action == Action::"view", resource);"#).unwrap();
        if let CedarActionConstraint::Eq(ref entity) = p.policies[0].scope.action {
            assert_eq!(entity.id, "view");
        } else {
            panic!("Expected action Eq constraint");
        }
    }

    #[test]
    fn test_action_in_list() {
        let p =
            parse(r#"permit(principal, action in [Action::"view", Action::"edit"], resource);"#)
                .unwrap();
        if let CedarActionConstraint::InList(ref entities) = p.policies[0].scope.action {
            assert_eq!(entities.len(), 2);
            assert_eq!(entities[0].id, "view");
            assert_eq!(entities[1].id, "edit");
        } else {
            panic!("Expected InList constraint");
        }
    }

    #[test]
    fn test_when_condition() {
        let p = parse(r#"permit(principal, action, resource) when { resource.public == true };"#)
            .unwrap();
        assert_eq!(p.policies[0].conditions.len(), 1);
        assert!(p.policies[0].conditions[0].is_when);
    }

    #[test]
    fn test_unless_condition() {
        let p =
            parse(r#"forbid(principal, action, resource) unless { principal == resource.owner };"#)
                .unwrap();
        assert_eq!(p.policies[0].conditions.len(), 1);
        assert!(!p.policies[0].conditions[0].is_when);
    }

    #[test]
    fn test_when_and_unless() {
        let p = parse(
            r#"forbid(principal, action, resource) 
               when { resource.private == true }
               unless { principal == resource.owner };"#,
        )
        .unwrap();
        assert_eq!(p.policies[0].conditions.len(), 2);
        assert!(p.policies[0].conditions[0].is_when);
        assert!(!p.policies[0].conditions[1].is_when);
    }

    #[test]
    fn test_annotation() {
        let p = parse(r#"@id("policy1") permit(principal, action, resource);"#).unwrap();
        assert_eq!(p.policies[0].annotations.len(), 1);
        assert_eq!(p.policies[0].annotations[0].key, "id");
        assert_eq!(p.policies[0].annotations[0].value, Some("policy1".into()));
    }

    #[test]
    fn test_complex_when_expr() {
        let p = parse(
            r#"permit(principal, action, resource) 
               when { principal.department == "Engineering" && principal.level >= 5 };"#,
        )
        .unwrap();
        // Should parse the && expression
        let cond = &p.policies[0].conditions[0];
        assert!(matches!(&cond.expr, CedarExpr::And(_, _)));
    }

    #[test]
    fn test_has_operator() {
        let p = parse(
            r#"permit(principal, action, resource)
               when { context has readOnly && context.readOnly == true };"#,
        )
        .unwrap();
        let cond = &p.policies[0].conditions[0];
        if let CedarExpr::And(ref lhs, _) = cond.expr {
            assert!(matches!(lhs.as_ref(), CedarExpr::Has(_, _)));
        } else {
            panic!("Expected And expression");
        }
    }

    #[test]
    fn test_or_expression() {
        let p = parse(
            r#"permit(principal, action, resource)
               when { action == Action::"view" || action == Action::"list" };"#,
        )
        .unwrap();
        let cond = &p.policies[0].conditions[0];
        assert!(matches!(&cond.expr, CedarExpr::Or(_, _)));
    }

    #[test]
    fn test_not_expression() {
        let p = parse(
            r#"permit(principal, action, resource)
               when { !resource.private };"#,
        )
        .unwrap();
        let cond = &p.policies[0].conditions[0];
        assert!(matches!(&cond.expr, CedarExpr::Not(_)));
    }

    #[test]
    fn test_method_call() {
        let p = parse(
            r#"permit(principal, action, resource)
               when { resource.admins.contains(principal) };"#,
        )
        .unwrap();
        let cond = &p.policies[0].conditions[0];
        if let CedarExpr::MethodCall(_, ref method, ref args) = cond.expr {
            assert_eq!(method, "contains");
            assert_eq!(args.len(), 1);
        } else {
            panic!("Expected MethodCall, got {:?}", cond.expr);
        }
    }

    #[test]
    fn test_set_literal() {
        let p = parse(
            r#"permit(principal, action, resource)
               when { action in [Action::"view", Action::"edit"] };"#,
        )
        .unwrap();
        let cond = &p.policies[0].conditions[0];
        if let CedarExpr::InExpr { rhs, .. } = &cond.expr {
            assert!(matches!(rhs.as_ref(), CedarExpr::Set(_)));
        } else {
            panic!("Expected InExpr with Set");
        }
    }

    #[test]
    fn test_multiple_policies() {
        let p = parse(
            r#"
            permit(principal == User::"alice", action, resource);
            forbid(principal, action, resource)
              when { resource.private == true };
            "#,
        )
        .unwrap();
        assert_eq!(p.policies.len(), 2);
        assert_eq!(p.policies[0].effect, CedarEffect::Permit);
        assert_eq!(p.policies[1].effect, CedarEffect::Forbid);
    }

    #[test]
    fn test_resource_in_entity() {
        let p = parse(r#"permit(principal, action, resource in Album::"vacation");"#).unwrap();
        if let CedarScopeConstraint::In(ref entity) = p.policies[0].scope.resource {
            assert_eq!(entity.path, vec!["Album"]);
            assert_eq!(entity.id, "vacation");
        } else {
            panic!("Expected In constraint");
        }
    }

    #[test]
    fn test_principal_is_type() {
        let p = parse(r#"permit(principal is User, action, resource);"#).unwrap();
        if let CedarScopeConstraint::Is(ref type_name) = p.policies[0].scope.principal {
            assert_eq!(type_name, "User");
        } else {
            panic!("Expected Is constraint");
        }
    }

    #[test]
    fn test_if_then_else() {
        let p = parse(
            r#"permit(principal, action, resource)
               when { if principal.admin then true else false };"#,
        )
        .unwrap();
        let cond = &p.policies[0].conditions[0];
        assert!(matches!(&cond.expr, CedarExpr::IfThenElse { .. }));
    }

    #[test]
    fn test_extension_function() {
        let p = parse(
            r#"permit(principal, action, resource)
               when { ip("192.168.0.1").isInRange(ip("192.168.0.0/24")) };"#,
        )
        .unwrap();
        let cond = &p.policies[0].conditions[0];
        // ip(...).isInRange(ip(...))
        if let CedarExpr::MethodCall(ref base, ref method, _) = cond.expr {
            assert_eq!(method, "isInRange");
            assert!(matches!(base.as_ref(), CedarExpr::ExtFun(..)));
        } else {
            panic!("Expected MethodCall, got {:?}", cond.expr);
        }
    }

    #[test]
    fn test_comments() {
        let p = parse(
            r#"
            // This is a comment
            permit(principal, action, resource); // inline comment
            /* block comment */
            forbid(principal, action, resource);
            "#,
        )
        .unwrap();
        assert_eq!(p.policies.len(), 2);
    }

    #[test]
    fn test_like_operator() {
        let p = parse(
            r#"permit(principal, action, resource)
               when { principal.name like "j*" };"#,
        )
        .unwrap();
        let cond = &p.policies[0].conditions[0];
        if let CedarExpr::Like(_, ref pat) = cond.expr {
            assert_eq!(pat, "j*");
        } else {
            panic!("Expected Like expression");
        }
    }

    #[test]
    fn test_parse_error_position() {
        let result = parse("permit(principal, action, );");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.line >= 1);
    }

    #[test]
    fn test_unexpected_character() {
        let result = parse("permit(principal, action, resource) ^");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.message, "Unexpected character: '^'");
    }

    #[test]
    fn test_template_slot() {
        let p =
            parse(r#"permit(principal == ?principal, action, resource in ?resource);"#).unwrap();
        assert!(matches!(
            p.policies[0].scope.principal,
            CedarScopeConstraint::Slot(_)
        ));
    }

    #[test]
    fn test_member_index() {
        let p = parse(
            r#"permit(principal, action, resource)
               when { context["key"] == "value" };"#,
        )
        .unwrap();
        let cond = &p.policies[0].conditions[0];
        if let CedarExpr::Relation { ref lhs, .. } = cond.expr {
            assert!(matches!(lhs.as_ref(), CedarExpr::Index(_, _)));
        } else {
            panic!("Expected Relation with Index");
        }
    }
}
