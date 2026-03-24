//! Parser for Cedar schema language (`.cedarschema` files).
//!
//! Converts Cedar schema source into Karu's `ModuleDef` AST types for
//! interoperability with Cedar-based authorization systems.
//!
//! # Supported Cedar Schema Features
//!
//! - `namespace Ns { ... }` → `ModuleDef { name: Some("Ns"), ... }`
//! - `entity User { ... }` → `EntityDef { kind: Actor, ... }`
//! - `entity File in [Folder] { ... }` → `EntityDef { parents: ["Folder"], ... }`
//! - `action "Delete" appliesTo { ... }` → `ActionDef { ... }`
//! - `type Ownable = { ... }` → `AbstractDef { ... }`
//! - `Set<T>` → `TypeRef::Set(T)`
//! - Inline record types `{ field: Type }` → `TypeRef::Record(...)`
//! - Annotations `@id("value")` are parsed and discarded
//!
//! # Example
//!
//! ```rust,ignore
//! use karu::cedar_schema_parser::parse_cedarschema;
//!
//! let schema = r#"
//!     namespace PhotoApp {
//!         entity User {};
//!         entity Photo {
//!             owner: User,
//!             name: String,
//!         };
//!     }
//! "#;
//!
//! let modules = parse_cedarschema(schema).unwrap();
//! assert_eq!(modules.len(), 1);
//! assert_eq!(modules[0].name, Some("PhotoApp".to_string()));
//! ```

use crate::schema::*;
use std::fmt;

// ============================================================================
// Error type
// ============================================================================

/// Error during Cedar schema parsing.
#[derive(Debug, Clone)]
pub struct CedarSchemaError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl fmt::Display for CedarSchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Cedar schema error at line {}:{}: {}",
            self.line, self.column, self.message
        )
    }
}

impl std::error::Error for CedarSchemaError {}

// ============================================================================
// Tokens - reuse the same token set as cedar_parser
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
enum Token {
    // Symbols
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LParen,
    RParen,
    Comma,
    Semi,
    Colon,
    ColonColon,
    At,
    Eq,       // ==  (used in `= RecType` for entity body)
    Assign,   // =   (used in `type X = ...`)
    Lt,       // <
    Gt,       // >
    Pipe,     // |
    Question, // ?
    // Literals
    Ident(String),
    Str(String),
    Int(i64),
    // EOF
    Eof,
}

#[derive(Debug, Clone)]
struct Spanned {
    token: Token,
    line: usize,
    column: usize,
}

