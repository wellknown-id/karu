// SPDX-License-Identifier: MIT

//! Compiler from AST to executable Policy.
//!
//! Converts parsed AST into runtime [`Policy`] structures that can be evaluated.
//!
//! # Example
//!
//! ```rust
//! use karu::compile;
//! use karu::rule::Effect;
//! use serde_json::json;
//!
//! let policy = compile(r#"
//!     allow access if role == "admin";
//!     deny blocked if banned == true;
//! "#).unwrap();
//!
//! assert_eq!(policy.evaluate(&json!({"role": "admin", "banned": false})), Effect::Allow);
//! assert_eq!(policy.evaluate(&json!({"role": "admin", "banned": true})), Effect::Deny);
//! ```

use crate::ast::*;
use crate::parser::{ParseError, Parser};
use crate::path::{Path, PathSegment};
use crate::pattern::Pattern;
use crate::rule::{
    Condition, ConditionExpr, Effect, Operator, Policy, QuantifierInfo, QuantifierMode, Rule,
};
use crate::type_registry::TypeRegistry;
use std::collections::{HashMap, HashSet};

/// Type alias for the assertion registry: maps assertion name → body expression.
type AssertionRegistry<'a> = HashMap<String, &'a ExprAst>;

/// Compile source code directly into an executable Policy.
///
/// This is the primary entry point for compiling Karu policy source code.
///
/// # Errors
///
/// Returns a [`ParseError`] if the source code is invalid.
///
/// # Example
///
/// ```rust
/// use karu::compile;
/// use karu::rule::Effect;
/// use serde_json::json;
///
/// let policy = compile(r#"allow read if action == "read";"#).unwrap();
/// assert_eq!(policy.evaluate(&json!({"action": "read"})), Effect::Allow);
/// ```
pub fn compile(source: &str) -> Result<Policy, ParseError> {
    let ast = Parser::parse(source)?;
    compile_program(&ast, &HashSet::new())
}

/// Compile source code with host-provided assert names.
///
/// Any bare identifier in a rule condition that matches a name in `host_asserts`
/// will be compiled as a `ConditionExpr::HostAssert` rather than falling through
/// to the default TypeRef handling.
pub fn compile_with_host_asserts(
    source: &str,
    host_asserts: &HashSet<String>,
) -> Result<Policy, ParseError> {
    let ast = Parser::parse(source)?;
    compile_program(&ast, host_asserts)
}

/// Compile a parsed program into a Policy.
pub fn compile_program(
    program: &Program,
    host_asserts: &HashSet<String>,
) -> Result<Policy, ParseError> {
    // Build assertion registry from program assertions
    let assertions: AssertionRegistry = program
        .assertions
        .iter()
        .map(|a| (a.name.clone(), &a.body))
        .collect();

    // Build type registry from schema modules
    let type_registry = TypeRegistry::from_modules(&program.modules);

    let mut policy = Policy::new();

    for rule_ast in &program.rules {
        policy.add_rule(compile_rule(
            rule_ast,
            &assertions,
            &type_registry,
            host_asserts,
        )?);
    }

    Ok(policy)
}

fn compile_rule(
    rule: &RuleAst,
    assertions: &AssertionRegistry,
    types: &TypeRegistry,
    host_asserts: &HashSet<String>,
) -> Result<Rule, ParseError> {
    let effect = match rule.effect {
        EffectAst::Allow => Effect::Allow,
        EffectAst::Deny => Effect::Deny,
    };

    let body = if let Some(ref body) = rule.body {
        let mut expanding = HashSet::new();
        Some(compile_expr_inner(
            body,
            assertions,
            types,
            host_asserts,
            &mut expanding,
        )?)
    } else {
        None
    };

    Ok(Rule::with_body(&rule.name, effect, body))
}

