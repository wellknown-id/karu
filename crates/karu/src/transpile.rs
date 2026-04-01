// SPDX-License-Identifier: MIT

//! Transpile Karu policies to Cedar syntax.
//!
//! This module converts Karu AST to Cedar policy language for interoperability
//! with Cedar-based authorization systems.
//!
//! # Limitations
//!
//! Not all Karu features have Cedar equivalents:
//! - `forall` expressions are not supported
//! - Variable bindings are not supported
//! - OR conditions emit multiple Cedar policies
//!
//! # Example
//!
//! ```
//! use karu::transpile::to_cedar;
//! use karu::parser::Parser;
//!
//! let policy = r#"
//!     allow view if
//!         principal == "alice" and
//!         action == "view";
//! "#;
//!
//! let ast = Parser::parse(policy).unwrap();
//! let cedar = to_cedar(&ast).unwrap();
//! assert!(cedar.contains("permit"));
//! ```

use crate::ast::*;
use std::fmt::Write;

/// Error during transpilation to Cedar.
#[derive(Debug, Clone, PartialEq)]
pub struct TranspileError {
    pub message: String,
}

impl std::fmt::Display for TranspileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TranspileError: {}", self.message)
    }
}

impl std::error::Error for TranspileError {}

/// Transpile a Karu program to Cedar syntax.
pub fn to_cedar(program: &Program) -> Result<String, TranspileError> {
    let mut output = String::new();

    for (i, rule) in program.rules.iter().enumerate() {
        if i > 0 {
            output.push_str("\n\n");
        }
        let cedar_rule = rule_to_cedar(rule)?;
        output.push_str(&cedar_rule);
    }

    Ok(output)
}

/// Transpile a single Karu rule to Cedar.
pub fn rule_to_cedar(rule: &RuleAst) -> Result<String, TranspileError> {
    let mut output = String::new();

    // Effect: allow -> permit, deny -> forbid
    let effect = match rule.effect {
        EffectAst::Allow => "permit",
        EffectAst::Deny => "forbid",
    };

    // Extract principal/action/resource conditions and remaining when clause
    let (principal, action, resource, context, when_conditions) = if let Some(body) = &rule.body {
        extract_par_conditions(body)?
    } else {
        (None, None, None, None, Vec::new())
    };

    // Build the policy header
    writeln!(output, "// Karu rule: {}", rule.name).unwrap();
    write!(output, "{}(\n  ", effect).unwrap();

    // Principal clause
    if let Some(p) = principal {
        write!(output, "principal == User::{}", format_cedar_value(&p)).unwrap();
    } else {
        write!(output, "principal").unwrap();
    }
    output.push_str(",\n  ");

    // Action clause
    if let Some(a) = action {
        write!(output, "action == Action::{}", format_cedar_value(&a)).unwrap();
    } else {
        write!(output, "action").unwrap();
    }
    output.push_str(",\n  ");

    // Resource clause
    if let Some(r) = resource {
        write!(output, "resource == Resource::{}", format_cedar_value(&r)).unwrap();
    } else {
        write!(output, "resource").unwrap();
    }

    output.push_str("\n)");

    // Context clause (Cedar supports context in when block, not header)
    let has_context = context.is_some();
    if let Some(c) = context {
        output.push_str("\nwhen {\n");
        write!(output, "  context == {}", format_cedar_value(&c)).unwrap();
        if !when_conditions.is_empty() {
            output.push_str(" &&\n");
        } else {
            output.push('\n');
        }
    } else if !when_conditions.is_empty() {
        output.push_str("\nwhen {\n");
    }

    // Remaining when conditions
    for (i, cond) in when_conditions.iter().enumerate() {
        if i > 0 {
            output.push_str(" &&\n");
        }
        output.push_str("  ");
        output.push_str(&expr_to_cedar(cond)?);
        if i == when_conditions.len() - 1 {
            output.push('\n');
        }
    }

    if has_context || !when_conditions.is_empty() {
        output.push('}');
    }

    output.push(';');

    Ok(output)
}

