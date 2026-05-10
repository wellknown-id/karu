// SPDX-License-Identifier: MIT

//! Import Cedar policies into Karu.
//!
//! Converts Cedar policy AST to Karu AST for interoperability with
//! Cedar-based authorization systems.
//!
//! # Supported Cedar Features
//!
//! - `permit`/`forbid` → `allow`/`deny`
//! - Scope constraints: `principal == Entity`, `action == Entity`,
//!   `action in [Entity, ...]`, `resource in Entity`
//! - `principal is Type` / `resource is Type` → `principal.type == "Type"` (convention)
//! - `principal is Type in Group` → type check AND group membership check
//! - `when` conditions → rule body conditions
//! - `unless` conditions → negated rule body conditions
//! - Expressions: `&&`, `||`, `!`, `==`, `!=`, `<`, `<=`, `>`, `>=`
//! - Entity references: `Type::"id"` → string literal `"id"`
//! - Dot access: `resource.field`, `context.key`
//! - Method calls: `.contains()` → `in` expression
//! - `has` attribute existence test → `has { path }`
//! - `like` glob pattern matching → `like { path, pattern }`
//! - IP extension methods: `ip(path).isInRange(ip("cidr"))`, `.isIpv4()`, etc.
//! - Decimal extension methods: `.lessThan()`, `.lessThanOrEqual()`, etc.
//! - `@id("name")` annotations → rule name
//! - Variables: `principal`, `action`, `resource`, `context`
//!
//! # Unsupported Cedar Features
//!
//! The following Cedar features will produce an explicit error:
//! - `if-then-else` expressions (unless trivially `if C then true else false`)
//! - Arithmetic (`+`, `-`, `*`) (unless constant-foldable at import time)
//! - `datetime()` / `duration()` extension functions
//! - Template slots (`?principal`, `?resource`)
//! - Set/record literals in conditions
//!
//! # Notes on `is` Type Tests
//!
//! Cedar's `principal is User` checks that the principal entity has type `User`.
//! Karu maps this to `principal.type == "User"`, which requires entity data
//! to carry a `type` field. Similarly, `principal is User in Group::"g1"` maps
//! to `principal.type == "User" AND "g1" in principal.groups`.
//!
//! # Example
//!
//! ```rust
//! use karu::cedar_import::from_cedar;
//!
//! let cedar = r#"
//!     permit(principal, action == Action::"view", resource)
//!     when { resource.public == true };
//! "#;
//!
//! let program = from_cedar(cedar).unwrap();
//! assert_eq!(program.rules.len(), 1);
//! assert_eq!(program.rules[0].name, "policy_0");
//! ```

use crate::ast::*;
use crate::cedar_parser::{
    self, CedarActionConstraint, CedarAddOp, CedarEffect, CedarExpr, CedarParseError, CedarPolicy,
    CedarRelOp, CedarScopeConstraint,
};

/// Error during import from Cedar.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportError {
    pub message: String,
    pub line: Option<usize>,
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(line) = self.line {
            write!(f, "ImportError at line {}: {}", line, self.message)
        } else {
            write!(f, "ImportError: {}", self.message)
        }
    }
}

impl std::error::Error for ImportError {}

impl From<CedarParseError> for ImportError {
    fn from(e: CedarParseError) -> Self {
        ImportError {
            message: e.message,
            line: Some(e.line),
        }
    }
}

/// Convert Cedar policy source to a Karu program AST.
///
/// Parses Cedar source using the Cedar parser, then converts each
/// Cedar policy to a Karu rule via AST-level transformation.
pub fn from_cedar(cedar_source: &str) -> Result<Program, ImportError> {
    let policy_set = cedar_parser::parse(cedar_source)?;

    let mut rules = Vec::new();
    for (i, policy) in policy_set.policies.iter().enumerate() {
        let rule = convert_policy(policy, i)?;
        rules.push(rule);
    }

    Ok(Program {
        use_schema: false,
        imports: vec![],
        modules: vec![],
        assertions: vec![],
        rules,
        tests: vec![],
    })
}

/// Convert Cedar schema source to Karu module definitions.
///
/// Parses a `.cedarschema` file and converts it to Karu's `ModuleDef` AST.
pub fn from_cedarschema(schema_source: &str) -> Result<Vec<crate::schema::ModuleDef>, ImportError> {
    crate::cedar_schema_parser::parse_cedarschema(schema_source).map_err(|e| ImportError {
        message: e.message,
        line: Some(e.line),
    })
}

/// Convert Cedar policy and schema sources into a combined Karu program.
///
/// This produces a `Program` with `use_schema: true`, combining both
/// the schema declarations (as `ModuleDef`s) and the policies (as rules).
pub fn from_cedar_with_schema(
    cedar_source: &str,
    schema_source: &str,
) -> Result<Program, ImportError> {
    let mut program = from_cedar(cedar_source)?;
    let modules = from_cedarschema(schema_source)?;
    program.use_schema = true;
    program.modules = modules;
    Ok(program)
}

/// Convert Cedar policy source to Karu policy source text.
///
/// This is a convenience wrapper that converts Cedar to Karu AST
/// and then serializes it back to Karu source text.
pub fn from_cedar_to_source(cedar_source: &str) -> Result<String, ImportError> {
    let program = from_cedar(cedar_source)?;
    let mut output = String::new();

    for rule in &program.rules {
        let effect = match rule.effect {
            EffectAst::Allow => "allow",
            EffectAst::Deny => "deny",
        };
        if let Some(ref body) = rule.body {
            output.push_str(&format!("{} {} if\n", effect, rule.name));
            output.push_str(&format_expr(body, 1));
            output.push_str(";\n");
        } else {
            output.push_str(&format!("{} {};\n", effect, rule.name));
        }
    }

    Ok(output)
}

// ============================================================================
// Policy → Rule conversion
// ============================================================================

fn convert_policy(policy: &CedarPolicy, index: usize) -> Result<RuleAst, ImportError> {
    // Determine rule name from annotation @id("name") or auto-generate
    let name = policy
        .annotations
        .iter()
        .find(|a| a.key == "id")
        .and_then(|a| a.value.clone())
        .unwrap_or_else(|| format!("policy_{}", index));

    let effect = match policy.effect {
        CedarEffect::Permit => EffectAst::Allow,
        CedarEffect::Forbid => EffectAst::Deny,
    };

    // Build list of conditions from scope + when/unless
    let mut conditions: Vec<ExprAst> = Vec::new();

    // Convert scope constraints
    convert_scope_principal(&policy.scope.principal, &mut conditions)?;
    convert_scope_action(&policy.scope.action, &mut conditions)?;
    convert_scope_resource(&policy.scope.resource, &mut conditions)?;

    // Convert when/unless conditions
    for condition in &policy.conditions {
        let expr = convert_expr(&condition.expr)?;
        if condition.is_when {
            conditions.push(expr);
        } else {
            // unless → negate
            conditions.push(ExprAst::Not(Box::new(expr)));
        }
    }

    // Combine conditions with AND
    let body = match conditions.len() {
        0 => None,
        1 => Some(conditions.remove(0)),
        _ => Some(ExprAst::And(conditions)),
    };

    Ok(RuleAst { name, effect, body })
}