// ============================================================================
// Lexer
// ============================================================================

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

    fn tokenize(mut self) -> Result<Vec<Spanned>, CedarSchemaError> {
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
            while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
                self.advance();
            }
            if self.pos + 1 < self.chars.len()
                && self.chars[self.pos] == '/'
                && self.chars[self.pos + 1] == '/'
            {
                while self.pos < self.chars.len() && self.chars[self.pos] != '\n' {
                    self.advance();
                }
                continue;
            }
            if self.pos + 1 < self.chars.len()
                && self.chars[self.pos] == '/'
                && self.chars[self.pos + 1] == '*'
            {
                self.advance();
                self.advance();
                loop {
                    if self.pos + 1 >= self.chars.len() {
                        break;
                    }
                    if self.chars[self.pos] == '*' && self.chars[self.pos + 1] == '/' {
                        self.advance();
                        self.advance();
                        break;
                    }
                    self.advance();
                }
                continue;
            }
            break;
        }
    }

    fn next_token(&mut self) -> Result<Token, CedarSchemaError> {
        let ch = self.peek().unwrap();
        match ch {
            '{' => {
                self.advance();
                Ok(Token::LBrace)
            }
            '}' => {
                self.advance();
                Ok(Token::RBrace)
            }
            '[' => {
                self.advance();
                Ok(Token::LBracket)
            }
            ']' => {
                self.advance();
                Ok(Token::RBracket)
            }
            '(' => {
                self.advance();
                Ok(Token::LParen)
            }
            ')' => {
                self.advance();
                Ok(Token::RParen)
            }
            ',' => {
                self.advance();
                Ok(Token::Comma)
            }
            ';' => {
                self.advance();
                Ok(Token::Semi)
            }
            '@' => {
                self.advance();
                Ok(Token::At)
            }
            '<' => {
                self.advance();
                Ok(Token::Lt)
            }
            '>' => {
                self.advance();
                Ok(Token::Gt)
            }
            '|' => {
                self.advance();
                Ok(Token::Pipe)
            }
            '?' => {
                self.advance();
                Ok(Token::Question)
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
            '=' => {
                self.advance();
                Ok(Token::Assign)
            }
            '"' => self.read_string(),
            c if c.is_ascii_digit() => self.read_int(),
            c if c.is_ascii_alphabetic() || c == '_' => {
                let ident = self.read_ident();
                Ok(Token::Ident(ident))
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

    fn read_string(&mut self) -> Result<Token, CedarSchemaError> {
        self.advance(); // opening "
        let mut s = String::new();
        loop {
            match self.peek() {
                None => return Err(self.err("Unterminated string")),
                Some('"') => {
                    self.advance();
                    return Ok(Token::Str(s));
                }
                Some('\\') => {
                    self.advance();
                    match self.peek() {
                        Some('"') => {
                            self.advance();
                            s.push('"');
                        }
                        Some('\\') => {
                            self.advance();
                            s.push('\\');
                        }
                        Some('n') => {
                            self.advance();
                            s.push('\n');
                        }
                        Some('t') => {
                            self.advance();
                            s.push('\t');
                        }
                        Some('*') => {
                            self.advance();
                            s.push('*');
                        }
                        _ => return Err(self.err("Invalid escape sequence")),
                    }
                }
                Some(c) => {
                    self.advance();
                    s.push(c);
                }
            }
        }
    }

    fn read_int(&mut self) -> Result<Token, CedarSchemaError> {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                s.push(self.advance());
            } else {
                break;
            }
        }
        s.parse::<i64>()
            .map(Token::Int)
            .map_err(|_| self.err(format!("Invalid integer: {}", s)))
    }

    fn err(&self, message: impl Into<String>) -> CedarSchemaError {
        CedarSchemaError {
            message: message.into(),
            line: self.line,
            column: self.column,
        }
    }
}

// ============================================================================
// Parser
// ============================================================================

struct Parser {
    tokens: Vec<Spanned>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Spanned>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos].token
    }

    fn at_ident(&self, name: &str) -> bool {
        matches!(self.peek(), Token::Ident(s) if s == name)
    }

    fn advance(&mut self) -> &Spanned {
        let sp = &self.tokens[self.pos];
        self.pos += 1;
        sp
    }

    fn expect(&mut self, expected: &Token) -> Result<(), CedarSchemaError> {
        if self.peek() == expected {
            self.advance();
            Ok(())
        } else {
            Err(self.err(format!("expected {:?}, got {:?}", expected, self.peek())))
        }
    }

    fn expect_ident(&mut self, name: &str) -> Result<(), CedarSchemaError> {
        if self.at_ident(name) {
            self.advance();
            Ok(())
        } else {
            Err(self.err(format!("expected '{}', got {:?}", name, self.peek())))
        }
    }

    fn consume_ident(&mut self) -> Result<String, CedarSchemaError> {
        match self.peek().clone() {
            Token::Ident(s) => {
                let name = s.clone();
                self.advance();
                Ok(name)
            }
            _ => Err(self.err(format!("expected identifier, got {:?}", self.peek()))),
        }
    }

    fn consume_name(&mut self) -> Result<String, CedarSchemaError> {
        // A Name is either an IDENT or a STR.
        match self.peek().clone() {
            Token::Ident(s) => {
                let name = s.clone();
                self.advance();
                Ok(name)
            }
            Token::Str(s) => {
                let name = s.clone();
                self.advance();
                Ok(name)
            }
            _ => Err(self.err(format!(
                "expected name (ident or string), got {:?}",
                self.peek()
            ))),
        }
    }

    fn err(&self, message: impl Into<String>) -> CedarSchemaError {
        let sp = &self.tokens[self.pos.min(self.tokens.len() - 1)];
        CedarSchemaError {
            message: message.into(),
            line: sp.line,
            column: sp.column,
        }
    }

    // ========================================================================
    // Schema → Vec<ModuleDef>
    // ========================================================================

    /// Parse the top-level schema: either namespaced or bare declarations.
    fn parse_schema(&mut self) -> Result<Vec<ModuleDef>, CedarSchemaError> {
        let mut modules = Vec::new();
        while *self.peek() != Token::Eof {
            // Skip annotations at top level
            self.skip_annotations()?;

            if self.at_ident("namespace") {
                modules.push(self.parse_namespace()?);
            } else if self.at_ident("entity") || self.at_ident("action") || self.at_ident("type") {
                // Bare declaration (no namespace) → unnamed module
                let decls = self.parse_decl()?;
                // Find or create the unnamed module
                if modules.is_empty() || modules.last().unwrap().name.is_some() {
                    modules.push(ModuleDef {
                        name: None,
                        entities: vec![],
                        actions: vec![],
                        abstracts: vec![],
                    });
                }
                let m = modules.last_mut().unwrap();
                for decl in decls {
                    match decl {
                        Decl::Entity(e) => m.entities.push(e),
                        Decl::Action(a) => m.actions.push(a),
                        Decl::TypeDecl(t) => m.abstracts.push(t),
                    }
                }
            } else if *self.peek() == Token::Eof {
                break;
            } else {
                return Err(self.err(format!(
                    "expected 'namespace', 'entity', 'action', or 'type', got {:?}",
                    self.peek()
                )));
            }
        }
        Ok(modules)
    }

    // ========================================================================
    // Namespace
    // ========================================================================

    fn parse_namespace(&mut self) -> Result<ModuleDef, CedarSchemaError> {
        self.expect_ident("namespace")?;
        let name = self.parse_path()?;
        self.expect(&Token::LBrace)?;

        let mut module = ModuleDef {
            name: Some(name),
            entities: vec![],
            actions: vec![],
            abstracts: vec![],
        };

        while *self.peek() != Token::RBrace {
            self.skip_annotations()?;
            if *self.peek() == Token::RBrace {
                break;
            }
            let decls = self.parse_decl()?;
            for decl in decls {
                match decl {
                    Decl::Entity(e) => module.entities.push(e),
                    Decl::Action(a) => module.actions.push(a),
                    Decl::TypeDecl(t) => module.abstracts.push(t),
                }
            }
        }

        self.expect(&Token::RBrace)?;
        // Optional trailing semicolon
        if *self.peek() == Token::Semi {
            self.advance();
        }

        Ok(module)
    }

    // ========================================================================
    // Declarations
    // ========================================================================

    /// Internal enum for parsed declarations.
    fn parse_decl(&mut self) -> Result<Vec<Decl>, CedarSchemaError> {
        if self.at_ident("entity") {
            Ok(self
                .parse_entities()?
                .into_iter()
                .map(Decl::Entity)
                .collect())
        } else if self.at_ident("action") {
            Ok(self
                .parse_actions()?
                .into_iter()
                .map(Decl::Action)
                .collect())
        } else if self.at_ident("type") {
            Ok(vec![Decl::TypeDecl(self.parse_type_decl()?)])
        } else {
            Err(self.err(format!(
                "expected 'entity', 'action', or 'type', got {:?}",
                self.peek()
            )))
        }
    }

    // ========================================================================
    // Entity
    // ========================================================================

    fn parse_entities(&mut self) -> Result<Vec<EntityDef>, CedarSchemaError> {
        self.expect_ident("entity")?;

        // Parse one or more comma-separated entity names
        let names = self.parse_ident_list()?;

        // Parse optional `in [Parent, ...]` or `in Parent`
        let parents = if self.at_ident("in") {
            self.advance();
            self.parse_ent_or_typs()?
        } else {
            vec![]
        };

        // Parse optional fields: either `= { ... }` or `{ ... }`
        let fields = if *self.peek() == Token::Assign {
            self.advance();
            self.parse_record_fields()?
        } else if *self.peek() == Token::LBrace {
            self.parse_record_fields()?
        } else {
            vec![]
        };

        // Skip `tags Type` if present (we don't model tags yet)
        if self.at_ident("tags") {
            self.advance();
            self._parse_type()?; // consume and discard
        }

        self.expect(&Token::Semi)?;

        // Create one EntityDef per name (multi-entity: `entity A, B { ... }`)
        Ok(names
            .into_iter()
            .map(|name| EntityDef {
                kind: EntityKind::Resource,
                name,
                parents: parents.clone(),
                traits: vec![],
                fields: fields.clone(),
            })
            .collect())
    }

    // ========================================================================
    // Action
    // ========================================================================

    fn parse_actions(&mut self) -> Result<Vec<ActionDef>, CedarSchemaError> {
        self.expect_ident("action")?;

        // Parse one or more comma-separated action names
        let mut names = vec![self.consume_name()?];
        while *self.peek() == Token::Comma {
            self.advance();
            names.push(self.consume_name()?);
        }

        // Parse optional `in Ref` or `in [Ref, ...]`
        if self.at_ident("in") {
            self.advance();
            self.parse_ref_or_refs()?; // consume and discard parent actions
        }

        // Parse optional `appliesTo { ... }`
        let applies_to = if self.at_ident("appliesTo") {
            self.advance();
            Some(self.parse_applies_to()?)
        } else {
            None
        };

        self.expect(&Token::Semi)?;

        // Create one ActionDef per name
        Ok(names
            .into_iter()
            .map(|name| ActionDef {
                name,
                applies_to: applies_to.clone(),
            })
            .collect())
    }

    fn parse_applies_to(&mut self) -> Result<ActionAppliesTo, CedarSchemaError> {
        self.expect(&Token::LBrace)?;

        let mut actors = Vec::new();
        let mut resources = Vec::new();
        let mut context = None;

        while *self.peek() != Token::RBrace {
            if self.at_ident("principal") {
                self.advance();
                self.expect(&Token::Colon)?;
                actors = self.parse_ent_or_typs()?;
            } else if self.at_ident("resource") {
                self.advance();
                self.expect(&Token::Colon)?;
                resources = self.parse_ent_or_typs()?;
            } else if self.at_ident("context") {
                self.advance();
                self.expect(&Token::Colon)?;
                if *self.peek() == Token::LBrace {
                    context = Some(self.parse_record_fields()?);
                } else {
                    // Context is a named type reference - just consume it
                    self._parse_type()?;
                }
            } else {
                return Err(self.err(format!(
                    "expected 'principal', 'resource', or 'context' in appliesTo, got {:?}",
                    self.peek()
                )));
            }

            // Optional comma between declarations
            if *self.peek() == Token::Comma {
                self.advance();
            }
        }

        self.expect(&Token::RBrace)?;

        Ok(ActionAppliesTo {
            actors,
            resources,
            context,
        })
    }

    // ========================================================================
    // Type declaration (Cedar's `type X = { ... }`)
    // ========================================================================

    fn parse_type_decl(&mut self) -> Result<AbstractDef, CedarSchemaError> {
        self.expect_ident("type")?;
        let name = self.consume_ident()?;
        self.expect(&Token::Assign)?;
        let ty = self._parse_type()?;
        self.expect(&Token::Semi)?;

        // Convert the type to fields if it's a record, otherwise create
        // an abstract with no fields (just the name).
        let fields = match ty {
            TypeRef::Record(fields) => fields,
            _ => vec![],
        };

        Ok(AbstractDef { name, fields })
    }

    // ========================================================================
    // Type parsing
    // ========================================================================

    fn _parse_type(&mut self) -> Result<TypeRef, CedarSchemaError> {
        let base = self.parse_base_type()?;

        // Check for union types: `A | B`
        if *self.peek() == Token::Pipe {
            let mut variants = vec![base];
            while *self.peek() == Token::Pipe {
                self.advance();
                variants.push(self.parse_base_type()?);
            }
            Ok(TypeRef::Union(variants))
        } else {
            Ok(base)
        }
    }

    fn parse_base_type(&mut self) -> Result<TypeRef, CedarSchemaError> {
        match self.peek().clone() {
            Token::LBrace => {
                let fields = self.parse_record_fields()?;
                Ok(TypeRef::Record(fields))
            }
            Token::Ident(ref name) if name == "Set" => {
                self.advance();
                self.expect(&Token::Lt)?;
                let inner = self._parse_type()?;
                self.expect(&Token::Gt)?;
                Ok(TypeRef::Set(Box::new(inner)))
            }
            Token::Ident(_) => {
                let name = self.parse_path()?;
                Ok(TypeRef::Named(name))
            }
            _ => Err(self.err(format!("expected type, got {:?}", self.peek()))),
        }
    }

    // ========================================================================
    // Record fields: { name: Type, name?: Type, ... }
    // ========================================================================

    fn parse_record_fields(&mut self) -> Result<Vec<FieldDef>, CedarSchemaError> {
        self.expect(&Token::LBrace)?;
        let mut fields = Vec::new();

        while *self.peek() != Token::RBrace {
            self.skip_annotations()?;
            if *self.peek() == Token::RBrace {
                break;
            }

            let name = self.consume_name()?;

            // Optional `?` for optional fields
            let optional = if *self.peek() == Token::Question {
                self.advance();
                true
            } else {
                false
            };

            self.expect(&Token::Colon)?;
            let ty = self._parse_type()?;

            fields.push(FieldDef { name, ty, optional });

            // Optional trailing comma
            if *self.peek() == Token::Comma {
                self.advance();
            }
        }

        self.expect(&Token::RBrace)?;
        Ok(fields)
    }

    // ========================================================================
    // Helpers
    // ========================================================================

    /// Parse a Path: `IDENT { '::' IDENT }`
    fn parse_path(&mut self) -> Result<String, CedarSchemaError> {
        let mut path = self.consume_ident()?;
        while *self.peek() == Token::ColonColon {
            self.advance();
            let seg = self.consume_ident()?;
            path.push_str("::");
            path.push_str(&seg);
        }
        Ok(path)
    }

    /// Parse a list of comma-separated identifiers.
    fn parse_ident_list(&mut self) -> Result<Vec<String>, CedarSchemaError> {
        let mut names = vec![self.consume_ident()?];
        while *self.peek() == Token::Comma {
            // Peek ahead to see if next is an ident (not a keyword like `in`)
            let saved = self.pos;
            self.advance(); // skip comma
            if matches!(self.peek(), Token::Ident(_)) {
                names.push(self.consume_ident()?);
            } else {
                // Wasn't an ident - backtrack
                self.pos = saved;
                break;
            }
        }
        Ok(names)
    }

    /// Parse entity or type references: either a single path or `[Path, Path, ...]`
    fn parse_ent_or_typs(&mut self) -> Result<Vec<String>, CedarSchemaError> {
        if *self.peek() == Token::LBracket {
            self.advance();
            let mut types = Vec::new();
            while *self.peek() != Token::RBracket {
                types.push(self.parse_path()?);
                if *self.peek() == Token::Comma {
                    self.advance();
                }
            }
            self.expect(&Token::RBracket)?;
            Ok(types)
        } else {
            Ok(vec![self.parse_path()?])
        }
    }

    /// Parse ref or refs (for action parents): single ref or `[ref, ...]`
    /// A ref can be an ident path, a string name, or a path::string entity ref.
    fn parse_ref_or_refs(&mut self) -> Result<Vec<String>, CedarSchemaError> {
        if *self.peek() == Token::LBracket {
            self.advance();
            let mut refs = Vec::new();
            while *self.peek() != Token::RBracket {
                refs.push(self.parse_ref()?);
                if *self.peek() == Token::Comma {
                    self.advance();
                }
            }
            self.expect(&Token::RBracket)?;
            Ok(refs)
        } else {
            Ok(vec![self.parse_ref()?])
        }
    }

    /// Parse a single ref: either a string name or a path (optionally followed by ::"str")
    fn parse_ref(&mut self) -> Result<String, CedarSchemaError> {
        match self.peek().clone() {
            Token::Str(s) => {
                let name = s.clone();
                self.advance();
                Ok(name)
            }
            Token::Ident(_) => {
                let path = self.parse_path()?;
                // Optional `::` STR for entity refs like `Type::"id"`
                if *self.peek() == Token::ColonColon {
                    self.advance();
                    if let Token::Str(_) = self.peek() {
                        self.advance();
                    }
                }
                Ok(path)
            }
            _ => Err(self.err(format!(
                "expected ref (ident or string), got {:?}",
                self.peek()
            ))),
        }
    }

    /// Skip annotations: `@ident("value")` or `@ident`
    fn skip_annotations(&mut self) -> Result<(), CedarSchemaError> {
        while *self.peek() == Token::At {
            self.advance(); // @
            self.consume_ident()?; // annotation name
            if *self.peek() == Token::LParen {
                self.advance();
                // Consume the string value if present
                if let Token::Str(_) = self.peek() {
                    self.advance();
                }
                self.expect(&Token::RParen)?;
            }
        }
        Ok(())
    }
}

