// SPDX-License-Identifier: MIT

//! Lexer for Karu's Polar-inspired syntax.
//!
//! Tokenizes source code into a stream of tokens for the parser.

use std::fmt;
use std::iter::Peekable;
use std::str::Chars;

/// Token types for Karu syntax.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Keywords
    Allow,
    Deny,
    If,
    And,
    Or,
    Not,
    In,
    Forall,
    Exists,
    Has,
    Like_,

    // Schema keywords
    Use,
    Schema,
    Mod,
    Actor,
    Resource,
    Action,
    Assert,
    Abstract,
    Import,
    On,
    Test,
    Expect,
    Is,

    // Identifiers and literals
    Ident(String),
    String(String),
    Number(f64),
    True,
    False,
    Null,

    // Operators
    Eq,       // ==
    Ne,       // !=
    Lt,       // <
    Gt,       // >
    Le,       // <=
    Ge,       // >=
    Dot,      // .
    Comma,    // ,
    Colon,    // :
    Semi,     // ;
    Pipe,     // |
    Question, // ?

    // Delimiters
    LParen,   // (
    RParen,   // )
    LBrace,   // {
    RBrace,   // }
    LBracket, // [
    RBracket, // ]

    // Special
    Underscore, // _ (wildcard)
    Comment(String),
    Eof,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Allow => write!(f, "allow"),
            Token::Deny => write!(f, "deny"),
            Token::If => write!(f, "if"),
            Token::And => write!(f, "and"),
            Token::Or => write!(f, "or"),
            Token::Not => write!(f, "not"),
            Token::In => write!(f, "in"),
            Token::Forall => write!(f, "forall"),
            Token::Exists => write!(f, "exists"),
            Token::Has => write!(f, "has"),
            Token::Like_ => write!(f, "like"),
            Token::Use => write!(f, "use"),
            Token::Schema => write!(f, "schema"),
            Token::Mod => write!(f, "mod"),
            Token::Actor => write!(f, "actor"),
            Token::Resource => write!(f, "resource"),
            Token::Action => write!(f, "action"),
            Token::Assert => write!(f, "assert"),
            Token::Abstract => write!(f, "abstract"),
            Token::Import => write!(f, "import"),
            Token::On => write!(f, "on"),
            Token::Test => write!(f, "test"),
            Token::Expect => write!(f, "expect"),
            Token::Is => write!(f, "is"),
            Token::Ident(s) => write!(f, "{}", s),
            Token::String(s) => write!(f, "\"{}\"", s),
            Token::Number(n) => write!(f, "{}", n),
            Token::True => write!(f, "true"),
            Token::False => write!(f, "false"),
            Token::Null => write!(f, "null"),
            Token::Eq => write!(f, "=="),
            Token::Ne => write!(f, "!="),
            Token::Lt => write!(f, "<"),
            Token::Gt => write!(f, ">"),
            Token::Le => write!(f, "<="),
            Token::Ge => write!(f, ">="),
            Token::Dot => write!(f, "."),
            Token::Comma => write!(f, ","),
            Token::Colon => write!(f, ":"),
            Token::Semi => write!(f, ";"),
            Token::Pipe => write!(f, "|"),
            Token::Question => write!(f, "?"),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::LBrace => write!(f, "{{"),
            Token::RBrace => write!(f, "}}"),
            Token::LBracket => write!(f, "["),
            Token::RBracket => write!(f, "]"),
            Token::Underscore => write!(f, "_"),
            Token::Comment(s) => write!(f, "// {}", s),
            Token::Eof => write!(f, "EOF"),
        }
    }
}

/// A token with its position in the source.
#[derive(Debug, Clone)]
pub struct Spanned {
    pub token: Token,
    pub line: usize,
    pub column: usize,
}

/// Lexer error.
#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for LexError {}

