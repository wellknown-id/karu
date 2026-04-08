// SPDX-License-Identifier: MIT

//! Tree-sitter grammar for Karu policy language.
//!
//! This module defines the Karu grammar using `krust-sitter` annotations,
//! providing error-tolerant parsing with span information for dev tooling (LSP).
//!
//! The grammar produces its own AST types which can be converted to the
//! canonical `ast::*` types used by the compiler and evaluator.

#[allow(clippy::module_inception, clippy::manual_non_exhaustive)]
pub mod grammar {
    use krust_sitter::{Rule, Spanned};

    // ── Token types ──────────────────────────────────────────────────────

    /// Identifier token
    #[derive(Debug, Clone, PartialEq, Eq, Rule)]
    #[leaf(pattern(r"[a-zA-Z_][a-zA-Z0-9_]*"))]
    pub struct KaruIdent;

    /// String literal token (double-quoted)
    #[derive(Debug, Clone, PartialEq, Eq, Rule)]
    #[leaf(pattern(r#""([^"\\]|\\.)*""#))]
    pub struct KaruString;

    /// Numeric literal token
    #[derive(Debug, Clone, PartialEq, Eq, Rule)]
    #[leaf(pattern(r"-?[0-9]+(\.[0-9]+)?([eE][+-]?[0-9]+)?"))]
    pub struct KaruNumber;

    /// Unsigned integer token (for array indices)
    #[derive(Debug, Clone, PartialEq, Eq, Rule)]
    #[leaf(pattern(r"\d+"))]
    pub struct KaruDigits;

    // ── Root ─────────────────────────────────────────────────────────────

    /// Root language node - a Karu program is a list of top-level items.
    #[derive(Debug, Rule)]
    #[language]
    #[extras(re(r"\s"), re(r"//[^\n]*"))]
    #[word(KaruIdent)]
    pub struct Program {
        pub items: Vec<TopLevelItem>,
    }

    /// A top-level item: rule, test, schema directive, mod, assert, or import.
    #[derive(Debug, Rule)]
    pub enum TopLevelItem {
        Rule(RuleDef),
        Test(TestBlock),
        UseSchema(UseSchema),
        Mod(ModDef),
        Assert(AssertDef),
        Import(ImportDef),
    }

    // ── Import ────────────────────────────────────────────────────────────

    /// `import "path/to/file.karu";`
    #[derive(Debug, Rule)]
    pub struct ImportDef {
        #[leaf("import")]
        _import_kw: (),
        #[leaf(KaruString)]
        pub path: String,
        #[leaf(";")]
        _semi: (),
    }

    // ── Schema constructs ────────────────────────────────────────────────

    /// `use schema;`
    #[derive(Debug, Rule)]
    pub struct UseSchema {
        #[leaf("use")]
        _use_kw: (),
        #[leaf("schema")]
        _schema_kw: (),
        #[leaf(";")]
        _semi: (),
    }

    /// `mod Name { ... };` or `mod { ... };`
    #[derive(Debug, Rule)]
    pub struct ModDef {
        #[leaf("mod")]
        _mod_kw: (),
        #[leaf(KaruIdent)]
        pub name: Option<String>,
        #[leaf("{")]
        _lbrace: (),
        pub items: Vec<ModItem>,
        #[leaf("}")]
        _rbrace: (),
        #[leaf(";")]
        _semi: (),
    }

    /// An item inside a mod block.
    #[derive(Debug, Rule)]
    pub enum ModItem {
        Entity(EntityDef),
        Abstract(AbstractDef),
        Action(ActionDef),
    }

    /// `actor Name { ... };` or `resource Name in Parent is Trait { ... };`
    #[derive(Debug, Rule)]
    pub struct EntityDef {
        pub kind: EntityKind,
        #[leaf(KaruIdent)]
        pub name: String,
        pub in_clause: Option<InClause>,
        pub is_clause: Option<IsClause>,
        #[leaf("{")]
        _lbrace: (),
        pub fields: Vec<SchemaField>,
        #[leaf("}")]
        _rbrace: (),
        #[leaf(";")]
        _semi: (),
    }