/// Internal enum for dispatching parsed declarations.
#[allow(clippy::enum_variant_names)]
enum Decl {
    Entity(EntityDef),
    Action(ActionDef),
    TypeDecl(AbstractDef),
}

// ============================================================================
// Public API
// ============================================================================

/// Parse a Cedar schema source string into a list of Karu `ModuleDef`s.
///
/// Each Cedar `namespace` becomes a `ModuleDef`. Bare declarations outside
/// any namespace go into an unnamed `ModuleDef`.
pub fn parse_cedarschema(source: &str) -> Result<Vec<ModuleDef>, CedarSchemaError> {
    let tokens = Lexer::new(source).tokenize()?;
    let mut parser = Parser::new(tokens);
    parser.parse_schema()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_namespace() {
        let modules = parse_cedarschema("namespace Foo {}").unwrap();
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, Some("Foo".to_string()));
        assert!(modules[0].entities.is_empty());
    }

    #[test]
    fn test_entity_simple() {
        let modules = parse_cedarschema(
            r#"
            namespace PhotoApp {
                entity User {};
            }
        "#,
        )
        .unwrap();
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].entities.len(), 1);
        assert_eq!(modules[0].entities[0].name, "User");
        assert!(modules[0].entities[0].fields.is_empty());
    }

    #[test]
    fn test_entity_with_fields() {
        let modules = parse_cedarschema(
            r#"
            namespace App {
                entity User {
                    name: String,
                    age: Long,
                    active?: Bool,
                };
            }
        "#,
        )
        .unwrap();
        let user = &modules[0].entities[0];
        assert_eq!(user.fields.len(), 3);
        assert_eq!(user.fields[0].name, "name");
        assert!(!user.fields[0].optional);
        assert_eq!(user.fields[2].name, "active");
        assert!(user.fields[2].optional);
    }

    #[test]
    fn test_entity_in_parent() {
        let modules = parse_cedarschema(
            r#"
            namespace App {
                entity Folder {};
                entity File in [Folder] {
                    owner: User,
                    name: String,
                };
            }
        "#,
        )
        .unwrap();
        let file = &modules[0].entities[1];
        assert_eq!(file.name, "File");
        assert_eq!(file.parents, vec!["Folder"]);
        assert_eq!(file.fields.len(), 2);
    }

    #[test]
    fn test_action_simple() {
        let modules = parse_cedarschema(
            r#"
            namespace App {
                entity User {};
                entity Photo {};
                action "viewPhoto" appliesTo {
                    principal: User,
                    resource: Photo,
                };
            }
        "#,
        )
        .unwrap();
        assert_eq!(modules[0].actions.len(), 1);
        assert_eq!(modules[0].actions[0].name, "viewPhoto");
        let at = modules[0].actions[0].applies_to.as_ref().unwrap();
        assert_eq!(at.actors, vec!["User"]);
        assert_eq!(at.resources, vec!["Photo"]);
    }

    #[test]
    fn test_action_with_context() {
        let modules = parse_cedarschema(
            r#"
            namespace App {
                entity User {};
                entity File {};
                action "delete" appliesTo {
                    principal: User,
                    resource: File,
                    context: {
                        authenticated: Bool,
                        ip?: String,
                    },
                };
            }
        "#,
        )
        .unwrap();
        let ctx = modules[0].actions[0]
            .applies_to
            .as_ref()
            .unwrap()
            .context
            .as_ref()
            .unwrap();
        assert_eq!(ctx.len(), 2);
        assert_eq!(ctx[0].name, "authenticated");
        assert!(!ctx[0].optional);
        assert_eq!(ctx[1].name, "ip");
        assert!(ctx[1].optional);
    }

    #[test]
    fn test_type_decl() {
        let modules = parse_cedarschema(
            r#"
            namespace App {
                type Address = {
                    street: String,
                    city: String,
                };
            }
        "#,
        )
        .unwrap();
        assert_eq!(modules[0].abstracts.len(), 1);
        assert_eq!(modules[0].abstracts[0].name, "Address");
        assert_eq!(modules[0].abstracts[0].fields.len(), 2);
    }

    #[test]
    fn test_set_type() {
        let modules = parse_cedarschema(
            r#"
            namespace App {
                entity User {
                    tags: Set<String>,
                };
            }
        "#,
        )
        .unwrap();
        let field = &modules[0].entities[0].fields[0];
        assert_eq!(field.name, "tags");
        assert!(matches!(field.ty, TypeRef::Set(_)));
    }

    #[test]
    fn test_annotations_skipped() {
        let modules = parse_cedarschema(
            r#"
            @doc("This is a schema")
            namespace App {
                @doc("A user")
                entity User {
                    @doc("The name")
                    name: String,
                };
            }
        "#,
        )
        .unwrap();
        assert_eq!(modules[0].entities[0].name, "User");
        assert_eq!(modules[0].entities[0].fields.len(), 1);
    }

    #[test]
    fn test_bare_declarations() {
        let modules = parse_cedarschema(
            r#"
            entity User {};
            entity Photo {
                owner: User,
            };
        "#,
        )
        .unwrap();
        assert_eq!(modules.len(), 1);
        assert!(modules[0].name.is_none());
        assert_eq!(modules[0].entities.len(), 2);
    }

    #[test]
    fn test_multiple_namespaces() {
        let modules = parse_cedarschema(
            r#"
            namespace Auth {
                entity User {};
            }
            namespace Content {
                entity Photo {};
            }
        "#,
        )
        .unwrap();
        assert_eq!(modules.len(), 2);
        assert_eq!(modules[0].name, Some("Auth".to_string()));
        assert_eq!(modules[1].name, Some("Content".to_string()));
    }

    #[test]
    fn test_entity_with_eq_syntax() {
        // Cedar allows `entity Foo = { ... };` with an equals sign
        let modules = parse_cedarschema(
            r#"
            namespace App {
                entity User = {
                    name: String,
                };
            }
        "#,
        )
        .unwrap();
        assert_eq!(modules[0].entities[0].fields.len(), 1);
    }

    #[test]
    fn test_action_with_parent() {
        let modules = parse_cedarschema(
            r#"
            namespace App {
                entity User {};
                entity Photo {};
                action "readPhoto" appliesTo {
                    principal: User,
                    resource: Photo,
                };
                action "listPhotos" in "readPhoto" appliesTo {
                    principal: User,
                    resource: Photo,
                };
            }
        "#,
        )
        .unwrap();
        assert_eq!(modules[0].actions.len(), 2);
        assert_eq!(modules[0].actions[1].name, "listPhotos");
    }

    #[test]
    fn test_union_type() {
        let modules = parse_cedarschema(
            r#"
            namespace App {
                entity Folder {};
                entity File {};
                action "delete" appliesTo {
                    principal: [User],
                    resource: [File, Folder],
                };
                entity User {};
            }
        "#,
        )
        .unwrap();
        let at = modules[0].actions[0].applies_to.as_ref().unwrap();
        assert_eq!(at.resources, vec!["File", "Folder"]);
    }

    #[test]
    fn test_nested_path_namespace() {
        let modules = parse_cedarschema(
            r#"
            namespace Acme::Auth {
                entity User {};
            }
        "#,
        )
        .unwrap();
        assert_eq!(modules[0].name, Some("Acme::Auth".to_string()));
    }

    #[test]
    fn test_comments_skipped() {
        let modules = parse_cedarschema(
            r#"
            // This is a comment
            namespace App {
                // Entity comment
                entity User {};
                /* block comment */
                entity Photo {};
            }
        "#,
        )
        .unwrap();
        assert_eq!(modules[0].entities.len(), 2);
    }
}