/// Extract principal, action, resource, context equality conditions from the body.
/// Returns (principal_value, action_value, resource_value, context_value, remaining_conditions).
#[allow(clippy::type_complexity)]
fn extract_par_conditions(
    expr: &ExprAst,
) -> Result<
    (
        Option<serde_json::Value>,
        Option<serde_json::Value>,
        Option<serde_json::Value>,
        Option<serde_json::Value>,
        Vec<ExprAst>,
    ),
    TranspileError,
> {
    let mut principal = None;
    let mut action = None;
    let mut resource = None;
    let mut context = None;
    let mut remaining = Vec::new();

    // Flatten AND expressions
    let conditions = flatten_and(expr);

    for cond in conditions {
        match cond {
            ExprAst::Compare {
                left,
                op: OpAst::Eq,
                right: PatternAst::Literal(val),
            } => {
                if left.segments.len() == 1 {
                    if let PathSegmentAst::Field(name) = &left.segments[0] {
                        match name.as_str() {
                            "principal" => {
                                principal = Some(val.clone());
                                continue;
                            }
                            "action" => {
                                action = Some(val.clone());
                                continue;
                            }
                            "resource" => {
                                resource = Some(val.clone());
                                continue;
                            }
                            "context" => {
                                context = Some(val.clone());
                                continue;
                            }
                            _ => {}
                        }
                    }
                }
                remaining.push(cond.clone());
            }
            _ => remaining.push(cond.clone()),
        }
    }

    Ok((principal, action, resource, context, remaining))
}

/// Flatten nested AND expressions into a list.
fn flatten_and(expr: &ExprAst) -> Vec<&ExprAst> {
    match expr {
        ExprAst::And(exprs) => exprs.iter().flat_map(flatten_and).collect(),
        _ => vec![expr],
    }
}

/// Convert an expression to Cedar syntax.
fn expr_to_cedar(expr: &ExprAst) -> Result<String, TranspileError> {
    match expr {
        ExprAst::And(exprs) => {
            let parts: Result<Vec<_>, _> = exprs.iter().map(expr_to_cedar).collect();
            Ok(parts?.join(" && "))
        }

        ExprAst::Or(exprs) => {
            let parts: Result<Vec<_>, _> = exprs.iter().map(expr_to_cedar).collect();
            Ok(format!("({})", parts?.join(" || ")))
        }

        ExprAst::Not(inner) => Ok(format!("!({})", expr_to_cedar(inner)?)),

        ExprAst::Compare { left, op, right } => {
            let left_str = path_to_cedar(left);
            let op_str = op_to_cedar(op);
            let right_str = pattern_to_cedar(right)?;
            Ok(format!("{} {} {}", left_str, op_str, right_str))
        }

        ExprAst::In { pattern, path } => {
            // Cedar uses .contains() for membership
            let path_str = path_to_cedar(path);
            let pattern_str = pattern_to_cedar(pattern)?;
            Ok(format!("{}.contains({})", path_str, pattern_str))
        }

        ExprAst::InLiteral { path, values } => {
            // path in ["a", "b"] → [\"a\", \"b\"].contains(path)
            let path_str = path_to_cedar(path);
            let parts: Result<Vec<_>, _> = values.iter().map(pattern_to_cedar).collect();
            Ok(format!("[{}].contains({})", parts?.join(", "), path_str))
        }

        ExprAst::Has { path } => {
            // has context.readOnly → context has readOnly
            // Split path: all but last segment is the base, last segment is the field
            let segments = &path.segments;
            if segments.len() >= 2 {
                let base = PathAst {
                    segments: segments[..segments.len() - 1].to_vec(),
                };
                let field = match &segments[segments.len() - 1] {
                    PathSegmentAst::Field(name) => name.clone(),
                    PathSegmentAst::Index(idx) => idx.to_string(),
                    PathSegmentAst::Variable(v) => v.clone(),
                };
                Ok(format!("{} has {}", path_to_cedar(&base), field))
            } else {
                // Single-segment path: just emit as-is
                Ok(format!("{} has \"\"", path_to_cedar(path)))
            }
        }

        ExprAst::Like { path, pattern } => {
            Ok(format!("{} like \"{}\"", path_to_cedar(path), pattern))
        }

        ExprAst::Forall { .. } => Err(TranspileError {
            message: "forall expressions are not supported in Cedar".into(),
        }),

        ExprAst::Exists { .. } => Err(TranspileError {
            message: "exists expressions are not supported in Cedar".into(),
        }),

        ExprAst::TypeRef { namespace, name } => {
            // Namespaced type ref → Cedar action scope
            match namespace {
                Some(ns) => Ok(format!("action == {}::\"{}\"", ns, name)),
                None => Ok(format!("action == \"{}\"", name)),
            }
        }

        ExprAst::IsType { path, type_name } => {
            // resource is File → resource is File
            Ok(format!("{} is {}", path_to_cedar(path), type_name))
        }
    }
}

