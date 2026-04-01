// SPDX-License-Identifier: MIT

//! Abstract Syntax Tree for Karu policies.

use crate::schema::{AssertDef, ModuleDef};
use serde_json::Value;

/// A complete Karu program (policy source file).
#[derive(Debug, Clone)]
pub struct Program {
    /// Whether `use schema;` is present.
    pub use_schema: bool,
    /// Import paths (e.g. `import "schema.karu";`).
    pub imports: Vec<String>,
    /// Module (namespace) definitions.
    pub modules: Vec<ModuleDef>,
    /// Assertion definitions.
    pub assertions: Vec<AssertDef>,
    /// Rule definitions.
    pub rules: Vec<RuleAst>,
    /// Inline test declarations.
    pub tests: Vec<TestDef>,
}

/// A test case declaration.
#[derive(Debug, Clone)]
pub struct TestDef {
    /// The test name (e.g. "alice can view").
    pub name: String,
    /// Entity descriptions for the test scenario.
    pub entities: Vec<TestEntity>,
    /// Expected policy decision(s).
    pub expected: ExpectedOutcome,
}

/// Expected outcome of a test case.
#[derive(Debug, Clone)]
pub enum ExpectedOutcome {
    /// Simple form: `expect allow` or `expect deny`
    Simple(EffectAst),
    /// Per-rule form: `expect { allow viewRule, deny deleteRule }`
    PerRule(Vec<(EffectAst, String)>),
}

/// An entity in a test case (resource, principal, or action).
#[derive(Debug, Clone)]
pub struct TestEntity {
    /// Entity kind: "resource", "principal", "actor", or "action".
    pub kind: String,
    /// Key-value pairs describing the entity fields.
    pub fields: Vec<(String, serde_json::Value)>,
    /// True when parsed from shorthand form (e.g. `action "view"` → id="view").
    /// In shorthand mode, the single `id` value is used directly as the entity
    /// value instead of wrapping in an object.
    pub shorthand: bool,
}

/// A rule definition.
#[derive(Debug, Clone)]
pub struct RuleAst {
    pub name: String,
    pub effect: EffectAst,
    pub body: Option<ExprAst>,
}

/// Allow or deny effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectAst {
    Allow,
    Deny,
}

/// An expression in a rule body.
#[derive(Debug, Clone)]
pub enum ExprAst {
    /// Logical AND of expressions
    And(Vec<ExprAst>),
    /// Logical OR of expressions
    Or(Vec<ExprAst>),
    /// Logical NOT
    Not(Box<ExprAst>),
    /// Binary comparison: path op pattern
    Compare {
        left: PathAst,
        op: OpAst,
        right: PatternAst,
    },
    /// Collection membership: pattern in path
    In { pattern: PatternAst, path: PathAst },
    /// Inline collection membership: path in [literal, literal, ...]
    /// The path value is checked against an inline array of patterns.
    InLiteral {
        path: PathAst,
        values: Vec<PatternAst>,
    },
    /// Attribute existence check: has path (checks field is present/non-null)
    Has { path: PathAst },
    /// Glob pattern matching: path like "pattern*"
    Like { path: PathAst, pattern: String },
    /// Universal: forall var in path: expr
    Forall {
        var: String,
        path: PathAst,
        body: Box<ExprAst>,
    },
    /// Existential: exists var in path: expr
    Exists {
        var: String,
        path: PathAst,
        body: Box<ExprAst>,
    },
    /// Namespaced type reference as boolean condition: `MyCedarNamespace:Delete`
    TypeRef {
        namespace: Option<String>,
        name: String,
    },
    /// Type membership check: `resource is File`
    IsType { path: PathAst, type_name: String },
}

/// A path expression (e.g., `resource.context.args`).
#[derive(Debug, Clone)]
pub struct PathAst {
    pub segments: Vec<PathSegmentAst>,
}

/// A segment in a path.
#[derive(Debug, Clone)]
pub enum PathSegmentAst {
    Field(String),
    Index(usize),
    /// A variable reference (e.g., `path[variable]`)
    Variable(String),
}

/// Comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpAst {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    ContainsAll,
    ContainsAny,
    // Cedar extension function operators
    IpIsInRange,
    IsIpv4,
    IsIpv6,
    IsLoopback,
    IsMulticast,
    DecimalLt,
    DecimalLe,
    DecimalGt,
    DecimalGe,
}

/// A pattern for matching values.
#[derive(Debug, Clone)]
pub enum PatternAst {
    /// Literal value
    Literal(Value),
    /// Variable (captures value)
    Variable(String),
    /// Wildcard (matches anything, no capture)
    Wildcard,
    /// Object pattern with field patterns
    Object(Vec<(String, PatternAst)>),
    /// Array pattern
    Array(Vec<PatternAst>),
    /// Path reference (for path-to-path comparison like resource.ownerId)
    PathRef(PathAst),
}