fn compile_expr_inner(
    expr: &ExprAst,
    assertions: &AssertionRegistry,
    types: &TypeRegistry,
    host_asserts: &HashSet<String>,
    expanding: &mut HashSet<String>,
) -> Result<ConditionExpr, ParseError> {
    match expr {
        ExprAst::And(exprs) => {
            let compiled: Vec<ConditionExpr> = exprs
                .iter()
                .map(|e| compile_expr_inner(e, assertions, types, host_asserts, expanding))
                .collect::<Result<_, _>>()?;
            Ok(match compiled.len() {
                1 => compiled.into_iter().next().unwrap(),
                _ => ConditionExpr::And(compiled),
            })
        }
        ExprAst::Or(exprs) => {
            let compiled: Vec<ConditionExpr> = exprs
                .iter()
                .map(|e| compile_expr_inner(e, assertions, types, host_asserts, expanding))
                .collect::<Result<_, _>>()?;
            Ok(match compiled.len() {
                1 => compiled.into_iter().next().unwrap(),
                _ => ConditionExpr::Or(compiled),
            })
        }
        ExprAst::Compare { left, op, right } => {
            // Check if this is an assertion reference: `assert_name == true`
            // (Parser generates this for standalone identifiers like `user_is_owner`)
            if matches!(op, OpAst::Eq)
                && matches!(right, PatternAst::Literal(v) if v == &serde_json::json!(true))
                && left.segments.len() == 1
            {
                if let PathSegmentAst::Field(name) = &left.segments[0] {
                    if let Some(assertion_body) = assertions.get(name) {
                        // Cycle detection: check if we're already expanding this assertion
                        if !expanding.insert(name.clone()) {
                            return Err(ParseError {
                                message: format!(
                                    "circular assertion reference: `{}` references itself",
                                    name
                                ),
                                line: 0,
                                column: 0,
                                token: None,
                            });
                        }
                        // Inline the assertion's body expression
                        let result = compile_expr_inner(
                            assertion_body,
                            assertions,
                            types,
                            host_asserts,
                            expanding,
                        );
                        expanding.remove(name);
                        return result;
                    }
                    // Check host-registered asserts (bare identifier like `resource_is_package_local`)
                    if host_asserts.contains(name) {
                        return Ok(ConditionExpr::HostAssert(name.clone()));
                    }
                }
            }

            let path = compile_path(left);
            let operator = compile_op(op);
            let pattern = compile_pattern(right);
            Ok(ConditionExpr::Leaf(Condition::new(path, operator, pattern)))
        }
        ExprAst::In { pattern, path } => {
            let compiled_path = compile_path(path);
            let compiled_pattern = compile_pattern(pattern);
            Ok(ConditionExpr::Leaf(Condition::new(
                compiled_path,
                Operator::In,
                compiled_pattern,
            )))
        }
        ExprAst::InLiteral { path, values } => {
            let path = compile_path(path);
            let arr: Vec<serde_json::Value> = values
                .iter()
                .map(|v| match compile_pattern(v) {
                    Pattern::Literal(val) => val,
                    _ => serde_json::Value::Null,
                })
                .collect();
            Ok(ConditionExpr::Leaf(Condition::new(
                path,
                Operator::In,
                Pattern::Literal(serde_json::Value::Array(arr)),
            )))
        }
        ExprAst::Has { path } => {
            let path = compile_path(path);
            Ok(ConditionExpr::Leaf(Condition::new(
                path,
                Operator::Has,
                Pattern::Variable(None),
            )))
        }
        ExprAst::Like { path, pattern } => {
            let compiled_path = compile_path(path);
            Ok(ConditionExpr::Leaf(Condition::new(
                compiled_path,
                Operator::Like,
                Pattern::Literal(serde_json::Value::String(pattern.clone())),
            )))
        }
        ExprAst::Not(inner) => {
            let compiled = compile_expr_inner(inner, assertions, types, host_asserts, expanding)?;
            Ok(ConditionExpr::Not(Box::new(compiled)))
        }
        ExprAst::Forall { var, path, body } => {
            let body_expr = compile_expr_inner(body, assertions, types, host_asserts, expanding)?;
            let source_path = compile_path(path);
            Ok(ConditionExpr::Leaf(Condition {
                path: Path::root(),
                op: Operator::ForAll,
                pattern: Pattern::Variable(None),
                quantifier: Some(QuantifierInfo {
                    mode: QuantifierMode::ForAll,
                    var: var.clone(),
                    source_path,
                    body: Box::new(body_expr),
                }),
            }))
        }
        ExprAst::Exists { var, path, body } => {
            let body_expr = compile_expr_inner(body, assertions, types, host_asserts, expanding)?;
            let source_path = compile_path(path);
            Ok(ConditionExpr::Leaf(Condition {
                path: Path::root(),
                op: Operator::Exists,
                pattern: Pattern::Variable(None),
                quantifier: Some(QuantifierInfo {
                    mode: QuantifierMode::Exists,
                    var: var.clone(),
                    source_path,
                    body: Box::new(body_expr),
                }),
            }))
        }
        ExprAst::TypeRef { namespace: _, name } => {
            // Check host-registered asserts first
            if host_asserts.contains(name) {
                return Ok(ConditionExpr::HostAssert(name.clone()));
            }
            // Type references like `MyCedarNamespace:Delete` compile to action == "Delete"
            let path = compile_path(&PathAst {
                segments: vec![PathSegmentAst::Field("action".to_string())],
            });
            Ok(ConditionExpr::Leaf(Condition::new(
                path,
                Operator::Eq,
                Pattern::Literal(serde_json::json!(name)),
            )))
        }
        ExprAst::IsType { path, type_name } => {
            // Structural type check: `resource is File`
            // Look up the type shape in the registry - error if not found.
            let shape = types.get(type_name).cloned().ok_or_else(|| ParseError {
                message: format!("unknown type `{}` in `is` expression", type_name),
                line: 0,
                column: 0,
                token: None,
            })?;
            let compiled_path = compile_path(path);
            Ok(ConditionExpr::IsType {
                path: compiled_path,
                shape,
            })
        }
    }
}

