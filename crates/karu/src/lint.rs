// SPDX-License-Identifier: MIT

//! Policy linter - detects common pitfalls in Karu policies.
//!
//! The linter walks a parsed AST and produces [`LintWarning`] diagnostics.
//! It is used by both the LSP (as part of semantic diagnostics) and the CLI
//! (`karu lint` / `karu check`).
//!
//! # Lint Rules
//!
//! | Code | Name | Severity |
//! |------|------|----------|
//! | W001 | `forall-empty-vacuous` | Warning |

use crate::ast::{ExprAst, PathAst, Program, RuleAst};

/// A warning produced by the linter.
#[derive(Debug, Clone)]
pub struct LintWarning {
    /// Machine-readable code, e.g. "W001".
    pub code: &'static str,
    /// Human-readable message.
    pub message: String,
    /// Suggested fix description (for code actions).
    pub suggestion: Option<String>,
    /// Name of the rule that triggered this warning.
    pub rule_name: String,
    /// The source path in the forall (for precise positioning).
    pub forall_path: Option<String>,
}

/// Run all lint rules against a parsed program.
///
/// Returns an empty vector if no issues are found.
pub fn lint(program: &Program) -> Vec<LintWarning> {
    let mut warnings = Vec::new();

    for rule in &program.rules {
        lint_rule(rule, &mut warnings);
    }

    warnings
}

/// Lint a single rule.
fn lint_rule(rule: &RuleAst, warnings: &mut Vec<LintWarning>) {
    if let Some(ref body) = rule.body {
        check_forall_vacuous(body, &rule.name, warnings);
    }
}

// ---------------------------------------------------------------------------
// W001: forall-empty-vacuous
//
// Detects `forall` expressions that aren't guarded by a length/non-empty
// check on the source array. When the array is empty, `forall` is vacuously
// true, which may unintentionally grant access.
//
// The linter considers a forall "guarded" if it appears inside an `And`
// alongside any condition that references the same source path (e.g. a
// `has` check, a length comparison, or any `Compare`/`In` on the path).
// ---------------------------------------------------------------------------

/// Format a PathAst as a dotted string for display.
fn path_to_string(path: &PathAst) -> String {
    use crate::ast::PathSegmentAst;
    use std::fmt::Write;
    let mut out = String::new();
    let mut first = true;
    for segment in &path.segments {
        if !first {
            out.push('.');
        }
        first = false;
        match segment {
            PathSegmentAst::Field(name) => out.push_str(name),
            PathSegmentAst::Index(idx) => {
                let _ = write!(out, "[{}]", idx);
            }
            PathSegmentAst::Variable(var) => {
                let _ = write!(out, "[{}]", var);
            }
        }
    }
    out
}

/// Check if an `And` sibling references the forall's source path.
///
/// We consider nearly any reference to the forall source path as a "guard"
/// because it implies the author has at least acknowledged the path exists.
/// This avoids false positives from creative guard patterns.
fn is_path_prefix(path: &PathAst, target: &PathAst) -> bool {
    if path.segments.len() > target.segments.len() {
        return false;
    }
    path.segments
        .iter()
        .zip(target.segments.iter())
        .all(|(a, b)| format!("{:?}", a) == format!("{:?}", b))
}

/// Extract all paths referenced in an expression (non-recursively into forall bodies).
fn collect_referenced_paths(expr: &ExprAst) -> Vec<&PathAst> {
    match expr {
        ExprAst::Compare { left, .. } => vec![left],
        ExprAst::In { path, .. } => vec![path],
        ExprAst::InLiteral { path, .. } => vec![path],
        ExprAst::Has { path } => vec![path],
        ExprAst::Like { path, .. } => vec![path],
        ExprAst::Not(inner) => collect_referenced_paths(inner),
        // Don't descend into And/Or - we check siblings at the And level
        _ => vec![],
    }
}

/// Check if a forall's source path is guarded by any sibling in an And.
fn has_guard_for_path(siblings: &[ExprAst], forall_path: &PathAst) -> bool {
    for sibling in siblings {
        for path in collect_referenced_paths(sibling) {
            if is_path_prefix(path, forall_path) || is_path_prefix(forall_path, path) {
                return true;
            }
        }
    }
    false
}