/// Convert a path to Cedar syntax.
fn path_to_cedar(path: &PathAst) -> String {
    path.segments
        .iter()
        .map(|seg| match seg {
            PathSegmentAst::Field(name) => name.clone(),
            PathSegmentAst::Index(idx) => format!("[{}]", idx),
            PathSegmentAst::Variable(var) => format!("[{}]", var),
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// Convert an operator to Cedar syntax.
fn op_to_cedar(op: &OpAst) -> &'static str {
    match op {
        OpAst::Eq => "==",
        OpAst::Ne => "!=",
        OpAst::Lt => "<",
        OpAst::Gt => ">",
        OpAst::Le => "<=",
        OpAst::Ge => ">=",
        // These use method call syntax in Cedar, shouldn't appear in Compare
        OpAst::ContainsAll => ".containsAll",
        OpAst::ContainsAny => ".containsAny",
        OpAst::IpIsInRange => ".isInRange",
        OpAst::IsIpv4 => ".isIpv4",
        OpAst::IsIpv6 => ".isIpv6",
        OpAst::IsLoopback => ".isLoopback",
        OpAst::IsMulticast => ".isMulticast",
        OpAst::DecimalLt => ".lessThan",
        OpAst::DecimalLe => ".lessThanOrEqual",
        OpAst::DecimalGt => ".greaterThan",
        OpAst::DecimalGe => ".greaterThanOrEqual",
    }
}

/// Convert a pattern to Cedar syntax.
fn pattern_to_cedar(pattern: &PatternAst) -> Result<String, TranspileError> {
    match pattern {
        PatternAst::Literal(val) => Ok(format_cedar_value(val)),

        PatternAst::Variable(_) => Err(TranspileError {
            message: "Variable bindings are not supported in Cedar".into(),
        }),

        PatternAst::Wildcard => Err(TranspileError {
            message: "Wildcards are not supported in Cedar patterns".into(),
        }),

        PatternAst::Object(_) => Err(TranspileError {
            message: "Object patterns are not supported in Cedar".into(),
        }),

        PatternAst::Array(_) => Err(TranspileError {
            message: "Array patterns are not supported in Cedar".into(),
        }),

        PatternAst::PathRef(path) => Ok(path_to_cedar(path)),
    }
}

/// Format a JSON value as Cedar syntax.
fn format_cedar_value(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::String(s) => format!("\"{}\"", s),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Array(arr) => {
            let items: Vec<_> = arr.iter().map(format_cedar_value).collect();
            format!("[{}]", items.join(", "))
        }
        serde_json::Value::Object(_) => "{}".to_string(), // Simplified
    }
}

// ============================================================================
// Cedar Schema emission
// ============================================================================

use crate::schema::*;

/// Transpile Karu modules to Cedar schema syntax (`.cedarschema`).
///
/// Converts `ModuleDef` AST (entities, actions, type declarations) to the
/// Cedar human-readable schema format.
pub fn to_cedarschema(modules: &[ModuleDef]) -> String {
    let mut out = String::new();

    for (i, module) in modules.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if let Some(ref name) = module.name {
            writeln!(out, "namespace {} {{", name).unwrap();
            emit_module_body(&mut out, module, "  ");
            out.push_str("}\n");
        } else {
            emit_module_body(&mut out, module, "");
        }
    }

    out
}

