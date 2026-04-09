// SPDX-License-Identifier: MIT

//! Tree-sitter grammar for Cedar policy language (dev-time parser).
//!
//! Provides error-tolerant parsing with span information for LSP tooling.
//! This complements the hand-rolled `cedar_parser.rs` runtime parser.
//!
//! The grammar produces its own AST types which can be converted to the
//! canonical `cedar_parser::*` types.

#[allow(clippy::manual_non_exhaustive)]
pub mod grammar {
    use krust_sitter::Rule;

    // ── Token types ──────────────────────────────────────────────────────

    /// Identifier token
    #[derive(Debug, Clone, PartialEq, Eq, Rule)]
    #[leaf(pattern(r"[a-zA-Z_][a-zA-Z0-9_]*"))]
    pub struct CedarIdent;

    /// String literal token (double-quoted)
    #[derive(Debug, Clone, PartialEq, Eq, Rule)]
    #[leaf(pattern(r#""([^"\\]|\\.)*""#))]
    pub struct CedarString;

    /// Integer literal token
    #[derive(Debug, Clone, PartialEq, Eq, Rule)]
    #[leaf(pattern(r"-?[0-9]+"))]
    pub struct CedarInt;

    /// Entity type path token: `Type` or `Namespace::Type`
    #[derive(Debug, Clone, PartialEq, Eq, Rule)]
    #[leaf(pattern(r"[a-zA-Z_][a-zA-Z0-9_]*(::[a-zA-Z_][a-zA-Z0-9_]*)*"))]
    pub struct CedarPath;

    /// Record key: identifier or string
    #[derive(Debug, Clone, PartialEq, Eq, Rule)]
    #[leaf(pattern(r#"[a-zA-Z_][a-zA-Z0-9_]*|"([^"\\]|\\.)*""#))]
    pub struct CedarRecordKey;

    // ========================================================================
    // Root
    // ========================================================================

    /// A Cedar file is a list of policies.
    #[derive(Debug, Rule)]
    #[language]
    #[extras(re(r"\s"), re(r"//[^\n]*"))]
    #[word(CedarIdent)]
    pub struct PolicySet {
        pub policies: Vec<Policy>,
    }

    // ========================================================================
    // Policy
    // ========================================================================

    /// A single Cedar policy: `@annot("v") permit(principal, action, resource) when { ... };`
    #[derive(Debug, Rule)]
    pub struct Policy {
        pub annotations: Vec<Annotation>,
        pub effect: Effect,
        #[leaf("(")]
        _lp: (),
        pub scope: Scope,
        #[leaf(")")]
        _rp: (),
        pub conditions: Vec<Condition>,
        #[leaf(";")]
        _semi: (),
    }

    /// `@key("value")` annotation.
    #[derive(Debug, Rule)]
    pub struct Annotation {
        #[leaf("@")]
        _at: (),
        #[leaf(CedarIdent)]
        pub key: String,
        pub value: Option<AnnotationValue>,
    }

    #[derive(Debug, Rule)]
    pub struct AnnotationValue {
        #[leaf("(")]
        _lp: (),
        #[leaf(CedarString)]
        pub value: String,
        #[leaf(")")]
        _rp: (),
    }