// ============================================================================
// Scope constraint → Karu conditions
// ============================================================================

fn convert_scope_principal(
    constraint: &CedarScopeConstraint,
    conditions: &mut Vec<ExprAst>,
) -> Result<(), ImportError> {
    match constraint {
        CedarScopeConstraint::Any => {} // No constraint
        CedarScopeConstraint::Eq(entity) => {
            conditions.push(ExprAst::Compare {
                left: make_path("principal"),
                op: OpAst::Eq,
                right: PatternAst::Literal(serde_json::Value::String(entity.id.clone())),
            });
        }
        CedarScopeConstraint::In(entity) => {
            conditions.push(ExprAst::In {
                pattern: PatternAst::Literal(serde_json::Value::String(entity.id.clone())),
                path: make_path("principal.groups"),
            });
        }
        CedarScopeConstraint::Is(type_name) => {
            // `principal is Type` → principal.type == "Type"
            // Requires entity data to carry a `type` field.
            conditions.push(ExprAst::Compare {
                left: make_path("principal.type"),
                op: OpAst::Eq,
                right: PatternAst::Literal(serde_json::Value::String(type_name.clone())),
            });
        }
        CedarScopeConstraint::IsIn(type_name, entity) => {
            // `principal is Type in Group` → type check AND group membership
            conditions.push(ExprAst::Compare {
                left: make_path("principal.type"),
                op: OpAst::Eq,
                right: PatternAst::Literal(serde_json::Value::String(type_name.clone())),
            });
            conditions.push(ExprAst::In {
                pattern: PatternAst::Literal(serde_json::Value::String(entity.id.clone())),
                path: make_path("principal.groups"),
            });
        }
        CedarScopeConstraint::Slot(slot) => {
            return Err(unsupported(format!(
                "Template slot '{}' not supported",
                slot
            )));
        }
    }
    Ok(())
}

fn convert_scope_action(
    constraint: &CedarActionConstraint,
    conditions: &mut Vec<ExprAst>,
) -> Result<(), ImportError> {
    match constraint {
        CedarActionConstraint::Any => {} // No constraint
        CedarActionConstraint::Eq(entity) => {
            conditions.push(ExprAst::Compare {
                left: make_path("action"),
                op: OpAst::Eq,
                right: PatternAst::Literal(serde_json::Value::String(entity.id.clone())),
            });
        }
        CedarActionConstraint::In(entity) => {
            // action in ActionGroup::"readOnly" → action == "readOnly"
            // (simplified - Cedar hierarchy not modeled)
            conditions.push(ExprAst::Compare {
                left: make_path("action"),
                op: OpAst::Eq,
                right: PatternAst::Literal(serde_json::Value::String(entity.id.clone())),
            });
        }
        CedarActionConstraint::InList(entities) => {
            // action in [Action::"view", Action::"edit"] → OR
            let checks: Vec<ExprAst> = entities
                .iter()
                .map(|e| ExprAst::Compare {
                    left: make_path("action"),
                    op: OpAst::Eq,
                    right: PatternAst::Literal(serde_json::Value::String(e.id.clone())),
                })
                .collect();
            if checks.len() == 1 {
                conditions.push(checks.into_iter().next().unwrap());
            } else {
                conditions.push(ExprAst::Or(checks));
            }
        }
    }
    Ok(())
}

fn convert_scope_resource(
    constraint: &CedarScopeConstraint,
    conditions: &mut Vec<ExprAst>,
) -> Result<(), ImportError> {
    match constraint {
        CedarScopeConstraint::Any => {} // No constraint
        CedarScopeConstraint::Eq(entity) => {
            conditions.push(ExprAst::Compare {
                left: make_path("resource"),
                op: OpAst::Eq,
                right: PatternAst::Literal(serde_json::Value::String(entity.id.clone())),
            });
        }
        CedarScopeConstraint::In(entity) => {
            // resource in Album::"vacation" → resource.album == "vacation"
            // Use the entity type as the field name (lowercased)
            let field = entity
                .path
                .last()
                .map(|s| s.to_lowercase())
                .unwrap_or_else(|| "container".into());
            conditions.push(ExprAst::Compare {
                left: make_path(&format!("resource.{}", field)),
                op: OpAst::Eq,
                right: PatternAst::Literal(serde_json::Value::String(entity.id.clone())),
            });
        }
        CedarScopeConstraint::Is(type_name) => {
            // `resource is Type` → resource.type == "Type"
            // Requires entity data to carry a `type` field.
            conditions.push(ExprAst::Compare {
                left: make_path("resource.type"),
                op: OpAst::Eq,
                right: PatternAst::Literal(serde_json::Value::String(type_name.clone())),
            });
        }
        CedarScopeConstraint::IsIn(type_name, entity) => {
            // `resource is Type in Container` → type check AND container field equality.
            // Follows the same convention as `resource in Container` above: resources model
            // parent membership as a named field (e.g. `resource.album == "vacation"`) rather
            // than a `groups` array, because Cedar resource hierarchies are typically
            // single-parent containment rather than multi-group membership.
            conditions.push(ExprAst::Compare {
                left: make_path("resource.type"),
                op: OpAst::Eq,
                right: PatternAst::Literal(serde_json::Value::String(type_name.clone())),
            });
            // Derive field name from the container type name, lowercased
            // (e.g. `Album::"vacation"` → `resource.album == "vacation"`).
            // Falls back to "container" if the entity path is empty.
            let field = entity
                .path
                .last()
                .map(|s| s.to_lowercase())
                .unwrap_or_else(|| "container".into());
            conditions.push(ExprAst::Compare {
                left: make_path(&format!("resource.{}", field)),
                op: OpAst::Eq,
                right: PatternAst::Literal(serde_json::Value::String(entity.id.clone())),
            });
        }
        CedarScopeConstraint::Slot(slot) => {
            return Err(unsupported(format!(
                "Template slot '{}' not supported",
                slot
            )));
        }
    }
    Ok(())
}

// ============================================================================
// Expression conversion
// ============================================================================