fn compile_path(path: &PathAst) -> Path {
    let segments: Vec<PathSegment> = path
        .segments
        .iter()
        .map(|s| match s {
            PathSegmentAst::Field(name) => PathSegment::Field(name.clone()),
            PathSegmentAst::Index(idx) => PathSegment::Index(*idx),
            PathSegmentAst::Variable(var) => PathSegment::Variable(var.clone()),
        })
        .collect();
    Path::from_segments(segments)
}

fn compile_op(op: &OpAst) -> Operator {
    match op {
        OpAst::Eq => Operator::Eq,
        OpAst::Ne => Operator::Ne,
        OpAst::Lt => Operator::Lt,
        OpAst::Gt => Operator::Gt,
        OpAst::Le => Operator::Le,
        OpAst::Ge => Operator::Ge,
        OpAst::ContainsAll => Operator::ContainsAll,
        OpAst::ContainsAny => Operator::ContainsAny,
        OpAst::IpIsInRange => Operator::IpIsInRange,
        OpAst::IsIpv4 => Operator::IsIpv4,
        OpAst::IsIpv6 => Operator::IsIpv6,
        OpAst::IsLoopback => Operator::IsLoopback,
        OpAst::IsMulticast => Operator::IsMulticast,
        OpAst::DecimalLt => Operator::DecimalLt,
        OpAst::DecimalLe => Operator::DecimalLe,
        OpAst::DecimalGt => Operator::DecimalGt,
        OpAst::DecimalGe => Operator::DecimalGe,
    }
}