    /// Entity kind: actor or resource.
    #[derive(Debug, Rule)]
    pub enum EntityKind {
        Actor(#[leaf("actor")] ()),
        Resource(#[leaf("resource")] ()),
    }

    /// `in Parent` clause on resource/entity.
    #[derive(Debug, Rule)]
    pub struct InClause {
        #[leaf("in")]
        _in_kw: (),
        #[leaf(KaruIdent)]
        pub parent: String,
    }

    /// `is Trait` clause on entity.
    #[derive(Debug, Rule)]
    pub struct IsClause {
        #[leaf("is")]
        _is_kw: (),
        #[leaf(KaruIdent)]
        pub trait_name: String,
    }

    /// `abstract Name { ... };`
    #[derive(Debug, Rule)]
    pub struct AbstractDef {
        #[leaf("abstract")]
        _abstract_kw: (),
        #[leaf(KaruIdent)]
        pub name: String,
        #[leaf("{")]
        _lbrace: (),
        pub fields: Vec<SchemaField>,
        #[leaf("}")]
        _rbrace: (),
        #[leaf(";")]
        _semi: (),
    }

    /// `action "Name" appliesTo { ... };`
    #[derive(Debug, Rule)]
    pub struct ActionDef {
        #[leaf("action")]
        _action_kw: (),
        #[leaf(KaruString)]
        pub name: String,
        #[leaf("appliesTo")]
        _appliesto_kw: (),
        #[leaf("{")]
        _lbrace: (),
        pub clauses: Vec<ActionClause>,
        #[leaf("}")]
        _rbrace: (),
        #[leaf(";")]
        _semi: (),
    }

    /// A clause inside an action appliesTo block.
    #[derive(Debug, Rule)]
    pub enum ActionClause {
        /// `actor Type | Type,` or `resource Type,`
        TypesClause(ActionTypesClause),
        /// `context { field Type, ... }`
        ContextClause(ActionContextClause),
    }

    /// A type-based clause: `actor User | ServiceAccount,`
    #[derive(Debug, Rule)]
    pub struct ActionTypesClause {
        #[leaf(KaruIdent)]
        pub kind: String,
        pub types: Vec<ActionTypeRef>,
        #[leaf(",")]
        _comma: (),
    }

    /// A context clause: `context { reason? string, }`
    #[derive(Debug, Rule)]
    pub struct ActionContextClause {
        #[leaf("context")]
        _context_kw: (),
        #[leaf("{")]
        _lbrace: (),
        pub fields: Vec<SchemaField>,
        #[leaf("}")]
        _rbrace: (),
    }

    /// A type reference in action clauses, possibly with `|` unions.
    #[derive(Debug, Rule)]
    pub enum ActionTypeRef {
        Named(#[leaf(KaruIdent)] String),
        Pipe(#[leaf("|")] (), #[leaf(KaruIdent)] String),
    }

    /// A field in a schema entity/abstract: `name Type,` or `name? Type,`
    #[derive(Debug, Rule)]
    pub struct SchemaField {
        #[leaf(KaruIdent)]
        pub name: String,
        #[leaf(optional("?"))]
        pub optional: Option<()>,
        pub ty: SchemaTypeRef,
        #[leaf(",")]
        _comma: (),
    }

    /// A type reference in schema: named, Set<T>, or record { ... }.
    #[derive(Debug, Rule)]
    pub enum SchemaTypeRef {
        Named(#[leaf(KaruIdent)] String),
        SetOf(
            #[leaf("Set")] (),
            #[leaf("<")] (),
            Box<SchemaTypeRef>,
            #[leaf(">")] (),
        ),
    }

    // ── Assert definition ────────────────────────────────────────────────

    /// `assert name<T, U> if expr;`
    #[derive(Debug, Rule)]
    pub struct AssertDef {
        #[leaf("assert")]
        _assert_kw: (),
        #[leaf(KaruIdent)]
        pub name: String,
        pub type_params: Option<AssertTypeParams>,
        pub body: Option<RuleBody>,
        #[leaf(";")]
        _semi: (),
    }

    /// Type parameters on an assert: `<User, Document>`
    #[derive(Debug, Rule)]
    pub struct AssertTypeParams {
        #[leaf("<")]
        _lt: (),
        pub first: AssertTypeParam,
        pub rest: Vec<AssertTypeParamTail>,
        #[leaf(">")]
        _gt: (),
    }

    /// A single type param name.
    #[derive(Debug, Rule)]
    pub struct AssertTypeParam {
        #[leaf(KaruIdent)]
        pub name: String,
    }

    /// A trailing `, TypeName` in assert type params.
    #[derive(Debug, Rule)]
    pub struct AssertTypeParamTail {
        #[leaf(",")]
        _comma: (),
        #[leaf(KaruIdent)]
        pub name: String,
    }

    // ── Core language ────────────────────────────────────────────────────

    /// An inline test block: `test "name" { ... }`
    #[derive(Debug, Rule)]
    pub struct TestBlock {
        #[leaf("test")]
        _test_kw: (),
        #[leaf(KaruString)]
        pub name: String,
        #[leaf("{")]
        _lbrace: (),
        pub items: Vec<TestItem>,
        #[leaf("}")]
        _rbrace: (),
    }

    /// An item inside a test block: entity or expect clause.
    #[derive(Debug, Rule)]
    pub enum TestItem {
        Entity(TestEntity),
        ExpectSimple(ExpectSimple),
        ExpectBlock(ExpectBlock),
    }

    /// A test entity block: `principal { id: "alice", type: "user", }`
    #[derive(Debug, Rule)]
    pub struct TestEntity {
        #[leaf(KaruIdent)]
        pub kind: String,
        #[leaf("{")]
        _lbrace: (),
        pub fields: Vec<TestField>,
        #[leaf("}")]
        _rbrace: (),
    }

    /// A field in a test entity: `key: value,`
    #[derive(Debug, Rule)]
    pub struct TestField {
        #[leaf(KaruIdent)]
        pub key: String,
        #[leaf(":")]
        _colon: (),
        pub value: TestValue,
        #[leaf(",")]
        _comma: (),
    }

    /// A value in a test entity field.
    #[derive(Debug, Rule)]
    pub enum TestValue {
        StringLit(#[leaf(KaruString)] String),
        NumberLit(#[leaf(KaruNumber)] f64),
        True(#[leaf("true")] ()),
        False(#[leaf("false")] ()),
    }

    /// Simple expect clause: `expect allow` or `expect deny`
    #[derive(Debug, Rule)]
    pub struct ExpectSimple {
        #[leaf("expect")]
        _expect_kw: (),
        pub effect: Effect,
    }

    /// Block expect clause: `expect { allow viewRule, deny deleteRule, }`
    #[derive(Debug, Rule)]
    pub struct ExpectBlock {
        #[leaf("expect")]
        _expect_kw: (),
        #[leaf("{")]
        _lbrace: (),
        pub entries: Vec<ExpectEntry>,
        #[leaf("}")]
        _rbrace: (),
    }

    /// A per-rule expect entry: `allow ruleName,`
    #[derive(Debug, Rule)]
    pub struct ExpectEntry {
        pub effect: Effect,
        #[leaf(KaruIdent)]
        pub name: String,
        #[leaf(",")]
        _comma: (),
    }

    /// A rule definition: `allow name if expr;` or `deny name;`
    #[derive(Debug, Rule)]
    pub struct RuleDef {
        pub effect: Effect,
        #[leaf(KaruIdent)]
        pub name: String,
        pub body: Option<RuleBody>,
        #[leaf(";")]
        _semi: (),
    }

    /// The `if expr` portion of a rule.
    #[derive(Debug, Rule)]
    pub struct RuleBody {
        #[leaf("if")]
        _if: (),
        pub expr: Expr,
    }

    /// Allow or deny effect.
    #[derive(Debug, Rule)]
    pub enum Effect {
        Allow(#[leaf("allow")] ()),
        Deny(#[leaf("deny")] ()),
    }

    /// An expression in a rule body.
    #[derive(Debug, Rule)]
    pub enum Expr {
        /// Logical OR: lower precedence
        #[prec_left(1)]
        Or(Box<Expr>, #[leaf("or")] (), Box<Expr>),
        /// Logical AND: higher precedence than OR
        #[prec_left(2)]
        And(Box<Expr>, #[leaf("and")] (), Box<Expr>),
        /// Logical NOT: unary prefix
        Not(#[leaf("not")] (), Box<Expr>),
        /// Parenthesized expression
        Group(#[leaf("(")] (), Box<Expr>, #[leaf(")")] ()),
        /// path op pattern  (comparisons)
        Compare(Path, CompareOp, Pattern),
        /// path in path  (membership check)
        InExpr(Pattern, #[leaf("in")] (), Path),
        /// forall var in path: expr
        Forall(
            #[leaf("forall")] (),
            #[leaf(KaruIdent)] String,
            #[leaf("in")] (),
            Path,
            #[leaf(":")] (),
            Box<Expr>,
        ),
        /// exists var in path: expr
        Exists(
            #[leaf("exists")] (),
            #[leaf(KaruIdent)] String,
            #[leaf("in")] (),
            Path,
            #[leaf(":")] (),
            Box<Expr>,
        ),
        /// Type guard: `actor is User` or `resource is Document`
        IsType(Path, #[leaf("is")] (), #[leaf(KaruIdent)] String),
        /// `path has field` check
        Has(Path, #[leaf("has")] (), #[leaf(KaruIdent)] String),
        /// Bare path/identifier reference used as boolean (e.g. `is_admin`)
        Ref(Path),
    }

    /// Comparison operators.
    #[derive(Debug, Rule)]
    pub enum CompareOp {
        Eq(#[leaf("==")] ()),
        Ne(#[leaf("!=")] ()),
        Le(#[leaf("<=")] ()),
        Ge(#[leaf(">=")] ()),
        Lt(#[leaf("<")] ()),
        Gt(#[leaf(">")] ()),
    }

    /// A path expression like `resource.context.args` or `data[0].name`.
    #[derive(Debug, Rule)]
    pub struct Path {
        #[leaf(KaruIdent)]
        pub head: Spanned<String>,
        pub segments: Vec<PathSegment>,
    }

    /// A segment in a dotted path.
    #[derive(Debug, Rule)]
    pub enum PathSegment {
        /// `.field`
        Field(#[leaf(".")] (), #[leaf(KaruIdent)] String),
        /// `[index]`
        Index(#[leaf("[")] (), #[leaf(KaruDigits)] u32, #[leaf("]")] ()),
        /// `[variable]` - identifier inside brackets
        Variable(#[leaf("[")] (), #[leaf(KaruIdent)] String, #[leaf("]")] ()),
    }

    /// A pattern for matching values.
    #[derive(Debug, Rule)]
    pub enum Pattern {
        /// String literal
        StringLit(#[leaf(KaruString)] String),
        /// Numeric literal
        NumberLit(#[leaf(KaruNumber)] f64),
        /// Boolean true
        True(#[leaf("true")] ()),
        /// Boolean false
        False(#[leaf("false")] ()),
        /// Null
        Null(#[leaf("null")] ()),
        /// Wildcard `_`
        Wildcard(#[leaf("_")] ()),
        /// Object pattern `{ key: pattern, ... }`
        Object(
            #[leaf("{")] (),
            #[sep_by(",")] Vec<ObjectField>,
            #[leaf("}")] (),
        ),
        /// Array pattern `[pattern, ...]`
        Array(
            #[leaf("[")] (),
            #[sep_by(",")] Vec<Pattern>,
            #[leaf("]")] (),
        ),
        /// Path reference (resolves at eval time)
        PathRef(Path),
    }

    /// A field in an object pattern: `key: pattern`
    #[derive(Debug, Rule)]
    pub struct ObjectField {
        #[leaf(KaruIdent)]
        pub key: String,
        #[leaf(":")]
        _colon: (),
        pub value: Pattern,
    }
}

// === Conversion to canonical AST ===

use grammar::*;

/// Strip surrounding double quotes from a string literal token.
fn strip_quotes(s: &str) -> String {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

impl Program {
    /// Get all rules from the program (ignoring test blocks).
    pub fn rules(&self) -> Vec<&RuleDef> {
        self.items
            .iter()
            .filter_map(|item| match item {
                TopLevelItem::Rule(r) => Some(r),
                _ => None,
            })
            .collect()
    }

    /// Convert this tree-sitter AST to the canonical `ast::Program`.
    pub fn to_ast(&self) -> crate::ast::Program {
        crate::ast::Program {
            use_schema: self
                .items
                .iter()
                .any(|i| matches!(i, TopLevelItem::UseSchema(_))),
            imports: self
                .items
                .iter()
                .filter_map(|i| match i {
                    TopLevelItem::Import(imp) => Some(strip_quotes(&imp.path)),
                    _ => None,
                })
                .collect(),
            modules: vec![],
            assertions: vec![],
            rules: self.rules().iter().map(|r| r.to_ast()).collect(),
            tests: vec![],
        }
    }
}

impl RuleDef {
    fn to_ast(&self) -> crate::ast::RuleAst {
        crate::ast::RuleAst {
            name: self.name.clone(),
            effect: self.effect.to_ast(),
            body: self.body.as_ref().map(|b| b.expr.to_ast()),
        }
    }
}

impl Effect {
    fn to_ast(&self) -> crate::ast::EffectAst {
        match self {
            Effect::Allow(_) => crate::ast::EffectAst::Allow,
            Effect::Deny(_) => crate::ast::EffectAst::Deny,
        }
    }
}

impl Expr {
    fn to_ast(&self) -> crate::ast::ExprAst {
        match self {
            Expr::And(l, _, r) => {
                let left = l.to_ast();
                let right = r.to_ast();
                // Flatten nested ANDs
                let mut parts = match left {
                    crate::ast::ExprAst::And(inner) => inner,
                    other => vec![other],
                };
                parts.push(right);
                crate::ast::ExprAst::And(parts)
            }
            Expr::Or(l, _, r) => {
                let left = l.to_ast();
                let right = r.to_ast();
                let mut parts = match left {
                    crate::ast::ExprAst::Or(inner) => inner,
                    other => vec![other],
                };
                parts.push(right);
                crate::ast::ExprAst::Or(parts)
            }
            Expr::Not(_, inner) => crate::ast::ExprAst::Not(Box::new(inner.to_ast())),
            Expr::Group(_, inner, _) => inner.to_ast(),
            Expr::Compare(path, op, pattern) => crate::ast::ExprAst::Compare {
                left: path.to_ast(),
                op: op.to_ast(),
                right: pattern.to_ast(),
            },
            Expr::InExpr(needle, _, haystack) => crate::ast::ExprAst::In {
                pattern: needle.to_ast(),
                path: haystack.to_ast(),
            },
            Expr::Forall(_, var, _, path, _, body) => crate::ast::ExprAst::Forall {
                var: var.clone(),
                path: path.to_ast(),
                body: Box::new(body.to_ast()),
            },
            Expr::Exists(_, var, _, path, _, body) => crate::ast::ExprAst::Exists {
                var: var.clone(),
                path: path.to_ast(),
                body: Box::new(body.to_ast()),
            },
            Expr::IsType(path, _, type_name) => crate::ast::ExprAst::IsType {
                path: path.to_ast(),
                type_name: type_name.clone(),
            },
            Expr::Has(path, _, _field_name) => crate::ast::ExprAst::Has {
                path: path.to_ast(),
            },
            Expr::Ref(path) => {
                // A bare identifier like `is_admin` is a type/assertion reference
                let p = path.to_ast();
                let name = p
                    .segments
                    .iter()
                    .map(|s| match s {
                        crate::ast::PathSegmentAst::Field(f) => f.as_str(),
                        _ => "",
                    })
                    .collect::<Vec<_>>()
                    .join(".");
                crate::ast::ExprAst::TypeRef {
                    namespace: None,
                    name,
                }
            }
        }
    }
}

impl CompareOp {
    fn to_ast(&self) -> crate::ast::OpAst {
        match self {
            CompareOp::Eq(_) => crate::ast::OpAst::Eq,
            CompareOp::Ne(_) => crate::ast::OpAst::Ne,
            CompareOp::Lt(_) => crate::ast::OpAst::Lt,
            CompareOp::Gt(_) => crate::ast::OpAst::Gt,
            CompareOp::Le(_) => crate::ast::OpAst::Le,
            CompareOp::Ge(_) => crate::ast::OpAst::Ge,
        }
    }
}

impl Path {
    fn to_ast(&self) -> crate::ast::PathAst {
        let mut segments = vec![crate::ast::PathSegmentAst::Field(self.head.value.clone())];
        for seg in &self.segments {
            segments.push(seg.to_ast());
        }
        crate::ast::PathAst { segments }
    }
}

impl PathSegment {
    fn to_ast(&self) -> crate::ast::PathSegmentAst {
        match self {
            PathSegment::Field(_, name) => crate::ast::PathSegmentAst::Field(name.clone()),
            PathSegment::Index(_, idx, _) => crate::ast::PathSegmentAst::Index(*idx as usize),
            PathSegment::Variable(_, name, _) => crate::ast::PathSegmentAst::Variable(name.clone()),
        }
    }
}

impl Pattern {
    fn to_ast(&self) -> crate::ast::PatternAst {
        match self {
            Pattern::StringLit(s) => {
                crate::ast::PatternAst::Literal(serde_json::Value::String(strip_quotes(s)))
            }
            Pattern::NumberLit(n) => crate::ast::PatternAst::Literal(serde_json::json!(*n)),
            Pattern::True(_) => crate::ast::PatternAst::Literal(serde_json::Value::Bool(true)),
            Pattern::False(_) => crate::ast::PatternAst::Literal(serde_json::Value::Bool(false)),
            Pattern::Null(_) => crate::ast::PatternAst::Literal(serde_json::Value::Null),
            Pattern::Wildcard(_) => crate::ast::PatternAst::Wildcard,
            Pattern::Object(_, fields, _) => crate::ast::PatternAst::Object(
                fields
                    .iter()
                    .map(|f| (f.key.clone(), f.value.to_ast()))
                    .collect(),
            ),
            Pattern::Array(_, elems, _) => {
                crate::ast::PatternAst::Array(elems.iter().map(|e| e.to_ast()).collect())
            }
            Pattern::PathRef(path) => crate::ast::PatternAst::PathRef(path.to_ast()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::grammar;
    use krust_sitter::Language;

    #[test]
    fn test_parse_simple_allow() {
        let result = grammar::Program::parse("allow access;");
        let prog = result.result.expect("should parse");
        let rules = prog.rules();
        assert_eq!(rules.len(), 1);
        assert!(matches!(rules[0].effect, grammar::Effect::Allow(_)));
        assert_eq!(rules[0].name, "access");
        assert!(rules[0].body.is_none());
    }

    #[test]
    fn test_parse_with_condition() {
        let result = grammar::Program::parse(r#"allow read if action == "read";"#);
        let prog = result.result.expect("should parse");
        assert_eq!(prog.rules().len(), 1);
        let ast = prog.to_ast();
        assert_eq!(ast.rules[0].name, "read");
    }

    #[test]
    fn test_parse_deny() {
        let result = grammar::Program::parse("deny blocked;");
        let prog = result.result.expect("should parse");
        assert!(matches!(prog.rules()[0].effect, grammar::Effect::Deny(_)));
    }

    #[test]
    fn test_parse_and_expression() {
        let result = grammar::Program::parse(r#"allow x if a == "1" and b == "2";"#);
        let prog = result.result.expect("should parse");
        let ast = prog.to_ast();
        assert!(matches!(
            ast.rules[0].body,
            Some(crate::ast::ExprAst::And(_))
        ));
    }

    #[test]
    fn test_parse_or_expression() {
        let result = grammar::Program::parse(r#"allow x if a == "1" or b == "2";"#);
        let prog = result.result.expect("should parse");
        let ast = prog.to_ast();
        assert!(matches!(
            ast.rules[0].body,
            Some(crate::ast::ExprAst::Or(_))
        ));
    }

    #[test]
    fn test_parse_path_in_path() {
        let result = grammar::Program::parse("allow admin if principal.id in resource.adminIds;");
        let prog = result.result.expect("should parse");
        let ast = prog.to_ast();
        assert!(matches!(
            ast.rules[0].body,
            Some(crate::ast::ExprAst::In { .. })
        ));
    }

    #[test]
    fn test_parse_multiple_rules() {
        let result = grammar::Program::parse(
            r#"
            allow read if action == "read";
            deny delete if action == "delete";
        "#,
        );
        let prog = result.result.expect("should parse");
        assert_eq!(prog.rules().len(), 2);
    }

    #[test]
    fn test_parse_use_schema() {
        let result = grammar::Program::parse("use schema;");
        let prog = result.result.expect("should parse use schema");
        assert!(prog
            .items
            .iter()
            .any(|i| matches!(i, grammar::TopLevelItem::UseSchema(_))));
    }

    #[test]
    fn test_parse_mod_with_entity() {
        let result = grammar::Program::parse(
            r#"
            use schema;
            mod {
                actor User {
                    name string,
                };
            };
        "#,
        );
        let prog = result.result.expect("should parse mod with entity");
        assert!(prog
            .items
            .iter()
            .any(|i| matches!(i, grammar::TopLevelItem::Mod(_))));
    }

    #[test]
    fn test_parse_assert_with_type_params() {
        let result = grammar::Program::parse(
            r#"
            assert is_admin<User> if "admin" in actor.roles;
        "#,
        );
        let prog = result.result.expect("should parse assert");
        assert!(prog
            .items
            .iter()
            .any(|i| matches!(i, grammar::TopLevelItem::Assert(_))));
    }

    #[test]
    fn test_parse_is_type_guard() {
        let result = grammar::Program::parse(
            r#"
            allow view if actor is User and actor.name == "alice";
        "#,
        );
        let prog = result.result.expect("should parse is type guard");
        let ast = prog.to_ast();
        // Should parse as And(IsType, Compare)
        assert!(matches!(
            ast.rules[0].body,
            Some(crate::ast::ExprAst::And(_))
        ));
    }

    #[test]
    fn test_rules_method_filtering() {
        let code = r#"
            use schema;
            assert is_admin<User> if "admin" in actor.roles;
            allow read if action == "read";
            deny delete if action == "delete";
        "#;
        let result = grammar::Program::parse(code);
        let prog = result.result.expect("should parse");

        // Assert that we parsed exactly 4 top-level items
        assert_eq!(prog.items.len(), 4);

        // Assert that the rules() method correctly filtered out the UseSchema and Assert items
        let rules = prog.rules();
        assert_eq!(rules.len(), 2);

        // Assert that the filtered rules match what we expect
        assert_eq!(rules[0].name, "read");
        assert_eq!(rules[1].name, "delete");
    }
}