fn convert_expr(expr: &CedarExpr) -> Result<ExprAst, ImportError> {
    match expr {
        CedarExpr::And(_, _) => {
            // Flatten nested ANDs
            let mut parts = Vec::new();
            flatten_and(expr, &mut parts)?;
            Ok(ExprAst::And(parts))
        }
        CedarExpr::Or(_, _) => {
            // Flatten nested ORs
            let mut parts = Vec::new();
            flatten_or(expr, &mut parts)?;
            Ok(ExprAst::Or(parts))
        }
        CedarExpr::Not(inner) => Ok(ExprAst::Not(Box::new(convert_expr(inner)?))),
        CedarExpr::Relation { lhs, op, rhs } => convert_relation(lhs, *op, rhs),
        CedarExpr::InExpr { lhs, rhs } => convert_in_expr(lhs, rhs),
        CedarExpr::Has(base_expr, field) => {
            let mut path = expr_to_path(base_expr)?;
            path.segments.push(PathSegmentAst::Field(field.clone()));
            Ok(ExprAst::Has { path })
        }
        CedarExpr::Like(base_expr, pat) => {
            let path = expr_to_path(base_expr)?;
            Ok(ExprAst::Like {
                path,
                pattern: pat.clone(),
            })
        }
        CedarExpr::Is {
            expr,
            type_name,
            in_expr,
        } => {
            // `expr is TypeName` → expr.type == "TypeName"
            // Requires entity data to carry a `type` field.
            let mut type_path = expr_to_path(expr)?;
            type_path
                .segments
                .push(PathSegmentAst::Field("type".to_string()));
            let type_check = ExprAst::Compare {
                left: type_path,
                op: OpAst::Eq,
                right: PatternAst::Literal(serde_json::Value::String(type_name.clone())),
            };
            if let Some(in_expr) = in_expr {
                // `expr is TypeName in Group` → type_check AND group membership
                let in_entity = match in_expr.as_ref() {
                    CedarExpr::Entity(e) => e,
                    _ => {
                        return Err(unsupported(
                            "'is ... in' expects an entity reference as the group",
                        ))
                    }
                };
                let mut groups_path = expr_to_path(expr)?;
                groups_path
                    .segments
                    .push(PathSegmentAst::Field("groups".to_string()));
                let group_check = ExprAst::In {
                    pattern: PatternAst::Literal(serde_json::Value::String(in_entity.id.clone())),
                    path: groups_path,
                };
                Ok(ExprAst::And(vec![type_check, group_check]))
            } else {
                Ok(type_check)
            }
        }
        CedarExpr::IfThenElse {
            cond,
            then_expr,
            else_expr,
        } => {
            // Decompose: if C then T else E → Or(And(C, T), And(Not(C), E))
            // Short-circuit for common cases:
            let cond_expr = convert_expr(cond)?;
            match (then_expr.as_ref(), else_expr.as_ref()) {
                // if C then true else false → C
                (CedarExpr::Bool(true), CedarExpr::Bool(false)) => Ok(cond_expr),
                // if C then false else true → Not(C)
                (CedarExpr::Bool(false), CedarExpr::Bool(true)) => {
                    Ok(ExprAst::Not(Box::new(cond_expr)))
                }
                // if C then T else false → And(C, T)
                (_, CedarExpr::Bool(false)) => {
                    let then_ast = convert_expr(then_expr)?;
                    Ok(ExprAst::And(vec![cond_expr, then_ast]))
                }
                // if C then true else E → Or(C, E)
                (CedarExpr::Bool(true), _) => {
                    let else_ast = convert_expr(else_expr)?;
                    Ok(ExprAst::Or(vec![cond_expr, else_ast]))
                }
                // General case: Or(And(C, T), And(Not(C), E))
                _ => {
                    let then_ast = convert_expr(then_expr)?;
                    let else_ast = convert_expr(else_expr)?;
                    Ok(ExprAst::Or(vec![
                        ExprAst::And(vec![cond_expr.clone(), then_ast]),
                        ExprAst::And(vec![ExprAst::Not(Box::new(cond_expr)), else_ast]),
                    ]))
                }
            }
        }
        CedarExpr::Add(lhs, op, rhs) => {
            // Try to evaluate arithmetic at import time
            if let (Some(a), Some(b)) = (expr_to_i64(lhs), expr_to_i64(rhs)) {
                let result = match op {
                    CedarAddOp::Add => a + b,
                    CedarAddOp::Sub => a - b,
                };
                // Return as a literal comparison against a wildcard path
                Ok(ExprAst::Compare {
                    left: make_path("_computed"),
                    op: OpAst::Eq,
                    right: PatternAst::Literal(serde_json::Value::Number(
                        serde_json::Number::from(result),
                    )),
                })
            } else {
                Err(unsupported(format!(
                    "Arithmetic '{}' with non-literal operands not yet supported",
                    match op {
                        CedarAddOp::Add => "+",
                        CedarAddOp::Sub => "-",
                    }
                )))
            }
        }
        CedarExpr::Mul(lhs, rhs) => {
            if let (Some(a), Some(b)) = (expr_to_i64(lhs), expr_to_i64(rhs)) {
                Ok(ExprAst::Compare {
                    left: make_path("_computed"),
                    op: OpAst::Eq,
                    right: PatternAst::Literal(serde_json::Value::Number(
                        serde_json::Number::from(a * b),
                    )),
                })
            } else {
                Err(unsupported(
                    "Arithmetic '*' with non-literal operands not yet supported",
                ))
            }
        }
        CedarExpr::Neg(inner) => {
            // -literal → negative literal
            if let Some(n) = expr_to_i64(inner) {
                Ok(ExprAst::Compare {
                    left: make_path("_computed"),
                    op: OpAst::Eq,
                    right: PatternAst::Literal(serde_json::Value::Number(
                        serde_json::Number::from(-n),
                    )),
                })
            } else {
                Err(unsupported(
                    "Unary negation of non-literal not yet supported",
                ))
            }
        }
        CedarExpr::MethodCall(base, method, args) => convert_method_call(base, method, args),
        CedarExpr::Access(_, _) => {
            // path.field used as boolean condition → path.field == true
            let path = expr_to_path(expr)?;
            Ok(ExprAst::Compare {
                left: path,
                op: OpAst::Eq,
                right: PatternAst::Literal(serde_json::Value::Bool(true)),
            })
        }
        CedarExpr::Index(_, key) => Err(unsupported(format!(
            "Index access '[\"{}\"]' not directly supported",
            key
        ))),
        CedarExpr::Var(name) => {
            // Standalone var like `principal` - when used as condition, treat as truthy
            let path = make_path(name);
            Ok(ExprAst::Compare {
                left: path,
                op: OpAst::Eq,
                right: PatternAst::Literal(serde_json::Value::Bool(true)),
            })
        }
        CedarExpr::Bool(true) => {
            // `true` as condition → no-op (always matches)
            // We represent this as a wildcard compare
            Ok(ExprAst::Compare {
                left: make_path("_always"),
                op: OpAst::Eq,
                right: PatternAst::Wildcard,
            })
        }
        CedarExpr::Bool(false) => {
            // `false` → negate true
            Ok(ExprAst::Not(Box::new(ExprAst::Compare {
                left: make_path("_always"),
                op: OpAst::Eq,
                right: PatternAst::Wildcard,
            })))
        }
        CedarExpr::Int(_) | CedarExpr::Str(_) => {
            Err(unsupported("Standalone literal not supported as condition"))
        }
        CedarExpr::Entity(_) => Err(unsupported(
            "Standalone entity reference not supported as condition",
        )),
        CedarExpr::ExtFun(name, _) => Err(unsupported(format!(
            "Extension function '{}' not supported",
            name
        ))),
        CedarExpr::Set(_) => Err(unsupported("Set literals not supported as condition")),
        CedarExpr::Record(_) => Err(unsupported("Record literals not supported as condition")),
    }
}