fn emit_module_body(out: &mut String, module: &ModuleDef, indent: &str) {
    // Emit entities
    for entity in &module.entities {
        emit_entity(out, entity, indent);
    }
    // Emit type declarations (abstracts)
    for abstract_def in &module.abstracts {
        emit_type_decl(out, abstract_def, indent);
    }
    // Blank line between entities and actions if both exist
    if !module.entities.is_empty() && !module.actions.is_empty() {
        out.push('\n');
    }
    // Emit actions
    for action in &module.actions {
        emit_action(out, action, indent);
    }
}

fn emit_entity(out: &mut String, entity: &EntityDef, indent: &str) {
    write!(out, "{}entity {}", indent, entity.name).unwrap();

    // Parents
    if !entity.parents.is_empty() {
        write!(out, " in [{}]", entity.parents.join(", ")).unwrap();
    }

    // Fields
    if entity.fields.is_empty() {
        out.push_str(";\n");
    } else {
        out.push_str(" = {\n");
        emit_fields(out, &entity.fields, indent);
        writeln!(out, "{}}};", indent).unwrap();
    }
}

fn emit_type_decl(out: &mut String, abstract_def: &AbstractDef, indent: &str) {
    write!(out, "{}type {} = ", indent, abstract_def.name).unwrap();
    if abstract_def.fields.is_empty() {
        out.push_str("{};\n");
    } else {
        out.push_str("{\n");
        emit_fields(out, &abstract_def.fields, indent);
        writeln!(out, "{}}};", indent).unwrap();
    }
}

fn emit_action(out: &mut String, action: &ActionDef, indent: &str) {
    write!(out, "{}action \"{}\"", indent, action.name).unwrap();

    if let Some(ref at) = action.applies_to {
        out.push_str(" appliesTo {\n");
        let inner = format!("{}  ", indent);

        if !at.actors.is_empty() {
            writeln!(out, "{}principal: [{}],", inner, at.actors.join(", ")).unwrap();
        }
        if !at.resources.is_empty() {
            writeln!(out, "{}resource: [{}],", inner, at.resources.join(", ")).unwrap();
        }
        if let Some(ref ctx) = at.context {
            writeln!(out, "{}context: {{", inner).unwrap();
            emit_fields(out, ctx, &inner);
            writeln!(out, "{}}},", inner).unwrap();
        }

        write!(out, "{}}}", indent).unwrap();
    }

    out.push_str(";\n");
}

fn emit_fields(out: &mut String, fields: &[FieldDef], indent: &str) {
    let inner = format!("{}  ", indent);
    for field in fields {
        if field.optional {
            writeln!(
                out,
                "{}\"{}\"?: {},",
                inner,
                field.name,
                type_ref_to_cedar(&field.ty, &inner)
            )
            .unwrap();
        } else {
            writeln!(
                out,
                "{}\"{}\": {},",
                inner,
                field.name,
                type_ref_to_cedar(&field.ty, &inner)
            )
            .unwrap();
        }
    }
}