    /// permit | forbid
    #[derive(Debug, Rule)]
    pub enum Effect {
        Permit(#[leaf("permit")] ()),
        Forbid(#[leaf("forbid")] ()),
    }

    // ========================================================================
    // Scope
    // ========================================================================

    /// `principal [constraint], action [constraint], resource [constraint]`
    #[derive(Debug, Rule)]
    pub struct Scope {
        pub principal: PrincipalScope,
        #[leaf(",")]
        _c1: (),
        pub action: ActionScope,
        #[leaf(",")]
        _c2: (),
        pub resource: ResourceScope,
    }

    /// `principal` with optional constraint.
    #[derive(Debug, Rule)]
    pub struct PrincipalScope {
        #[leaf("principal")]
        _kw: (),
        pub constraint: Option<ScopeConstraint>,
    }

    /// `action` with optional constraint.
    #[derive(Debug, Rule)]
    pub struct ActionScope {
        #[leaf("action")]
        _kw: (),
        pub constraint: Option<ActionConstraint>,
    }

    /// `resource` with optional constraint.
    #[derive(Debug, Rule)]
    pub struct ResourceScope {
        #[leaf("resource")]
        _kw: (),
        pub constraint: Option<ScopeConstraint>,
    }

    /// `== Entity` or `in Entity` for principal/resource.
    #[derive(Debug, Rule)]
    pub enum ScopeConstraint {
        Eq(#[leaf("==")] (), EntityRef),
        In(#[leaf("in")] (), EntityRef),
    }

    /// `== Entity` or `in Entity` or `in [Entity, ...]` for action.
    #[derive(Debug, Rule)]
    pub enum ActionConstraint {
        Eq(#[leaf("==")] (), EntityRef),
        In(#[leaf("in")] (), EntityRef),
        InList(
            #[leaf("in")] (),
            #[leaf("[")] (),
            #[sep_by(",")] Vec<EntityRef>,
            #[leaf("]")] (),
        ),
    }

    /// Entity reference: `Type::"id"` or `Namespace::Type::"id"`
    #[derive(Debug, Rule)]
    pub struct EntityRef {
        /// Full type path including namespace: e.g. `Namespace::Type` or just `Type`
        #[leaf(CedarPath)]
        pub path: String,
        #[leaf("::")]
        _sep: (),
        #[leaf(CedarString)]
        pub id: String,
    }

    // ========================================================================
    // Conditions
    // ========================================================================

    /// `when { expr }` or `unless { expr }`
    #[derive(Debug, Rule)]
    pub enum Condition {
        When(#[leaf("when")] (), #[leaf("{")] (), Expr, #[leaf("}")] ()),
        Unless(#[leaf("unless")] (), #[leaf("{")] (), Expr, #[leaf("}")] ()),
    }

    // ========================================================================
    // Expressions (precedence climbing)
    // ========================================================================

    /// Cedar expression with operator precedence.
    #[derive(Debug, Rule)]
    pub enum Expr {
        /// `a || b`
        #[prec_left(1)]
        Or(Box<Expr>, #[leaf("||")] (), Box<Expr>),

        /// `a && b`
        #[prec_left(2)]
        And(Box<Expr>, #[leaf("&&")] (), Box<Expr>),

        /// `a == b`, `a != b`, `a < b`, etc.
        #[prec_left(3)]
        Relation(Box<Expr>, RelOp, Box<Expr>),

        /// `a in b`
        #[prec_left(3)]
        InExpr(Box<Expr>, #[leaf("in")] (), Box<Expr>),

        /// `a has field`
        #[prec_left(3)]
        Has(Box<Expr>, #[leaf("has")] (), #[leaf(CedarIdent)] String),

        /// `a has "field"`
        #[prec_left(3)]
        HasStr(Box<Expr>, #[leaf("has")] (), #[leaf(CedarString)] String),

        /// `a like "pattern"`
        #[prec_left(3)]
        Like(Box<Expr>, #[leaf("like")] (), #[leaf(CedarString)] String),

        /// `a + b`
        #[prec_left(4)]
        Add(Box<Expr>, #[leaf("+")] (), Box<Expr>),

        /// `a - b`
        #[prec_left(4)]
        Sub(Box<Expr>, #[leaf("-")] (), Box<Expr>),

        /// `a * b`
        #[prec_left(5)]
        Mul(Box<Expr>, #[leaf("*")] (), Box<Expr>),

        /// `!expr`
        #[prec(6)]
        Not(#[leaf("!")] (), Box<Expr>),

        /// `-expr`
        #[prec(6)]
        Neg(#[leaf("-")] (), Box<Expr>),

        /// `expr.field` or `expr.method(args)`
        #[prec_left(7)]
        MemberAccess(
            Box<Expr>,
            #[leaf(".")] (),
            #[leaf(CedarIdent)] String,
            Option<CallArgs>,
        ),

        /// `expr["key"]`
        #[prec_left(7)]
        IndexAccess(
            Box<Expr>,
            #[leaf("[")] (),
            #[leaf(CedarString)] String,
            #[leaf("]")] (),
        ),

        /// `if cond then a else b`
        IfThenElse(
            #[leaf("if")] (),
            Box<Expr>,
            #[leaf("then")] (),
            Box<Expr>,
            #[leaf("else")] (),
            Box<Expr>,
        ),

        /// `(expr)`
        Group(#[leaf("(")] (), Box<Expr>, #[leaf(")")] ()),

        /// Variables: `principal`, `action`, `resource`, `context`
        Principal(#[leaf("principal")] ()),
        Action(#[leaf("action")] ()),
        Resource(#[leaf("resource")] ()),
        Context(#[leaf("context")] ()),

        /// Boolean: `true` / `false`
        True(#[leaf("true")] ()),
        False(#[leaf("false")] ()),

        /// Integer literal
        IntLit(#[leaf(CedarInt)] i64),

        /// String literal
        StrLit(#[leaf(CedarString)] String),

        /// Identifier (catches function names, entity types, etc.)
        Ident(#[leaf(CedarIdent)] String),

        /// Set literal: `[expr, ...]`
        SetLit(#[leaf("[")] (), #[sep_by(",")] Vec<Expr>, #[leaf("]")] ()),

        /// Record literal: `{ key: expr, ... }`
        RecordLit(
            #[leaf("{")] (),
            #[sep_by(",")] Vec<RecordField>,
            #[leaf("}")] (),
        ),

        /// Function call: `func(args)` or `Namespace::func(args)`
        FuncCall(
            #[leaf(CedarIdent)] String,
            #[leaf("(")] (),
            #[sep_by(",")] Vec<Expr>,
            #[leaf(")")] (),
        ),
    }

    /// Relational operators.
    #[derive(Debug, Rule)]
    pub enum RelOp {
        Eq(#[leaf("==")] ()),
        Neq(#[leaf("!=")] ()),
        Lt(#[leaf("<")] ()),
        Lte(#[leaf("<=")] ()),
        Gt(#[leaf(">")] ()),
        Gte(#[leaf(">=")] ()),
    }

    /// Call arguments: `(expr, ...)`
    #[derive(Debug, Rule)]
    pub struct CallArgs {
        #[leaf("(")]
        _lp: (),
        #[sep_by(",")]
        pub args: Vec<Expr>,
        #[leaf(")")]
        _rp: (),
    }

    /// Record field: `key: expr`
    #[derive(Debug, Rule)]
    pub struct RecordField {
        #[leaf(CedarRecordKey)]
        pub key: String,
        #[leaf(":")]
        _colon: (),
        pub value: Expr,
    }
}

// ============================================================================
// Conversion to canonical Cedar AST types
// ============================================================================

use grammar::*;

/// Strip surrounding double quotes from a string literal token.
fn strip_quotes(s: &str) -> String {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

impl PolicySet {
    /// Convert to the canonical `CedarPolicySet` from `cedar_parser`.
    pub fn to_ast(&self) -> crate::cedar_parser::CedarPolicySet {
        crate::cedar_parser::CedarPolicySet {
            policies: self.policies.iter().map(|p| p.to_ast()).collect(),
        }
    }
}

impl Policy {
    fn to_ast(&self) -> crate::cedar_parser::CedarPolicy {
        crate::cedar_parser::CedarPolicy {
            annotations: self.annotations.iter().map(|a| a.to_ast()).collect(),
            effect: self.effect.to_ast(),
            scope: self.scope.to_ast(),
            conditions: self.conditions.iter().map(|c| c.to_ast()).collect(),
        }
    }
}

impl Annotation {
    fn to_ast(&self) -> crate::cedar_parser::CedarAnnotation {
        crate::cedar_parser::CedarAnnotation {
            key: self.key.clone(),
            value: self.value.as_ref().map(|v| strip_quotes(&v.value)),
        }
    }
}

impl Effect {
    fn to_ast(&self) -> crate::cedar_parser::CedarEffect {
        match self {
            Effect::Permit(_) => crate::cedar_parser::CedarEffect::Permit,
            Effect::Forbid(_) => crate::cedar_parser::CedarEffect::Forbid,
        }
    }
}

impl Scope {
    fn to_ast(&self) -> crate::cedar_parser::CedarScope {
        crate::cedar_parser::CedarScope {
            principal: self.principal.to_ast(),
            action: self.action.to_ast(),
            resource: self.resource.to_ast(),
        }
    }
}

impl PrincipalScope {
    fn to_ast(&self) -> crate::cedar_parser::CedarScopeConstraint {
        match &self.constraint {
            None => crate::cedar_parser::CedarScopeConstraint::Any,
            Some(c) => c.to_ast(),
        }
    }
}

impl ResourceScope {
    fn to_ast(&self) -> crate::cedar_parser::CedarScopeConstraint {
        match &self.constraint {
            None => crate::cedar_parser::CedarScopeConstraint::Any,
            Some(c) => c.to_ast(),
        }
    }
}

impl ScopeConstraint {
    fn to_ast(&self) -> crate::cedar_parser::CedarScopeConstraint {
        match self {
            ScopeConstraint::Eq(_, e) => {
                crate::cedar_parser::CedarScopeConstraint::Eq(e.to_entity_ref())
            }
            ScopeConstraint::In(_, e) => {
                crate::cedar_parser::CedarScopeConstraint::In(e.to_entity_ref())
            }
        }
    }
}

impl ActionScope {
    fn to_ast(&self) -> crate::cedar_parser::CedarActionConstraint {
        match &self.constraint {
            None => crate::cedar_parser::CedarActionConstraint::Any,
            Some(c) => c.to_ast(),
        }
    }
}

impl ActionConstraint {
    fn to_ast(&self) -> crate::cedar_parser::CedarActionConstraint {
        match self {
            ActionConstraint::Eq(_, e) => {
                crate::cedar_parser::CedarActionConstraint::Eq(e.to_entity_ref())
            }
            ActionConstraint::In(_, e) => {
                crate::cedar_parser::CedarActionConstraint::In(e.to_entity_ref())
            }
            ActionConstraint::InList(_, _, entities, _) => {
                crate::cedar_parser::CedarActionConstraint::InList(
                    entities.iter().map(|e| e.to_entity_ref()).collect(),
                )
            }
        }
    }
}

impl EntityRef {
    fn to_entity_ref(&self) -> crate::cedar_parser::CedarEntityRef {
        let path: Vec<String> = self.path.split("::").map(|s| s.to_string()).collect();
        crate::cedar_parser::CedarEntityRef {
            path,
            id: strip_quotes(&self.id),
        }
    }
}

impl Condition {
    fn to_ast(&self) -> crate::cedar_parser::CedarCondition {
        match self {
            Condition::When(_, _, expr, _) => crate::cedar_parser::CedarCondition {
                is_when: true,
                expr: expr.to_ast(),
            },
            Condition::Unless(_, _, expr, _) => crate::cedar_parser::CedarCondition {
                is_when: false,
                expr: expr.to_ast(),
            },
        }
    }
}

impl Expr {
    fn to_ast(&self) -> crate::cedar_parser::CedarExpr {
        use crate::cedar_parser::CedarExpr as CE;
        match self {
            Expr::Or(l, _, r) => CE::Or(Box::new(l.to_ast()), Box::new(r.to_ast())),
            Expr::And(l, _, r) => CE::And(Box::new(l.to_ast()), Box::new(r.to_ast())),
            Expr::Relation(l, op, r) => CE::Relation {
                lhs: Box::new(l.to_ast()),
                op: op.to_ast(),
                rhs: Box::new(r.to_ast()),
            },
            Expr::InExpr(l, _, r) => CE::InExpr {
                lhs: Box::new(l.to_ast()),
                rhs: Box::new(r.to_ast()),
            },
            Expr::Has(e, _, field) => CE::Has(Box::new(e.to_ast()), field.clone()),
            Expr::HasStr(e, _, field) => CE::Has(Box::new(e.to_ast()), strip_quotes(field)),
            Expr::Like(e, _, pattern) => CE::Like(Box::new(e.to_ast()), strip_quotes(pattern)),
            Expr::Add(l, _, r) => CE::Add(
                Box::new(l.to_ast()),
                crate::cedar_parser::CedarAddOp::Add,
                Box::new(r.to_ast()),
            ),
            Expr::Sub(l, _, r) => CE::Add(
                Box::new(l.to_ast()),
                crate::cedar_parser::CedarAddOp::Sub,
                Box::new(r.to_ast()),
            ),
            Expr::Mul(l, _, r) => CE::Mul(Box::new(l.to_ast()), Box::new(r.to_ast())),
            Expr::Not(_, e) => CE::Not(Box::new(e.to_ast())),
            Expr::Neg(_, e) => CE::Neg(Box::new(e.to_ast())),
            Expr::MemberAccess(e, _, field, call_args) => match call_args {
                None => CE::Access(Box::new(e.to_ast()), field.clone()),
                Some(args) => CE::MethodCall(
                    Box::new(e.to_ast()),
                    field.clone(),
                    args.args.iter().map(|a| a.to_ast()).collect(),
                ),
            },
            Expr::IndexAccess(e, _, key, _) => CE::Index(Box::new(e.to_ast()), strip_quotes(key)),
            Expr::IfThenElse(_, cond, _, then_e, _, else_e) => CE::IfThenElse {
                cond: Box::new(cond.to_ast()),
                then_expr: Box::new(then_e.to_ast()),
                else_expr: Box::new(else_e.to_ast()),
            },
            Expr::Group(_, e, _) => e.to_ast(),
            Expr::Principal(_) => CE::Var("principal".to_string()),
            Expr::Action(_) => CE::Var("action".to_string()),
            Expr::Resource(_) => CE::Var("resource".to_string()),
            Expr::Context(_) => CE::Var("context".to_string()),
            Expr::True(_) => CE::Bool(true),
            Expr::False(_) => CE::Bool(false),
            Expr::IntLit(n) => CE::Int(*n),
            Expr::StrLit(s) => CE::Str(strip_quotes(s)),
            Expr::Ident(name) => CE::Var(name.clone()),
            Expr::SetLit(_, elems, _) => CE::Set(elems.iter().map(|e| e.to_ast()).collect()),
            Expr::RecordLit(_, fields, _) => CE::Record(
                fields
                    .iter()
                    .map(|f| (strip_quotes(&f.key), f.value.to_ast()))
                    .collect(),
            ),
            Expr::FuncCall(name, _, args, _) => {
                CE::ExtFun(name.clone(), args.iter().map(|a| a.to_ast()).collect())
            }
        }
    }
}

impl RelOp {
    fn to_ast(&self) -> crate::cedar_parser::CedarRelOp {
        match self {
            RelOp::Eq(_) => crate::cedar_parser::CedarRelOp::Eq,
            RelOp::Neq(_) => crate::cedar_parser::CedarRelOp::Neq,
            RelOp::Lt(_) => crate::cedar_parser::CedarRelOp::Lt,
            RelOp::Lte(_) => crate::cedar_parser::CedarRelOp::Lte,
            RelOp::Gt(_) => crate::cedar_parser::CedarRelOp::Gt,
            RelOp::Gte(_) => crate::cedar_parser::CedarRelOp::Gte,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::grammar;
    use krust_sitter::Language;

    #[test]
    fn test_parse_simple_permit() {
        let result = grammar::PolicySet::parse(r#"permit(principal, action, resource);"#);
        let ps = result.result.expect("should parse");
        assert_eq!(ps.policies.len(), 1);
        assert!(matches!(ps.policies[0].effect, grammar::Effect::Permit(_)));
    }

    #[test]
    fn test_parse_forbid() {
        let result = grammar::PolicySet::parse(r#"forbid(principal, action, resource);"#);
        let ps = result.result.expect("should parse");
        assert!(matches!(ps.policies[0].effect, grammar::Effect::Forbid(_)));
    }

    #[test]
    fn test_parse_with_when() {
        let result = grammar::PolicySet::parse(
            r#"permit(principal, action, resource) when { principal.role == "admin" };"#,
        );
        let ps = result.result.expect("should parse");
        assert_eq!(ps.policies[0].conditions.len(), 1);
    }

    #[test]
    fn test_parse_multiple_policies() {
        let result = grammar::PolicySet::parse(
            r#"
            permit(principal, action, resource) when { true };
            forbid(principal, action, resource) when { false };
            "#,
        );
        let ps = result.result.expect("should parse");
        assert_eq!(ps.policies.len(), 2);
    }

    #[test]
    fn test_parse_annotated_policy() {
        let result =
            grammar::PolicySet::parse(r#"@id("policy1") permit(principal, action, resource);"#);
        let ps = result.result.expect("should parse");
        assert_eq!(ps.policies[0].annotations.len(), 1);
        assert_eq!(ps.policies[0].annotations[0].key, "id");
    }

    #[test]
    fn test_roundtrip_to_ast() {
        let result = grammar::PolicySet::parse(
            r#"permit(principal, action, resource) when { context.authenticated == true };"#,
        );
        let ps = result.result.expect("should parse");
        let ast = ps.to_ast();
        assert_eq!(ast.policies.len(), 1);
        assert_eq!(
            ast.policies[0].effect,
            crate::cedar_parser::CedarEffect::Permit
        );
    }

    #[test]
    fn test_parse_with_comments() {
        // Line comments at start, inline, and between policies
        let result = grammar::PolicySet::parse(
            r#"
            // This is a leading comment
            permit(principal, action, resource); // inline comment
            // Comment between policies
            forbid(principal, action, resource);
            "#,
        );
        let ps = result.result.expect("should parse with // comments");
        assert_eq!(ps.policies.len(), 2);
        assert!(matches!(ps.policies[0].effect, grammar::Effect::Permit(_)));
        assert!(matches!(ps.policies[1].effect, grammar::Effect::Forbid(_)));
        // NOTE: result.errors may contain spurious word/extras errors from
        // tree-sitter; these are filtered in cedar_ts_parse_diagnostics.
    }
}