fn flatten_and(expr: &CedarExpr, parts: &mut Vec<ExprAst>) -> Result<(), ImportError> {
    match expr {
        CedarExpr::And(lhs, rhs) => {
            flatten_and(lhs, parts)?;
            flatten_and(rhs, parts)?;
        }
        _ => parts.push(convert_expr(expr)?),
    }
    Ok(())
}

fn flatten_or(expr: &CedarExpr, parts: &mut Vec<ExprAst>) -> Result<(), ImportError> {
    match expr {
        CedarExpr::Or(lhs, rhs) => {
            flatten_or(lhs, parts)?;
            flatten_or(rhs, parts)?;
        }
        _ => parts.push(convert_expr(expr)?),
    }
    Ok(())
}

fn convert_relation(
    lhs: &CedarExpr,
    op: CedarRelOp,
    rhs: &CedarExpr,
) -> Result<ExprAst, ImportError> {
    let karu_op = match op {
        CedarRelOp::Eq => OpAst::Eq,
        CedarRelOp::Neq => OpAst::Ne,
        CedarRelOp::Lt => OpAst::Lt,
        CedarRelOp::Lte => OpAst::Le,
        CedarRelOp::Gt => OpAst::Gt,
        CedarRelOp::Gte => OpAst::Ge,
    };

    // Try normal path-based comparison first
    match expr_to_path(lhs) {
        Ok(left) => {
            let right = expr_to_pattern(rhs)?;
            Ok(ExprAst::Compare {
                left,
                op: karu_op,
                right,
            })
        }
        Err(_) => {
            // LHS isn't a path - try constant-folding both sides
            if let (Some(a), Some(b)) = (expr_to_i64(lhs), expr_to_i64(rhs)) {
                let result = match op {
                    CedarRelOp::Eq => a == b,
                    CedarRelOp::Neq => a != b,
                    CedarRelOp::Lt => a < b,
                    CedarRelOp::Lte => a <= b,
                    CedarRelOp::Gt => a > b,
                    CedarRelOp::Gte => a >= b,
                };
                // Convert to boolean expression
                convert_expr(&CedarExpr::Bool(result))
            } else {
                Err(unsupported(format!(
                    "Cannot convert expression to path: {:?}",
                    lhs
                )))
            }
        }
    }
}

fn convert_in_expr(lhs: &CedarExpr, rhs: &CedarExpr) -> Result<ExprAst, ImportError> {
    let pattern = expr_to_pattern(lhs)?;
    let path = expr_to_path(rhs)?;
    Ok(ExprAst::In { pattern, path })
}

fn convert_method_call(
    base: &CedarExpr,
    method: &str,
    args: &[CedarExpr],
) -> Result<ExprAst, ImportError> {
    match method {
        "contains" => {
            // base.contains(arg) → arg in base
            if args.len() != 1 {
                return Err(unsupported("contains() expects exactly one argument"));
            }
            let path = expr_to_path(base)?;
            let pattern = expr_to_pattern(&args[0])?;
            Ok(ExprAst::In { pattern, path })
        }
        "containsAll" | "containsAny" => {
            // collection.containsAll([a, b]) → And(In(a, coll), In(b, coll))
            // collection.containsAny([a, b]) → Or(In(a, coll), In(b, coll))
            if args.len() != 1 {
                return Err(unsupported(format!(
                    "{}() expects exactly one argument",
                    method
                )));
            }
            let path = expr_to_path(base)?;
            // The argument should be a set literal [a, b, c]
            let elements = match &args[0] {
                CedarExpr::Set(elems) => elems,
                _ => {
                    return Err(unsupported(format!(
                        "{}() argument must be a set literal",
                        method
                    )));
                }
            };
            if elements.is_empty() {
                // containsAll([]) is always true, containsAny([]) is always false
                // For simplicity, treat as unsupported edge case
                return Err(unsupported(format!("{}() with empty set", method)));
            }
            // Convert set literal to array pattern
            let arr_values: Vec<serde_json::Value> = elements
                .iter()
                .map(|elem| match expr_to_pattern(elem)? {
                    PatternAst::Literal(v) => Ok(v),
                    _ => Err(unsupported(format!(
                        "{}() elements must be literals",
                        method
                    ))),
                })
                .collect::<Result<_, _>>()?;
            let op = if method == "containsAll" {
                OpAst::ContainsAll
            } else {
                OpAst::ContainsAny
            };
            Ok(ExprAst::Compare {
                left: path,
                op,
                right: PatternAst::Literal(serde_json::Value::Array(arr_values)),
            })
        }
        // ── IP extension methods ──
        "isInRange" => {
            // ip(path).isInRange(ip("cidr")) → Compare(path, IpIsInRange, "cidr")
            if args.len() != 1 {
                return Err(unsupported("isInRange() expects exactly one argument"));
            }
            let path = extfun_to_path(base, "ip")?;
            let cidr = extfun_to_literal(&args[0], "ip")?;
            Ok(ExprAst::Compare {
                left: path,
                op: OpAst::IpIsInRange,
                right: PatternAst::Literal(serde_json::Value::String(cidr)),
            })
        }
        "isIpv4" => {
            let path = extfun_to_path(base, "ip")?;
            Ok(ExprAst::Compare {
                left: path,
                op: OpAst::IsIpv4,
                right: PatternAst::Wildcard,
            })
        }
        "isIpv6" => {
            let path = extfun_to_path(base, "ip")?;
            Ok(ExprAst::Compare {
                left: path,
                op: OpAst::IsIpv6,
                right: PatternAst::Wildcard,
            })
        }
        "isLoopback" => {
            let path = extfun_to_path(base, "ip")?;
            Ok(ExprAst::Compare {
                left: path,
                op: OpAst::IsLoopback,
                right: PatternAst::Wildcard,
            })
        }
        "isMulticast" => {
            let path = extfun_to_path(base, "ip")?;
            Ok(ExprAst::Compare {
                left: path,
                op: OpAst::IsMulticast,
                right: PatternAst::Wildcard,
            })
        }
        // ── Decimal extension methods ──
        "lessThan" | "lessThanOrEqual" | "greaterThan" | "greaterThanOrEqual" => {
            if args.len() != 1 {
                return Err(unsupported(format!(
                    "{}() expects exactly one argument",
                    method
                )));
            }
            let path = extfun_to_path(base, "decimal")?;
            let value = extfun_to_literal(&args[0], "decimal")?;
            let op = match method {
                "lessThan" => OpAst::DecimalLt,
                "lessThanOrEqual" => OpAst::DecimalLe,
                "greaterThan" => OpAst::DecimalGt,
                "greaterThanOrEqual" => OpAst::DecimalGe,
                _ => unreachable!(),
            };
            Ok(ExprAst::Compare {
                left: path,
                op,
                right: PatternAst::Literal(serde_json::Value::String(value)),
            })
        }
        _ => Err(unsupported(format!(
            "Method call '.{}()' not supported",
            method
        ))),
    }
}