/// Recursively check for unguarded forall expressions.
fn check_forall_vacuous(expr: &ExprAst, rule_name: &str, warnings: &mut Vec<LintWarning>) {
    match expr {
        ExprAst::And(exprs) => {
            // For each child in this And:
            // - If it's a Forall, check siblings for a guard and handle it fully
            // - Otherwise, recurse normally
            for (i, child) in exprs.iter().enumerate() {
                if let ExprAst::Forall { path, body, .. } = child {
                    // Collect siblings (all non-self And children)
                    let siblings: Vec<ExprAst> = exprs
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| *j != i)
                        .map(|(_, e)| e.clone())
                        .collect();

                    if !has_guard_for_path(&siblings, path) {
                        emit_w001(path, rule_name, warnings);
                    }
                    // Recurse into the forall body for nested foralls
                    check_forall_vacuous(body, rule_name, warnings);
                } else {
                    // Non-forall child: recurse normally
                    check_forall_vacuous(child, rule_name, warnings);
                }
            }
        }
        ExprAst::Or(exprs) => {
            for child in exprs {
                check_forall_vacuous(child, rule_name, warnings);
            }
        }
        ExprAst::Not(inner) => {
            check_forall_vacuous(inner, rule_name, warnings);
        }
        ExprAst::Forall { path, body, .. } => {
            // A bare forall (not inside And) is always unguarded
            emit_w001(path, rule_name, warnings);
            // Recurse into body for nested foralls
            check_forall_vacuous(body, rule_name, warnings);
        }
        // All other expression types: no forall to check
        _ => {}
    }
}

/// Emit a W001 warning for an unguarded forall.
fn emit_w001(path: &PathAst, rule_name: &str, warnings: &mut Vec<LintWarning>) {
    let path_str = path_to_string(path);
    warnings.push(LintWarning {
        code: "W001",
        message: format!(
            "forall over `{}` is vacuously true when the collection is empty - \
             this may unintentionally grant access",
            path_str
        ),
        suggestion: Some(format!("Add a guard: `has {} and forall ...`", path_str)),
        rule_name: rule_name.to_string(),
        forall_path: Some(path_str),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    fn lint_source(source: &str) -> Vec<LintWarning> {
        let program = Parser::parse(source).expect("parse failed");
        lint(&program)
    }

    #[test]
    fn forall_without_guard_emits_w001() {
        let warnings =
            lint_source(r#"allow access if forall user in users: user.verified == true;"#);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].code, "W001");
        assert!(warnings[0].message.contains("users"));
        assert_eq!(warnings[0].rule_name, "access");
        assert!(warnings[0].suggestion.is_some());
    }

    #[test]
    fn forall_with_has_guard_no_warning() {
        let warnings = lint_source(
            r#"allow access if has users and forall user in users: user.verified == true;"#,
        );
        assert_eq!(warnings.len(), 0, "has guard should suppress W001");
    }

    #[test]
    fn forall_with_length_guard_no_warning() {
        // A comparison on users (e.g. users != null or any reference) counts as a guard
        let warnings = lint_source(
            r#"allow access if users != null and forall user in users: user.verified == true;"#,
        );
        assert_eq!(warnings.len(), 0, "comparison guard should suppress W001");
    }

    #[test]
    fn forall_with_in_guard_no_warning() {
        // Using `in` on the same path as a guard
        let warnings = lint_source(
            r#"allow access if "admin" in users and forall user in users: user.active == true;"#,
        );
        assert_eq!(warnings.len(), 0, "in-check guard should suppress W001");
    }

    #[test]
    fn exists_does_not_trigger_w001() {
        let warnings =
            lint_source(r#"allow access if exists user in users: user.verified == true;"#);
        assert_eq!(warnings.len(), 0, "exists should not trigger W001");
    }

    #[test]
    fn multiple_forall_emits_multiple_warnings() {
        let warnings = lint_source(
            r#"allow access if forall user in users: user.active == true and forall role in roles: role.valid == true;"#,
        );
        // Should warn about both unguarded foralls
        assert!(
            warnings.len() >= 2,
            "should have at least 2 warnings, got {}",
            warnings.len()
        );
    }

    #[test]
    fn nested_forall_emits_warnings() {
        let warnings = lint_source(
            r#"allow access if forall group in groups: forall user in group.members: user.active == true;"#,
        );
        // Outer and inner forall are both unguarded
        assert!(warnings.len() >= 2, "nested foralls should both warn");
    }

    #[test]
    fn forall_in_deny_still_warns() {
        let warnings = lint_source(r#"deny block if forall user in users: user.banned == true;"#);
        assert_eq!(warnings.len(), 1, "deny rules with forall should also warn");
        assert_eq!(warnings[0].code, "W001");
    }

    #[test]
    fn no_forall_no_warnings() {
        let warnings = lint_source(r#"allow access if action == "read" and role == "admin";"#);
        assert_eq!(warnings.len(), 0);
    }

    #[test]
    fn forall_guarded_by_subpath_no_warning() {
        // A check on users.count references a sub-path of users - still counts as guard
        let warnings = lint_source(
            r#"allow access if users.count > 0 and forall user in users: user.active == true;"#,
        );
        assert_eq!(warnings.len(), 0, "sub-path guard should suppress W001");
    }
}