fn type_ref_to_cedar(ty: &TypeRef, indent: &str) -> String {
    match ty {
        TypeRef::Named(name) => name.clone(),
        TypeRef::Set(inner) => format!("Set<{}>", type_ref_to_cedar(inner, indent)),
        TypeRef::Record(fields) => {
            if fields.is_empty() {
                "{}".to_string()
            } else {
                let inner_indent = format!("{}    ", indent);
                let mut s = String::from("{\n");
                for field in fields {
                    if field.optional {
                        writeln!(
                            s,
                            "{}\"{}\"?: {},",
                            inner_indent,
                            field.name,
                            type_ref_to_cedar(&field.ty, &inner_indent)
                        )
                        .unwrap();
                    } else {
                        writeln!(
                            s,
                            "{}\"{}\": {},",
                            inner_indent,
                            field.name,
                            type_ref_to_cedar(&field.ty, &inner_indent)
                        )
                        .unwrap();
                    }
                }
                s.push_str(indent);
                s.push('}');
                s
            }
        }
        TypeRef::Union(variants) => variants
            .iter()
            .map(|v| type_ref_to_cedar(v, indent))
            .collect::<Vec<_>>()
            .join(" | "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    fn parse(source: &str) -> Program {
        Parser::parse(source).unwrap()
    }

    #[test]
    fn test_simple_allow() {
        let policy = r#"
            allow view if
                principal == "alice" and
                action == "view";
        "#;

        let ast = parse(policy);
        let cedar = to_cedar(&ast).unwrap();

        assert!(cedar.contains("permit"));
        assert!(cedar.contains("principal == User::\"alice\""));
        assert!(cedar.contains("action == Action::\"view\""));
    }

    #[test]
    fn test_deny_rule() {
        let policy = r#"
            deny block if action == "delete";
        "#;

        let ast = parse(policy);
        let cedar = to_cedar(&ast).unwrap();

        assert!(cedar.contains("forbid"));
        assert!(cedar.contains("action == Action::\"delete\""));
    }

    #[test]
    fn test_with_when_clause() {
        let policy = r#"
            allow read if
                principal == "alice" and
                resource.public == true;
        "#;

        let ast = parse(policy);
        let cedar = to_cedar(&ast).unwrap();

        assert!(cedar.contains("permit"));
        assert!(cedar.contains("when {"));
        assert!(cedar.contains("resource.public == true"));
    }

    #[test]
    fn test_membership() {
        let policy = r#"
            allow admin if "admin" in principal.roles;
        "#;

        let ast = parse(policy);
        let cedar = to_cedar(&ast).unwrap();

        assert!(cedar.contains("principal.roles.contains(\"admin\")"));
    }

    #[test]
    fn test_not_expression() {
        let policy = r#"
            deny block if not resource.public == true;
        "#;

        let ast = parse(policy);
        let cedar = to_cedar(&ast).unwrap();

        assert!(cedar.contains("!(resource.public == true)"));
    }

    #[test]
    fn test_path_comparison() {
        let policy = r#"
            allow owner if principal.id == resource.ownerId;
        "#;

        let ast = parse(policy);
        let cedar = to_cedar(&ast).unwrap();

        assert!(cedar.contains("principal.id == resource.ownerId"));
    }

    #[test]
    fn test_forall_not_supported() {
        let policy = r#"
            allow all if forall x in items: x.valid == true;
        "#;

        let ast = parse(policy);
        let result = to_cedar(&ast);

        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("forall"));
    }

    #[test]
    fn test_multiple_rules() {
        let policy = r#"
            allow view;
            deny delete if principal == "guest";
        "#;

        let ast = parse(policy);
        let cedar = to_cedar(&ast).unwrap();

        assert!(cedar.contains("permit"));
        assert!(cedar.contains("forbid"));
    }

    #[test]
    fn test_context_extraction() {
        let policy = r#"
            allow access if
                principal == "alice" and
                context == "web";
        "#;

        let ast = parse(policy);
        let cedar = to_cedar(&ast).unwrap();

        assert!(cedar.contains("permit"));
        assert!(cedar.contains("principal == User::\"alice\""));
        assert!(cedar.contains("context == \"web\""));
        assert!(cedar.contains("when {"));
    }
}