/// Extract the inner path from an extension function call like `ip(expr)` or `decimal(expr)`.
/// If the argument is a string literal, treats it as a literal comparison (rare in real policies).
/// If the argument is a path expression, returns the path.
fn extfun_to_path(expr: &CedarExpr, expected_fn: &str) -> Result<PathAst, ImportError> {
    match expr {
        CedarExpr::ExtFun(name, args) if name == expected_fn && args.len() == 1 => {
            // Try to interpret the argument as a path
            match &args[0] {
                CedarExpr::Var(v) => Ok(make_path(v)),
                CedarExpr::Access(_, _) => expr_to_path(&args[0]),
                CedarExpr::Str(s) => {
                    // Literal string like ip("10.0.0.1") - create a synthetic literal path
                    // This case happens when comparing two literals, which is unusual
                    // For now, embed as a single-segment path that won't resolve
                    // The evaluator will see the string value directly
                    Ok(PathAst {
                        segments: vec![PathSegmentAst::Field(format!("@lit:{}", s))],
                    })
                }
                _ => Err(unsupported(format!(
                    "{}() argument must be a path or string literal",
                    expected_fn
                ))),
            }
        }
        _ => Err(unsupported(format!(
            "Expected {}() call, got {:?}",
            expected_fn, expr
        ))),
    }
}

/// Extract a string literal from an extension function call like `ip("10.0.0.0/24")`.
fn extfun_to_literal(expr: &CedarExpr, expected_fn: &str) -> Result<String, ImportError> {
    match expr {
        CedarExpr::ExtFun(name, args) if name == expected_fn && args.len() == 1 => match &args[0] {
            CedarExpr::Str(s) => Ok(s.clone()),
            _ => Err(unsupported(format!(
                "{}() argument must be a string literal",
                expected_fn
            ))),
        },
        _ => Err(unsupported(format!(
            "Expected {}() call with string literal",
            expected_fn
        ))),
    }
}

// ============================================================================
// Expression → Path/Pattern helpers
// ============================================================================

/// Try to extract a compile-time integer from a Cedar expression.
fn expr_to_i64(expr: &CedarExpr) -> Option<i64> {
    match expr {
        CedarExpr::Int(n) => Some(*n),
        CedarExpr::Neg(inner) => expr_to_i64(inner).map(|n| -n),
        CedarExpr::Add(lhs, op, rhs) => {
            let a = expr_to_i64(lhs)?;
            let b = expr_to_i64(rhs)?;
            match op {
                CedarAddOp::Add => Some(a + b),
                CedarAddOp::Sub => Some(a - b),
            }
        }
        CedarExpr::Mul(lhs, rhs) => {
            let a = expr_to_i64(lhs)?;
            let b = expr_to_i64(rhs)?;
            Some(a * b)
        }
        _ => None,
    }
}

fn expr_to_path(expr: &CedarExpr) -> Result<PathAst, ImportError> {
    match expr {
        CedarExpr::Var(name) => Ok(make_path(name)),
        CedarExpr::Access(base, field) => {
            let mut path = expr_to_path(base)?;
            path.segments.push(PathSegmentAst::Field(field.clone()));
            Ok(path)
        }
        CedarExpr::Entity(entity_ref) => {
            // Entity in path position: treat as string value via path
            // This is a simplification - entities become their ID
            Err(unsupported(format!(
                "Entity reference {}::\"{}\" cannot be used as a path",
                entity_ref.path.join("::"),
                entity_ref.id
            )))
        }
        _ => Err(unsupported(format!(
            "Cannot convert expression to path: {:?}",
            expr
        ))),
    }
}

fn expr_to_pattern(expr: &CedarExpr) -> Result<PatternAst, ImportError> {
    match expr {
        CedarExpr::Str(s) => Ok(PatternAst::Literal(serde_json::Value::String(s.clone()))),
        CedarExpr::Int(n) => Ok(PatternAst::Literal(serde_json::json!(*n))),
        CedarExpr::Bool(b) => Ok(PatternAst::Literal(serde_json::Value::Bool(*b))),
        CedarExpr::Entity(entity_ref) => {
            // Entity::"id" → string literal "id"
            Ok(PatternAst::Literal(serde_json::Value::String(
                entity_ref.id.clone(),
            )))
        }
        CedarExpr::Var(name) => {
            // Variable in pattern position → path reference
            Ok(PatternAst::PathRef(make_path(name)))
        }
        CedarExpr::Access(_, _) => {
            // Path expression in pattern position → path reference
            let path = expr_to_path(expr)?;
            Ok(PatternAst::PathRef(path))
        }
        CedarExpr::Set(elements) => {
            // Set literal [a, b, c] → Array pattern
            let patterns: Vec<PatternAst> = elements
                .iter()
                .map(expr_to_pattern)
                .collect::<Result<_, _>>()?;
            Ok(PatternAst::Array(patterns))
        }
        CedarExpr::Record(fields) => {
            // Record literal {k: v, ...} → Object pattern
            let pairs: Vec<(String, PatternAst)> = fields
                .iter()
                .map(|(k, v)| Ok::<_, ImportError>((k.clone(), expr_to_pattern(v)?)))
                .collect::<Result<_, _>>()?;
            Ok(PatternAst::Object(pairs))
        }
        _ => Err(unsupported(format!(
            "Cannot convert expression to pattern: {:?}",
            expr
        ))),
    }
}

// ============================================================================
// Helpers
// ============================================================================

fn make_path(dotted: &str) -> PathAst {
    PathAst {
        segments: dotted
            .split('.')
            .map(|s| PathSegmentAst::Field(s.to_string()))
            .collect(),
    }
}

fn path_to_string(path: &PathAst) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    for (i, segment) in path.segments.iter().enumerate() {
        if i > 0 {
            s.push('.');
        }
        match segment {
            PathSegmentAst::Field(f) => s.push_str(f),
            PathSegmentAst::Index(idx) => write!(s, "[{}]", idx).unwrap(),
            PathSegmentAst::Variable(v) => s.push_str(v),
        }
    }
    s
}