fn compile_pattern(pattern: &PatternAst) -> Pattern {
    match pattern {
        PatternAst::Literal(v) => Pattern::Literal(v.clone()),
        PatternAst::Variable(name) => Pattern::Variable(Some(name.clone())),
        PatternAst::Wildcard => Pattern::Variable(None),
        PatternAst::Object(fields) => {
            let pairs: Vec<(String, Pattern)> = fields
                .iter()
                .map(|(k, v)| (k.clone(), compile_pattern(v)))
                .collect();
            Pattern::Object(pairs.into_iter().collect())
        }
        PatternAst::Array(elements) => {
            Pattern::Array(elements.iter().map(compile_pattern).collect())
        }
        PatternAst::PathRef(path_ast) => Pattern::PathRef(compile_path(path_ast)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_compile_simple_rule() {
        let policy = compile("allow access;").unwrap();
        assert_eq!(policy.rules.len(), 1);
        // Empty conditions = always matches
        assert_eq!(policy.evaluate(&json!({})), Effect::Allow);
    }

    #[test]
    fn test_compile_with_condition() {
        let policy = compile(r#"allow read if action == "read";"#).unwrap();
        assert_eq!(policy.evaluate(&json!({"action": "read"})), Effect::Allow);
        assert_eq!(policy.evaluate(&json!({"action": "write"})), Effect::Deny);
    }

    #[test]
    fn test_compile_and_conditions() {
        let policy = compile(r#"allow admin if role == "admin" and active == true;"#).unwrap();
        assert_eq!(
            policy.evaluate(&json!({"role": "admin", "active": true})),
            Effect::Allow
        );
        assert_eq!(
            policy.evaluate(&json!({"role": "admin", "active": false})),
            Effect::Deny
        );
        assert_eq!(
            policy.evaluate(&json!({"role": "user", "active": true})),
            Effect::Deny
        );
    }

    #[test]
    fn test_compile_in_expression() {
        let policy = compile(r#"allow access if {name: "admin"} in roles;"#).unwrap();
        assert_eq!(
            policy.evaluate(&json!({"roles": [{"name": "user"}, {"name": "admin"}]})),
            Effect::Allow
        );
        assert_eq!(
            policy.evaluate(&json!({"roles": [{"name": "user"}]})),
            Effect::Deny
        );
    }

    #[test]
    fn test_compile_path_in_path() {
        // Path-in-path membership: principal.id in resource.adminIds
        let policy = compile(r#"allow admin if principal.id in resource.adminIds;"#).unwrap();
        assert_eq!(
            policy.evaluate(&json!({
                "principal": {"id": "alice"},
                "resource": {"adminIds": ["bob", "alice", "charlie"]}
            })),
            Effect::Allow
        );
        assert_eq!(
            policy.evaluate(&json!({
                "principal": {"id": "eve"},
                "resource": {"adminIds": ["bob", "alice", "charlie"]}
            })),
            Effect::Deny
        );
    }

    #[test]
    fn test_compile_comparison_operators() {
        let policy = compile(r#"allow adult if age >= 18;"#).unwrap();
        assert_eq!(policy.evaluate(&json!({"age": 18})), Effect::Allow);
        assert_eq!(policy.evaluate(&json!({"age": 21})), Effect::Allow);
        assert_eq!(policy.evaluate(&json!({"age": 17})), Effect::Deny);
    }

    #[test]
    fn test_compile_nested_path() {
        let policy = compile(r#"allow access if user.profile.verified == true;"#).unwrap();
        assert_eq!(
            policy.evaluate(&json!({"user": {"profile": {"verified": true}}})),
            Effect::Allow
        );
        assert_eq!(
            policy.evaluate(&json!({"user": {"profile": {"verified": false}}})),
            Effect::Deny
        );
    }

    #[test]
    fn test_compile_deny_overrides() {
        let policy = compile(
            r#"
            allow access if role == "admin";
            deny blocked if banned == true;
        "#,
        )
        .unwrap();

        assert_eq!(
            policy.evaluate(&json!({"role": "admin", "banned": false})),
            Effect::Allow
        );
        assert_eq!(
            policy.evaluate(&json!({"role": "admin", "banned": true})),
            Effect::Deny
        );
    }

    #[test]
    fn test_compile_readme_example() {
        let policy = compile(
            r#"
            allow access if
                action == "call" and
                { name: "lhs", value: 10 } in resource.context.namedArguments;
        "#,
        )
        .unwrap();

        let input = json!({
            "principal": "alice",
            "action": "call",
            "resource": {
                "method": "add",
                "context": {
                    "namedArguments": [
                        {"name": "random_junk", "value": 999},
                        {"name": "lhs", "value": 10, "type": "int"},
                        {"name": "rhs", "value": 5, "type": "int"}
                    ]
                }
            }
        });

        assert_eq!(policy.evaluate(&input), Effect::Allow);
    }

    #[test]
    fn test_compile_not_in() {
        // Need an allow rule for the "safe" case to return Allow
        let policy = compile(
            r#"
            allow safe if "safe" in tags;
            deny blocked if not "safe" in tags;
        "#,
        )
        .unwrap();
        // When "safe" is NOT in tags, the deny rule triggers
        assert_eq!(
            policy.evaluate(&json!({"tags": ["dangerous", "risky"]})),
            Effect::Deny
        );
        // When "safe" IS in tags, allow rule triggers
        assert_eq!(
            policy.evaluate(&json!({"tags": ["safe", "verified"]})),
            Effect::Allow
        );
    }

    #[test]
    fn test_compile_exists_with_variable_binding() {
        // The core use case: exists with dynamic path lookup using bound variable
        // This is the nested resource hierarchy pattern
        let policy = compile(
            r#"
            allow read if
                action == "read" and
                exists ancestor in resource.ancestors:
                    "reader" in principal.resourceRoles[ancestor];
        "#,
        )
        .unwrap();

        // User has reader role on repo:backend, which is an ancestor
        assert_eq!(
            policy.evaluate(&json!({
                "action": "read",
                "resource": {"ancestors": ["folder:src", "repo:backend"]},
                "principal": {"resourceRoles": {"repo:backend": ["reader", "committer"]}}
            })),
            Effect::Allow
        );

        // User has no roles on any ancestor
        assert_eq!(
            policy.evaluate(&json!({
                "action": "read",
                "resource": {"ancestors": ["folder:src", "repo:backend"]},
                "principal": {"resourceRoles": {"other:thing": ["reader"]}}
            })),
            Effect::Deny
        );

        // User has reader on first ancestor (folder:src)
        assert_eq!(
            policy.evaluate(&json!({
                "action": "read",
                "resource": {"ancestors": ["folder:src", "repo:backend"]},
                "principal": {"resourceRoles": {"folder:src": ["reader"]}}
            })),
            Effect::Allow
        );
    }

    #[test]
    fn test_compile_forall_with_variable_binding() {
        // forall with variable binding - all ancestors must have reader role
        let policy = compile(
            r#"
            allow full_read if
                exists ancestor in resource.ancestors:
                    "admin" in principal.resourceRoles[ancestor];
        "#,
        )
        .unwrap();

        // Has admin on all ancestors - should allow
        assert_eq!(
            policy.evaluate(&json!({
                "resource": {"ancestors": ["folder:src", "repo:backend"]},
                "principal": {"resourceRoles": {
                    "folder:src": ["admin"],
                    "repo:backend": ["admin"]
                }}
            })),
            Effect::Allow
        );
    }

    #[test]
    fn test_compile_path_expression_in_brackets() {
        // Dynamic path in brackets: principal.resourceRoles[resource.id]
        let policy = compile(
            r#"
            allow admin if
                "admin" in principal.resourceRoles[resource.id];
        "#,
        )
        .unwrap();

        // User has admin role on the specific resource
        assert_eq!(
            policy.evaluate(&json!({
                "principal": {"resourceRoles": {"repo:anvil": ["admin", "member"]}},
                "resource": {"id": "repo:anvil"}
            })),
            Effect::Allow
        );

        // User has role on different resource
        assert_eq!(
            policy.evaluate(&json!({
                "principal": {"resourceRoles": {"repo:other": ["admin"]}},
                "resource": {"id": "repo:anvil"}
            })),
            Effect::Deny
        );

        // User has member role, not admin
        assert_eq!(
            policy.evaluate(&json!({
                "principal": {"resourceRoles": {"repo:anvil": ["member"]}},
                "resource": {"id": "repo:anvil"}
            })),
            Effect::Deny
        );
    }

    #[test]
    fn test_or_basic() {
        let policy = compile(r#"allow access if action == "read" or action == "write";"#).unwrap();
        assert_eq!(policy.evaluate(&json!({"action": "read"})), Effect::Allow);
        assert_eq!(policy.evaluate(&json!({"action": "write"})), Effect::Allow);
        assert_eq!(policy.evaluate(&json!({"action": "delete"})), Effect::Deny);
    }

    #[test]
    fn test_or_with_and() {
        let policy = compile(
            r#"allow access if (action == "read" or action == "write") and role == "user";"#,
        )
        .unwrap();
        assert_eq!(
            policy.evaluate(&json!({"action": "read", "role": "user"})),
            Effect::Allow
        );
        assert_eq!(
            policy.evaluate(&json!({"action": "write", "role": "user"})),
            Effect::Allow
        );
        assert_eq!(
            policy.evaluate(&json!({"action": "read", "role": "guest"})),
            Effect::Deny
        );
        assert_eq!(
            policy.evaluate(&json!({"action": "delete", "role": "user"})),
            Effect::Deny
        );
    }

    #[test]
    fn test_not_or() {
        let policy = compile(
            r#"
            allow access if role == "admin" or role == "moderator";
            deny block if not (role == "admin" or role == "moderator");
            "#,
        )
        .unwrap();
        assert_eq!(policy.evaluate(&json!({"role": "admin"})), Effect::Allow);
        assert_eq!(policy.evaluate(&json!({"role": "guest"})), Effect::Deny);
    }

    #[test]
    fn test_complex_or_and_not() {
        // The user's exact example
        let policy = compile(
            r#"deny foobar if (action == "fizz" or action == "buzz") and not (resource == "bar" or resource == "baz");"#,
        )
        .unwrap();
        // fizz + not(bar/baz) → deny
        assert_eq!(
            policy.evaluate(&json!({"action": "fizz", "resource": "qux"})),
            Effect::Deny
        );
        // buzz + not(bar/baz) → deny
        assert_eq!(
            policy.evaluate(&json!({"action": "buzz", "resource": "qux"})),
            Effect::Deny
        );
        // fizz + bar → deny rule doesn't match because not(resource=="bar") is false
        // but still default deny (no allow rule)
        // Let's test with an allow rule too:
        let policy2 = compile(
            r#"
            allow ok;
            deny foobar if (action == "fizz" or action == "buzz") and not (resource == "bar" or resource == "baz");
            "#,
        )
        .unwrap();
        // fizz + bar → allow (deny rule doesn't match, allow catches)
        assert_eq!(
            policy2.evaluate(&json!({"action": "fizz", "resource": "bar"})),
            Effect::Allow
        );
        // fizz + qux → deny
        assert_eq!(
            policy2.evaluate(&json!({"action": "fizz", "resource": "qux"})),
            Effect::Deny
        );
        // other action → allow (deny doesn't match)
        assert_eq!(
            policy2.evaluate(&json!({"action": "other", "resource": "qux"})),
            Effect::Allow
        );
    }
}

#[cfg(test)]
mod schema_compile_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_compile_assertion_untyped() {
        // assert should work WITHOUT `use schema;`
        let src = concat!(
            "assert is_admin if principal.role == \"admin\";\n",
            "allow manage if is_admin and action == \"manage\";\n",
        );
        let policy = compile(src).unwrap();

        assert_eq!(
            policy.evaluate(&json!({"principal": {"role": "admin"}, "action": "manage"})),
            Effect::Allow
        );
        assert_eq!(
            policy.evaluate(&json!({"principal": {"role": "user"}, "action": "manage"})),
            Effect::Deny
        );
    }

    #[test]
    fn test_compile_assertion_inlining() {
        // assert is_admin if principal.role == "admin";
        // allow access if is_admin;
        let src = concat!(
            "use schema;\n",
            "assert is_admin if principal.role == \"admin\";\n",
            "allow access if is_admin;\n",
        );
        let policy = compile(src).unwrap();
        assert_eq!(policy.rules.len(), 1);

        // Should allow when principal.role == "admin"
        assert_eq!(
            policy.evaluate(&json!({"principal": {"role": "admin"}})),
            Effect::Allow
        );
        // Should deny when principal.role != "admin"
        assert_eq!(
            policy.evaluate(&json!({"principal": {"role": "user"}})),
            Effect::Deny
        );
    }

    #[test]
    fn test_compile_type_ref_rule() {
        // allow do_delete if Ns:Delete and principal.role == "admin";
        let src = concat!(
            "use schema;\n",
            "allow do_delete if Ns:Delete and principal.role == \"admin\";\n",
        );
        let policy = compile(src).unwrap();
        assert_eq!(policy.rules.len(), 1);

        // Should allow when action=Delete and role=admin
        assert_eq!(
            policy.evaluate(&json!({
                "action": "Delete",
                "principal": {"role": "admin"}
            })),
            Effect::Allow
        );
        // Should deny when action is wrong
        assert_eq!(
            policy.evaluate(&json!({
                "action": "Read",
                "principal": {"role": "admin"}
            })),
            Effect::Deny
        );
        // Should deny when role is wrong
        assert_eq!(
            policy.evaluate(&json!({
                "action": "Delete",
                "principal": {"role": "user"}
            })),
            Effect::Deny
        );
    }

    #[test]
    fn test_compile_assertion_with_type_ref() {
        // assert is_owner if principal.name == resource.owner;
        // allow do_delete if Ns:Delete and is_owner;
        let src = concat!(
            "use schema;\n",
            "assert is_owner if principal.name == resource.owner;\n",
            "allow do_delete if Ns:Delete and is_owner;\n",
        );
        let policy = compile(src).unwrap();

        // Should allow: correct action and owner match
        assert_eq!(
            policy.evaluate(&json!({
                "action": "Delete",
                "resource": {"owner": "alice"},
                "principal": {"name": "alice"}
            })),
            Effect::Allow
        );
        // Should deny: owner doesn't match
        assert_eq!(
            policy.evaluate(&json!({
                "action": "Delete",
                "resource": {"owner": "bob"},
                "principal": {"name": "alice"}
            })),
            Effect::Deny
        );
    }

    #[test]
    fn test_compile_assertion_in_and() {
        // assert is_admin if principal.role == "admin";
        // allow manage if is_admin and action == "manage";
        let src = concat!(
            "use schema;\n",
            "assert is_admin if principal.role == \"admin\";\n",
            "allow manage if is_admin and action == \"manage\";\n",
        );
        let policy = compile(src).unwrap();

        assert_eq!(
            policy.evaluate(&json!({"principal": {"role": "admin"}, "action": "manage"})),
            Effect::Allow
        );
        assert_eq!(
            policy.evaluate(&json!({"principal": {"role": "user"}, "action": "manage"})),
            Effect::Deny
        );
        assert_eq!(
            policy.evaluate(&json!({"principal": {"role": "admin"}, "action": "read"})),
            Effect::Deny
        );
    }

    #[test]
    fn test_compile_nested_assertion_inlining() {
        // assert has_role if principal has role;
        // assert is_admin if has_role and principal.role == "admin";
        // (has_role must be inlined first, then is_admin)
        let src = concat!(
            "use schema;\n",
            "assert has_role if principal has role;\n",
            "assert is_admin if has_role and principal.role == \"admin\";\n",
            "allow access if is_admin;\n",
        );
        let policy = compile(src).unwrap();

        // Should allow when principal has role and role is admin
        assert_eq!(
            policy.evaluate(&json!({"principal": {"role": "admin"}})),
            Effect::Allow
        );
        // Should deny when principal has role but not admin
        assert_eq!(
            policy.evaluate(&json!({"principal": {"role": "user"}})),
            Effect::Deny
        );
        // Should deny when principal doesn't have role
        assert_eq!(policy.evaluate(&json!({"principal": {}})), Effect::Deny);
    }

    #[test]
    fn test_compile_type_ref_unconditional() {
        // allow do_delete if Ns:Delete;
        let src = "use schema;\nallow do_delete if Ns:Delete;";
        let policy = compile(src).unwrap();

        assert_eq!(policy.evaluate(&json!({"action": "Delete"})), Effect::Allow);
        assert_eq!(policy.evaluate(&json!({"action": "Read"})), Effect::Deny);
    }

    #[test]
    fn test_compile_host_assert_bare_identifier() {
        // A bare identifier that matches a host assert should compile to HostAssert
        let mut host_asserts = HashSet::new();
        host_asserts.insert("resource_is_package_local".to_string());

        let src = r#"allow ro if resource_is_package_local;"#;
        let policy = compile_with_host_asserts(src, &host_asserts).unwrap();

        // Without context, host asserts default to false → deny
        assert_eq!(policy.evaluate(&json!({"action": "ro"})), Effect::Deny);

        // With context where the callback returns true → allow
        let mut ctx = crate::rule::EvalContext::new();
        ctx.register("resource_is_package_local", |_| true);
        assert_eq!(
            policy.evaluate_with_context(&json!({"action": "ro"}), &ctx),
            Effect::Allow
        );

        // With context where callback returns false → deny
        let mut ctx_false = crate::rule::EvalContext::new();
        ctx_false.register("resource_is_package_local", |_| false);
        assert_eq!(
            policy.evaluate_with_context(&json!({"action": "ro"}), &ctx_false),
            Effect::Deny
        );
    }

    #[test]
    fn test_host_assert_combined_with_conditions() {
        let mut host_asserts = HashSet::new();
        host_asserts.insert("is_local".to_string());

        let src = r#"allow access if action == "read" and is_local;"#;
        let policy = compile_with_host_asserts(src, &host_asserts).unwrap();

        let mut ctx = crate::rule::EvalContext::new();
        ctx.register("is_local", |_| true);

        // Both conditions met → allow
        assert_eq!(
            policy.evaluate_with_context(&json!({"action": "read"}), &ctx),
            Effect::Allow
        );

        // Action doesn't match → deny even if host assert passes
        assert_eq!(
            policy.evaluate_with_context(&json!({"action": "write"}), &ctx),
            Effect::Deny
        );
    }

    #[test]
    fn test_unknown_bare_identifier_without_host_assert() {
        // A bare identifier that is NOT a host assert compiles to path == true
        // (standard behavior for unknown identifiers)
        let src = r#"allow check if some_flag;"#;
        let policy = compile(src).unwrap();

        // some_flag == true → allow
        assert_eq!(policy.evaluate(&json!({"some_flag": true})), Effect::Allow);
        // some_flag == false → deny
        assert_eq!(policy.evaluate(&json!({"some_flag": false})), Effect::Deny);
    }

    #[test]
    fn test_host_assert_takes_priority_over_path_check() {
        // If "my_check" is registered as host assert, `my_check` in a rule
        // should be compiled as HostAssert, NOT as path `my_check == true`
        let mut host_asserts = HashSet::new();
        host_asserts.insert("my_check".to_string());

        let src = r#"allow access if my_check;"#;
        let policy = compile_with_host_asserts(src, &host_asserts).unwrap();

        // Even if input has `my_check: true`, without context it should deny
        // (HostAssert defaults to false without EvalContext)
        assert_eq!(policy.evaluate(&json!({"my_check": true})), Effect::Deny);

        // With context providing the callback → allow
        let mut ctx = crate::rule::EvalContext::new();
        ctx.register("my_check", |_| true);
        assert_eq!(
            policy.evaluate_with_context(&json!({}), &ctx),
            Effect::Allow
        );
    }
}

#[cfg(test)]
mod quantifier_regression_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_forall_with_dot_path_body() {
        // Regression: `item.approved` inside forall must resolve `item` from bindings
        let policy = compile(
            r#"
            allow batch if
                forall item in resource.items:
                    item.approved == true;
        "#,
        )
        .unwrap();

        assert_eq!(
            policy.evaluate(&json!({
                "resource": {
                    "items": [
                        {"id": 1, "approved": true},
                        {"id": 2, "approved": true}
                    ]
                }
            })),
            Effect::Allow,
        );

        // One unapproved → deny
        assert_eq!(
            policy.evaluate(&json!({
                "resource": {
                    "items": [
                        {"id": 1, "approved": true},
                        {"id": 2, "approved": false}
                    ]
                }
            })),
            Effect::Deny,
        );
    }

    #[test]
    fn test_exists_with_dot_path_body() {
        let policy = compile(
            r#"
            allow ok;
            deny flagged if
                exists tag in resource.tags:
                    tag == "blocked";
        "#,
        )
        .unwrap();

        assert_eq!(
            policy.evaluate(&json!({"resource": {"tags": ["normal", "reviewed"]}})),
            Effect::Allow,
        );
        assert_eq!(
            policy.evaluate(&json!({"resource": {"tags": ["normal", "blocked"]}})),
            Effect::Deny,
        );
    }
}