/// Lexer for Karu source code.
pub struct Lexer<'a> {
    chars: Peekable<Chars<'a>>,
    line: usize,
    column: usize,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer from source code.
    pub fn new(source: &'a str) -> Self {
        Self {
            chars: source.chars().peekable(),
            line: 1,
            column: 1,
        }
    }

    /// Tokenize the entire source into a vector of tokens.
    pub fn tokenize(source: &str) -> Result<Vec<Token>, LexError> {
        let spanned = Self::tokenize_spanned(source)?;
        Ok(spanned.into_iter().map(|s| s.token).collect())
    }

    /// Tokenize the entire source into a vector of tokens with position info.
    pub fn tokenize_spanned(source: &str) -> Result<Vec<Spanned>, LexError> {
        let mut lexer = Lexer::new(source);
        let mut tokens = Vec::new();

        loop {
            let spanned = lexer.next_spanned()?;
            let is_eof = spanned.token == Token::Eof;
            // Skip comments in token stream
            if !matches!(spanned.token, Token::Comment(_)) {
                tokens.push(spanned);
            }
            if is_eof {
                break;
            }
        }

        Ok(tokens)
    }

    /// Get the next token with position info.
    pub fn next_spanned(&mut self) -> Result<Spanned, LexError> {
        self.skip_whitespace();
        let line = self.line;
        let column = self.column;
        let token = self.next_token_inner()?;
        Ok(Spanned {
            token,
            line,
            column,
        })
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.chars.next();
        if let Some(ch) = c {
            if ch == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }
        c
    }

    fn peek(&mut self) -> Option<&char> {
        self.chars.peek()
    }

    fn skip_whitespace(&mut self) {
        while let Some(&c) = self.peek() {
            if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn read_string(&mut self) -> Result<String, LexError> {
        let start_line = self.line;
        let start_col = self.column;
        let mut s = String::new();

        loop {
            match self.advance() {
                Some('"') => return Ok(s),
                Some('\\') => match self.advance() {
                    Some('n') => s.push('\n'),
                    Some('t') => s.push('\t'),
                    Some('r') => s.push('\r'),
                    Some('\\') => s.push('\\'),
                    Some('"') => s.push('"'),
                    Some(c) => s.push(c),
                    None => {
                        return Err(LexError {
                            message: "Unterminated string escape".into(),
                            line: self.line,
                            column: self.column,
                        })
                    }
                },
                Some(c) => s.push(c),
                None => {
                    return Err(LexError {
                        message: "Unterminated string".into(),
                        line: start_line,
                        column: start_col,
                    })
                }
            }
        }
    }

    fn read_number(&mut self, first: char) -> Result<f64, LexError> {
        let mut s = String::new();
        s.push(first);

        while let Some(&c) = self.peek() {
            if c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '-' || c == '+' {
                // Handle sign only after e/E
                if (c == '-' || c == '+') && !s.ends_with('e') && !s.ends_with('E') {
                    break;
                }
                s.push(self.advance().unwrap());
            } else {
                break;
            }
        }

        s.parse().map_err(|_| LexError {
            message: format!("Invalid number: {}", s),
            line: self.line,
            column: self.column,
        })
    }

    fn read_ident(&mut self, first: char) -> String {
        let mut s = String::new();
        s.push(first);

        while let Some(&c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                s.push(self.advance().unwrap());
            } else {
                break;
            }
        }

        s
    }

    fn read_comment(&mut self) -> String {
        let mut s = String::new();
        while let Some(&c) = self.peek() {
            if c == '\n' {
                break;
            }
            s.push(self.advance().unwrap());
        }
        s.trim().to_string()
    }

    /// Get the next token (internal, without position tracking).
    fn next_token_inner(&mut self) -> Result<Token, LexError> {
        let c = match self.advance() {
            Some(c) => c,
            None => return Ok(Token::Eof),
        };

        let token = match c {
            '/' if self.peek() == Some(&'/') => {
                self.advance(); // consume second '/'
                Token::Comment(self.read_comment())
            }
            '"' => Token::String(self.read_string()?),
            '(' => Token::LParen,
            ')' => Token::RParen,
            '{' => Token::LBrace,
            '}' => Token::RBrace,
            '[' => Token::LBracket,
            ']' => Token::RBracket,
            ',' => Token::Comma,
            ':' => Token::Colon,
            ';' => Token::Semi,
            '.' => Token::Dot,
            '_' => {
                // Check if it's just underscore or start of identifier
                if let Some(&next) = self.peek() {
                    if next.is_alphanumeric() {
                        let ident = self.read_ident('_');
                        self.keyword_or_ident(&ident)
                    } else {
                        Token::Underscore
                    }
                } else {
                    Token::Underscore
                }
            }
            '=' => {
                if self.peek() == Some(&'=') {
                    self.advance();
                    Token::Eq
                } else {
                    return Err(LexError {
                        message: "Expected '==' for equality".into(),
                        line: self.line,
                        column: self.column,
                    });
                }
            }
            '!' => {
                if self.peek() == Some(&'=') {
                    self.advance();
                    Token::Ne
                } else {
                    return Err(LexError {
                        message: "Expected '!=' for not-equal".into(),
                        line: self.line,
                        column: self.column,
                    });
                }
            }
            '<' => {
                if self.peek() == Some(&'=') {
                    self.advance();
                    Token::Le
                } else {
                    Token::Lt
                }
            }
            '>' => {
                if self.peek() == Some(&'=') {
                    self.advance();
                    Token::Ge
                } else {
                    Token::Gt
                }
            }
            c if c.is_ascii_digit() => Token::Number(self.read_number(c)?),
            '-' if self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) => {
                let first_digit = self.advance().unwrap();
                Token::Number(-self.read_number(first_digit)?)
            }
            c if c.is_alphabetic() || c == '_' => {
                let ident = self.read_ident(c);
                self.keyword_or_ident(&ident)
            }
            '|' => Token::Pipe,
            '?' => Token::Question,
            c => {
                return Err(LexError {
                    message: format!("Unexpected character: '{}'", c),
                    line: self.line,
                    column: self.column,
                })
            }
        };

        Ok(token)
    }

    fn keyword_or_ident(&self, s: &str) -> Token {
        match s {
            "allow" => Token::Allow,
            "deny" => Token::Deny,
            "if" => Token::If,
            "and" => Token::And,
            "or" => Token::Or,
            "not" => Token::Not,
            "in" => Token::In,
            "forall" => Token::Forall,
            "exists" => Token::Exists,
            "has" => Token::Has,
            "like" => Token::Like_,
            "use" => Token::Use,
            "schema" => Token::Schema,
            "mod" => Token::Mod,
            "actor" => Token::Actor,
            "resource" => Token::Resource,
            "action" => Token::Action,
            "assert" => Token::Assert,
            "abstract" => Token::Abstract,
            "import" => Token::Import,
            "on" => Token::On,
            "test" => Token::Test,
            "expect" => Token::Expect,
            "is" => Token::Is,
            "true" => Token::True,
            "false" => Token::False,
            "null" => Token::Null,
            _ => Token::Ident(s.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lex_keywords() {
        let tokens = Lexer::tokenize("allow deny if and or not in forall exists").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Allow,
                Token::Deny,
                Token::If,
                Token::And,
                Token::Or,
                Token::Not,
                Token::In,
                Token::Forall,
                Token::Exists,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_lex_operators() {
        let tokens = Lexer::tokenize("== != < > <= >=").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Eq,
                Token::Ne,
                Token::Lt,
                Token::Gt,
                Token::Le,
                Token::Ge,
                Token::Eof
            ]
        );
    }

    #[test]
    fn test_lex_literals() {
        let tokens = Lexer::tokenize(r#"true false null 42 3.5 "hello""#).unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::True,
                Token::False,
                Token::Null,
                Token::Number(42.0),
                Token::Number(3.5),
                Token::String("hello".into()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_lex_identifiers() {
        let tokens = Lexer::tokenize("foo bar_baz _private").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Ident("foo".into()),
                Token::Ident("bar_baz".into()),
                Token::Ident("_private".into()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_lex_delimiters() {
        let tokens = Lexer::tokenize("() {} [] , : ; . _").unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::LParen,
                Token::RParen,
                Token::LBrace,
                Token::RBrace,
                Token::LBracket,
                Token::RBracket,
                Token::Comma,
                Token::Colon,
                Token::Semi,
                Token::Dot,
                Token::Underscore,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_lex_string_escapes() {
        let tokens = Lexer::tokenize(r#""hello\nworld""#).unwrap();
        assert_eq!(
            tokens,
            vec![Token::String("hello\nworld".into()), Token::Eof]
        );
    }

    #[test]
    fn test_lex_comments_stripped() {
        let tokens = Lexer::tokenize("allow // this is a comment\ndeny").unwrap();
        assert_eq!(tokens, vec![Token::Allow, Token::Deny, Token::Eof]);
    }

    #[test]
    fn test_lex_negative_number() {
        let tokens = Lexer::tokenize("-42 -3.5").unwrap();
        assert_eq!(
            tokens,
            vec![Token::Number(-42.0), Token::Number(-3.5), Token::Eof]
        );
    }

    #[test]
    fn test_lex_rule_example() {
        let src = r#"allow(actor, action, resource) if action == "read";"#;
        let tokens = Lexer::tokenize(src).unwrap();
        assert!(tokens.contains(&Token::Allow));
        assert!(tokens.contains(&Token::If));
        assert!(tokens.contains(&Token::Eq));
        assert!(tokens.contains(&Token::String("read".into())));
    }

    #[test]
    fn test_lex_object_pattern() {
        let src = r#"{ name: "lhs", value: 10 }"#;
        let tokens = Lexer::tokenize(src).unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::LBrace,
                Token::Ident("name".into()),
                Token::Colon,
                Token::String("lhs".into()),
                Token::Comma,
                Token::Ident("value".into()),
                Token::Colon,
                Token::Number(10.0),
                Token::RBrace,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_lex_error_unterminated_string() {
        let result = Lexer::tokenize(r#""hello"#);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Unterminated"));
    }

    #[test]
    fn test_lex_test_and_expect_keywords() {
        let tokens = Lexer::tokenize(r#"test "hello" { expect allow }"#).unwrap();
        assert_eq!(
            tokens,
            vec![
                Token::Test,
                Token::String("hello".into()),
                Token::LBrace,
                Token::Expect,
                Token::Allow,
                Token::RBrace,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_lex_error_unknown_character() {
        let result = Lexer::tokenize("@");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("Unexpected character"));
    }
}