fn format_expr(expr: &ExprAst, indent: usize) -> String {
    let pad = "    ".repeat(indent);
    match expr {
        ExprAst::And(parts) => parts
            .iter()
            .map(|p| format_expr(p, indent))
            .collect::<Vec<_>>()
            .join(&format!(" and\n{}", pad)),
        ExprAst::Or(parts) => {
            let inner = parts
                .iter()
                .map(|p| format_expr(p, indent))
                .collect::<Vec<_>>()
                .join(" or ");
            format!("({})", inner)
        }
        ExprAst::Not(inner) => format!("not {}", format_expr(inner, indent)),
        ExprAst::Compare { left, op, right } => {
            let op_str = match op {
                OpAst::Eq => "==",
                OpAst::Ne => "!=",
                OpAst::Lt => "<",
                OpAst::Gt => ">",
                OpAst::Le => "<=",
                OpAst::Ge => ">=",
                OpAst::ContainsAll => "containsAll",
                OpAst::ContainsAny => "containsAny",
                OpAst::IpIsInRange => "isInRange",
                OpAst::IsIpv4 => "isIpv4",
                OpAst::IsIpv6 => "isIpv6",
                OpAst::IsLoopback => "isLoopback",
                OpAst::IsMulticast => "isMulticast",
                OpAst::DecimalLt => "lessThan",
                OpAst::DecimalLe => "lessThanOrEqual",
                OpAst::DecimalGt => "greaterThan",
                OpAst::DecimalGe => "greaterThanOrEqual",
            };
            format!(
                "{}{} {} {}",
                pad,
                path_to_string(left),
                op_str,
                format_pattern(right)
            )
        }
        ExprAst::In { pattern, path } => {
            format!(
                "{}{} in {}",
                pad,
                format_pattern(pattern),
                path_to_string(path)
            )
        }
        _ => format!("{}/* unsupported */", pad),
    }
}

fn format_pattern(pat: &PatternAst) -> String {
    match pat {
        PatternAst::Literal(v) => match v {
            serde_json::Value::String(s) => format!("\"{}\"", s),
            other => other.to_string(),
        },
        PatternAst::PathRef(p) => path_to_string(p),
        PatternAst::Wildcard => "_".into(),
        _ => "/* complex */".into(),
    }
}

fn unsupported(msg: impl Into<String>) -> ImportError {
    ImportError {
        message: msg.into(),
        line: None,
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
        let program = from_cedar(r#"permit(principal, action, resource);"#).unwrap();
        assert_eq!(program.rules.len(), 1);
        assert_eq!(program.rules[0].effect, EffectAst::Allow);
        assert!(program.rules[0].body.is_none()); // No conditions
    }

    #[test]
    fn test_forbid() {
        let program = from_cedar(r#"forbid(principal, action, resource);"#).unwrap();
        assert_eq!(program.rules[0].effect, EffectAst::Deny);
    }

    #[test]
    fn test_with_entity() {
        let program =
            from_cedar(r#"permit(principal == User::"alice", action, resource);"#).unwrap();
        if let Some(ExprAst::Compare { left, op, right }) = &program.rules[0].body {
            assert_eq!(path_to_string(left), "principal");
            assert_eq!(*op, OpAst::Eq);
            assert!(
                matches!(right, PatternAst::Literal(serde_json::Value::String(s)) if s == "alice")
            );
        } else {
            panic!("Expected Compare expression");
        }
    }

    #[test]
    fn test_action_constraint() {
        let program =
            from_cedar(r#"permit(principal, action == Action::"view", resource);"#).unwrap();
        if let Some(ExprAst::Compare { left, .. }) = &program.rules[0].body {
            assert_eq!(path_to_string(left), "action");
        } else {
            panic!("Expected Compare expression");
        }
    }

    #[test]
    fn test_action_in_list() {
        let program = from_cedar(
            r#"permit(principal, action in [Action::"view", Action::"edit"], resource);"#,
        )
        .unwrap();
        if let Some(ExprAst::Or(parts)) = &program.rules[0].body {
            assert_eq!(parts.len(), 2);
        } else {
            panic!("Expected Or expression for action list");
        }
    }

    #[test]
    fn test_when_condition() {
        let program =
            from_cedar(r#"permit(principal, action, resource) when { resource.public == true };"#)
                .unwrap();
        assert!(program.rules[0].body.is_some());
    }

    #[test]
    fn test_unless_negated() {
        let program = from_cedar(
            r#"forbid(principal, action, resource) unless { principal == resource.owner };"#,
        )
        .unwrap();
        if let Some(ExprAst::Not(_)) = &program.rules[0].body {
            // Good - unless is negated
        } else {
            panic!("Expected Not expression for unless");
        }
    }

    #[test]
    fn test_combined_scope_and_when() {
        let program = from_cedar(
            r#"permit(principal == User::"alice", action == Action::"view", resource)
               when { resource.public == true };"#,
        )
        .unwrap();
        if let Some(ExprAst::And(parts)) = &program.rules[0].body {
            assert_eq!(parts.len(), 3); // principal ==, action ==, resource.public ==
        } else {
            panic!("Expected And with 3 parts");
        }
    }

    #[test]
    fn test_annotation_as_name() {
        let program =
            from_cedar(r#"@id("admin_policy") permit(principal, action, resource);"#).unwrap();
        assert_eq!(program.rules[0].name, "admin_policy");
    }

    #[test]
    fn test_auto_name() {
        let program = from_cedar(r#"permit(principal, action, resource);"#).unwrap();
        assert_eq!(program.rules[0].name, "policy_0");
    }

    #[test]
    fn test_contains_to_in() {
        let program = from_cedar(
            r#"permit(principal, action, resource)
               when { resource.admins.contains(principal) };"#,
        )
        .unwrap();
        if let Some(ExprAst::In { pattern, path }) = &program.rules[0].body {
            assert!(matches!(pattern, PatternAst::PathRef(_)));
            assert_eq!(path_to_string(path), "resource.admins");
        } else {
            panic!("Expected In expression, got {:?}", program.rules[0].body);
        }
    }

    #[test]
    fn test_complex_when_and() {
        let program = from_cedar(
            r#"permit(principal, action, resource) 
               when { principal.department == "Engineering" && principal.level >= 5 };"#,
        )
        .unwrap();
        if let Some(ExprAst::And(parts)) = &program.rules[0].body {
            assert_eq!(parts.len(), 2);
        } else {
            panic!("Expected And expression");
        }
    }

    #[test]
    fn test_has_supported() {
        let result =
            from_cedar(r#"permit(principal, action, resource) when { context has readOnly };"#);
        assert!(
            result.is_ok(),
            "has should be supported: {:?}",
            result.err()
        );
        let program = result.unwrap();
        // Should compile and evaluate: has context.readOnly
        let policy = crate::compiler::compile_program(&program, &std::collections::HashSet::new())
            .expect("should compile");
        // readOnly present → true
        let effect = policy.evaluate(&serde_json::json!({"context": {"readOnly": true}}));
        assert_eq!(effect, crate::rule::Effect::Allow);
        // readOnly absent → false (deny)
        let effect = policy.evaluate(&serde_json::json!({"context": {}}));
        assert_eq!(effect, crate::rule::Effect::Deny);
    }

    #[test]
    fn test_like_supported() {
        let result =
            from_cedar(r#"permit(principal, action, resource) when { principal.name like "j*" };"#);
        assert!(
            result.is_ok(),
            "like should be supported: {:?}",
            result.err()
        );
        let program = result.unwrap();
        let policy = crate::compiler::compile_program(&program, &std::collections::HashSet::new())
            .expect("should compile");
        // "john" matches "j*"
        let effect = policy.evaluate(&serde_json::json!({"principal": {"name": "john"}}));
        assert_eq!(effect, crate::rule::Effect::Allow);
        // "alice" does not match "j*"
        let effect = policy.evaluate(&serde_json::json!({"principal": {"name": "alice"}}));
        assert_eq!(effect, crate::rule::Effect::Deny);
    }

    #[test]
    fn test_template_slot_unsupported() {
        let result = from_cedar(r#"permit(principal == ?principal, action, resource);"#);
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("Template slot"));
    }

    #[test]
    fn test_multiple_policies() {
        let program = from_cedar(
            r#"
            permit(principal == User::"alice", action, resource);
            forbid(principal, action, resource) when { resource.private == true };
            "#,
        )
        .unwrap();
        assert_eq!(program.rules.len(), 2);
        assert_eq!(program.rules[0].effect, EffectAst::Allow);
        assert_eq!(program.rules[1].effect, EffectAst::Deny);
    }

    #[test]
    fn test_to_source() {
        let source = from_cedar_to_source(
            r#"permit(principal == User::"alice", action == Action::"view", resource)
               when { resource.public == true };"#,
        )
        .unwrap();
        assert!(source.contains("allow"));
        assert!(source.contains("principal == \"alice\""));
        assert!(source.contains("action == \"view\""));
        assert!(source.contains("resource.public == true"));
    }

    #[test]
    fn test_compile_cedar_roundtrip() {
        // Verify the Cedar import produces valid Karu AST that compiles
        let program = from_cedar(
            r#"permit(principal, action == Action::"view", resource)
               when { resource.public == true };"#,
        )
        .unwrap();

        // Should be able to compile this AST
        let policy =
            crate::compiler::compile_program(&program, &std::collections::HashSet::new()).unwrap();
        let result = policy.evaluate(&serde_json::json!({
            "action": "view",
            "resource": {"public": true}
        }));
        assert_eq!(result, crate::rule::Effect::Allow);
    }

    #[test]
    fn test_contains_all() {
        let program = from_cedar(
            r#"permit(principal, action, resource)
               when { resource.tags.containsAll(["admin", "beta"]) };"#,
        )
        .unwrap();
        let policy =
            crate::compiler::compile_program(&program, &std::collections::HashSet::new()).unwrap();
        // Has both tags → allow
        let effect = policy.evaluate(&serde_json::json!({
            "resource": {"tags": ["admin", "beta", "prod"]}
        }));
        assert_eq!(effect, crate::rule::Effect::Allow);
        // Missing "beta" → deny
        let effect = policy.evaluate(&serde_json::json!({
            "resource": {"tags": ["admin", "prod"]}
        }));
        assert_eq!(effect, crate::rule::Effect::Deny);
    }

    #[test]
    fn test_contains_any() {
        let program = from_cedar(
            r#"permit(principal, action, resource)
               when { resource.tags.containsAny(["admin", "superuser"]) };"#,
        )
        .unwrap();
        let policy =
            crate::compiler::compile_program(&program, &std::collections::HashSet::new()).unwrap();
        // Has "admin" → allow
        let effect = policy.evaluate(&serde_json::json!({
            "resource": {"tags": ["admin", "viewer"]}
        }));
        assert_eq!(effect, crate::rule::Effect::Allow);
        // Has neither → deny
        let effect = policy.evaluate(&serde_json::json!({
            "resource": {"tags": ["viewer", "editor"]}
        }));
        assert_eq!(effect, crate::rule::Effect::Deny);
    }

    #[test]
    fn test_ip_is_in_range() {
        let program = from_cedar(
            r#"permit(principal, action, resource)
               when { ip(context.srcIp).isInRange(ip("10.0.0.0/8")) };"#,
        )
        .unwrap();
        let policy =
            crate::compiler::compile_program(&program, &std::collections::HashSet::new()).unwrap();
        // In range → allow
        let effect = policy.evaluate(&serde_json::json!({
            "context": {"srcIp": "10.20.30.40"}
        }));
        assert_eq!(effect, crate::rule::Effect::Allow);
        // Out of range → deny
        let effect = policy.evaluate(&serde_json::json!({
            "context": {"srcIp": "192.168.1.1"}
        }));
        assert_eq!(effect, crate::rule::Effect::Deny);
    }

    #[test]
    fn test_ip_is_ipv4() {
        let program = from_cedar(
            r#"permit(principal, action, resource)
               when { ip(context.addr).isIpv4() };"#,
        )
        .unwrap();
        let policy =
            crate::compiler::compile_program(&program, &std::collections::HashSet::new()).unwrap();
        let effect = policy.evaluate(&serde_json::json!({
            "context": {"addr": "192.168.1.1"}
        }));
        assert_eq!(effect, crate::rule::Effect::Allow);
        let effect = policy.evaluate(&serde_json::json!({
            "context": {"addr": "not-an-ip"}
        }));
        assert_eq!(effect, crate::rule::Effect::Deny);
    }

    #[test]
    fn test_decimal_less_than() {
        let program = from_cedar(
            r#"permit(principal, action, resource)
               when { decimal(context.score).lessThan(decimal("0.75")) };"#,
        )
        .unwrap();
        let policy =
            crate::compiler::compile_program(&program, &std::collections::HashSet::new()).unwrap();
        // 0.5 < 0.75 → allow
        let effect = policy.evaluate(&serde_json::json!({
            "context": {"score": "0.5"}
        }));
        assert_eq!(effect, crate::rule::Effect::Allow);
        // 0.9 < 0.75 → deny
        let effect = policy.evaluate(&serde_json::json!({
            "context": {"score": "0.9"}
        }));
        assert_eq!(effect, crate::rule::Effect::Deny);
    }

    #[test]
    fn test_if_then_else_basic() {
        // if principal == Admin then true else false → simplifies to principal == Admin
        let program = from_cedar(
            r#"permit(principal, action, resource)
               when { if principal == Admin::"admin" then true else false };"#,
        )
        .unwrap();
        let policy =
            crate::compiler::compile_program(&program, &std::collections::HashSet::new()).unwrap();
        let effect = policy.evaluate(&serde_json::json!({
            "principal": "admin"
        }));
        assert_eq!(effect, crate::rule::Effect::Allow);
        let effect = policy.evaluate(&serde_json::json!({
            "principal": "user"
        }));
        assert_eq!(effect, crate::rule::Effect::Deny);
    }

    #[test]
    fn test_if_then_else_inverted() {
        // if cond then false else true → Not(cond)
        let program = from_cedar(
            r#"permit(principal, action, resource)
               when { if context.blocked then false else true };"#,
        )
        .unwrap();
        let policy =
            crate::compiler::compile_program(&program, &std::collections::HashSet::new()).unwrap();
        // blocked = true → deny (Not(true) = false → condition fails)
        let effect = policy.evaluate(&serde_json::json!({
            "context": {"blocked": true}
        }));
        assert_eq!(effect, crate::rule::Effect::Deny);
    }

    #[test]
    fn test_cedar_arithmetic_constant_fold() {
        // Constant arithmetic should be folded at import time
        // This test just verifies the import succeeds (arithmetic is folded)
        let result = from_cedar(
            r#"permit(principal, action, resource)
               when { 2 + 3 == 5 };"#,
        );
        result.unwrap();
    }

    // ========================================================================
    // Tests for `is` type narrowing (previously unsupported)
    // ========================================================================

    #[test]
    fn test_is_type_principal_scope() {
        // `principal is User` in scope → principal.type == "User"
        let program = from_cedar(r#"permit(principal is User, action, resource);"#).unwrap();
        assert_eq!(program.rules.len(), 1);
        if let Some(ExprAst::Compare { left, op, right }) = &program.rules[0].body {
            assert_eq!(path_to_string(left), "principal.type");
            assert_eq!(*op, OpAst::Eq);
            assert!(
                matches!(right, PatternAst::Literal(serde_json::Value::String(s)) if s == "User")
            );
        } else {
            panic!(
                "Expected Compare on principal.type, got {:?}",
                program.rules[0].body
            );
        }
    }

    #[test]
    fn test_is_type_resource_scope() {
        // `resource is Document` in scope → resource.type == "Document"
        let program = from_cedar(r#"permit(principal, action, resource is Document);"#).unwrap();
        assert_eq!(program.rules.len(), 1);
        if let Some(ExprAst::Compare { left, op, right }) = &program.rules[0].body {
            assert_eq!(path_to_string(left), "resource.type");
            assert_eq!(*op, OpAst::Eq);
            assert!(
                matches!(right, PatternAst::Literal(serde_json::Value::String(s)) if s == "Document")
            );
        } else {
            panic!(
                "Expected Compare on resource.type, got {:?}",
                program.rules[0].body
            );
        }
    }

    #[test]
    fn test_is_type_principal_in_group() {
        // `principal is Admin in Group::"admins"` → type check AND group membership
        let program =
            from_cedar(r#"permit(principal is Admin in Group::"admins", action, resource);"#)
                .unwrap();
        if let Some(ExprAst::And(parts)) = &program.rules[0].body {
            assert_eq!(parts.len(), 2);
            // First part: principal.type == "Admin"
            if let ExprAst::Compare { left, op, right } = &parts[0] {
                assert_eq!(path_to_string(left), "principal.type");
                assert_eq!(*op, OpAst::Eq);
                assert!(
                    matches!(right, PatternAst::Literal(serde_json::Value::String(s)) if s == "Admin")
                );
            } else {
                panic!("Expected Compare for type check");
            }
            // Second part: "admins" in principal.groups
            if let ExprAst::In { pattern, path } = &parts[1] {
                assert!(
                    matches!(pattern, PatternAst::Literal(serde_json::Value::String(s)) if s == "admins")
                );
                assert_eq!(path_to_string(path), "principal.groups");
            } else {
                panic!("Expected In for group check");
            }
        } else {
            panic!("Expected And expression, got {:?}", program.rules[0].body);
        }
    }

    #[test]
    fn test_is_type_in_when_clause() {
        // `principal is User` in when clause → principal.type == "User"
        let program =
            from_cedar(r#"permit(principal, action, resource) when { principal is User };"#)
                .unwrap();
        if let Some(ExprAst::Compare { left, op, right }) = &program.rules[0].body {
            assert_eq!(path_to_string(left), "principal.type");
            assert_eq!(*op, OpAst::Eq);
            assert!(
                matches!(right, PatternAst::Literal(serde_json::Value::String(s)) if s == "User")
            );
        } else {
            panic!("Expected Compare on principal.type");
        }
    }

    #[test]
    fn test_is_type_evaluates_correctly() {
        // Full evaluation: `principal is Admin` should check principal.type field
        let program = from_cedar(r#"permit(principal is Admin, action, resource);"#).unwrap();
        let policy =
            crate::compiler::compile_program(&program, &std::collections::HashSet::new()).unwrap();

        // principal.type == "Admin" → allow
        let effect = policy.evaluate(&serde_json::json!({
            "principal": {"type": "Admin", "id": "alice"}
        }));
        assert_eq!(effect, crate::rule::Effect::Allow);

        // principal.type == "User" → deny
        let effect = policy.evaluate(&serde_json::json!({
            "principal": {"type": "User", "id": "bob"}
        }));
        assert_eq!(effect, crate::rule::Effect::Deny);

        // principal.type missing → deny
        let effect = policy.evaluate(&serde_json::json!({
            "principal": {"id": "charlie"}
        }));
        assert_eq!(effect, crate::rule::Effect::Deny);
    }

    #[test]
    fn test_is_type_in_group_evaluates_correctly() {
        // Full evaluation: `principal is Admin in Group::"superusers"`
        let program =
            from_cedar(r#"permit(principal is Admin in Group::"superusers", action, resource);"#)
                .unwrap();
        let policy =
            crate::compiler::compile_program(&program, &std::collections::HashSet::new()).unwrap();

        // type "Admin" AND group "superusers" → allow
        let effect = policy.evaluate(&serde_json::json!({
            "principal": {"type": "Admin", "groups": ["superusers", "staff"]}
        }));
        assert_eq!(effect, crate::rule::Effect::Allow);

        // type "Admin" but wrong group → deny
        let effect = policy.evaluate(&serde_json::json!({
            "principal": {"type": "Admin", "groups": ["staff"]}
        }));
        assert_eq!(effect, crate::rule::Effect::Deny);

        // right group but wrong type → deny
        let effect = policy.evaluate(&serde_json::json!({
            "principal": {"type": "User", "groups": ["superusers"]}
        }));
        assert_eq!(effect, crate::rule::Effect::Deny);
    }
}
