//! Semantic diff for policy changes.
//!
//! Compares policies to identify added, removed, and modified rules.
//!
//! # Example
//!
//! ```rust
//! use karu::compile;
//! use karu::diff::PolicyDiff;
//!
//! let old = compile(r#"
//!     allow rule1 if role == "admin";
//!     allow rule2 if role == "user";
//! "#).unwrap();
//!
//! let new = compile(r#"
//!     allow rule1 if role == "admin";
//!     allow rule3 if role == "guest";
//! "#).unwrap();
//!
//! let diff = PolicyDiff::compare(&old, &new);
//! assert_eq!(diff.added.len(), 1);    // rule3
//! assert_eq!(diff.removed.len(), 1);  // rule2
//! ```

use crate::rule::{Effect, Policy, Rule};
use std::collections::HashSet;

/// Represents the difference between two policies.
#[derive(Debug, Clone)]
pub struct PolicyDiff {
    /// Rules added in the new policy.
    pub added: Vec<RuleSummary>,
    /// Rules removed from the old policy.
    pub removed: Vec<RuleSummary>,
    /// Rules with the same name but different content.
    pub modified: Vec<RuleModification>,
    /// Rules unchanged between policies.
    pub unchanged: Vec<String>,
}

/// Summary of a rule for diff output.
#[derive(Debug, Clone)]
pub struct RuleSummary {
    /// Rule name.
    pub name: String,
    /// Rule effect.
    pub effect: Effect,
    /// Number of conditions.
    pub condition_count: usize,
}

/// A modification to an existing rule.
#[derive(Debug, Clone)]
pub struct RuleModification {
    /// Rule name.
    pub name: String,
    /// Old effect.
    pub old_effect: Effect,
    /// New effect.
    pub new_effect: Effect,
    /// Old condition count.
    pub old_conditions: usize,
    /// New condition count.
    pub new_conditions: usize,
    /// Whether the effect changed.
    pub effect_changed: bool,
    /// Whether conditions changed.
    pub conditions_changed: bool,
}

impl PolicyDiff {
    /// Compare two policies and produce a diff.
    pub fn compare(old: &Policy, new: &Policy) -> Self {
        let old_names: HashSet<_> = old.rules.iter().map(|r| &r.name).collect();
        let new_names: HashSet<_> = new.rules.iter().map(|r| &r.name).collect();

        let mut added = Vec::new();
        let mut removed = Vec::new();
        let mut modified = Vec::new();
        let mut unchanged = Vec::new();

        // Find added rules
        for rule in &new.rules {
            if !old_names.contains(&rule.name) {
                added.push(RuleSummary::from_rule(rule));
            }
        }

        // Find removed rules
        for rule in &old.rules {
            if !new_names.contains(&rule.name) {
                removed.push(RuleSummary::from_rule(rule));
            }
        }

        // Find modified and unchanged rules
        for old_rule in &old.rules {
            if let Some(new_rule) = new.rules.iter().find(|r| r.name == old_rule.name) {
                let effect_changed = old_rule.effect != new_rule.effect;
                let conditions_changed = !rules_semantically_equal(old_rule, new_rule);

                if effect_changed || conditions_changed {
                    modified.push(RuleModification {
                        name: old_rule.name.clone(),
                        old_effect: old_rule.effect,
                        new_effect: new_rule.effect,
                        old_conditions: count_conditions(&old_rule.body),
                        new_conditions: count_conditions(&new_rule.body),
                        effect_changed,
                        conditions_changed,
                    });
                } else {
                    unchanged.push(old_rule.name.clone());
                }
            }
        }

        Self {
            added,
            removed,
            modified,
            unchanged,
        }
    }

    /// Check if there are any changes.
    pub fn has_changes(&self) -> bool {
        !self.added.is_empty() || !self.removed.is_empty() || !self.modified.is_empty()
    }

    /// Get a summary string of changes.
    pub fn summary(&self) -> String {
        format!(
            "+{} added, -{} removed, ~{} modified, ={} unchanged",
            self.added.len(),
            self.removed.len(),
            self.modified.len(),
            self.unchanged.len()
        )
    }
}

impl RuleSummary {
    fn from_rule(rule: &Rule) -> Self {
        Self {
            name: rule.name.clone(),
            effect: rule.effect,
            condition_count: count_conditions(&rule.body),
        }
    }
}

/// Count the number of leaf conditions in a body expression.
fn count_conditions(body: &Option<crate::rule::ConditionExpr>) -> usize {
    match body {
        None => 0,
        Some(expr) => count_expr(expr),
    }
}

fn count_expr(expr: &crate::rule::ConditionExpr) -> usize {
    use crate::rule::ConditionExpr;
    match expr {
        ConditionExpr::Leaf(_) => 1,
        ConditionExpr::And(v) | ConditionExpr::Or(v) => v.iter().map(count_expr).sum(),
        ConditionExpr::Not(inner) => count_expr(inner),
        ConditionExpr::IsType { .. } => 1,
        ConditionExpr::HostAssert(_) => 1,
    }
}

/// Check if two rules are semantically equal.
fn rules_semantically_equal(a: &Rule, b: &Rule) -> bool {
    if a.effect != b.effect {
        return false;
    }
    // Compare by debug representation (simple approximation)
    format!("{:?}", a.body) == format!("{:?}", b.body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile;

    #[test]
    fn test_diff_no_changes() {
        let policy = compile(r#"allow access if role == "admin";"#).unwrap();
        let diff = PolicyDiff::compare(&policy, &policy);

        assert!(!diff.has_changes());
        assert_eq!(diff.unchanged.len(), 1);
    }

    #[test]
    fn test_diff_added_rule() {
        let old = compile(r#"allow rule1 if x == "a";"#).unwrap();
        let new = compile(
            r#"
            allow rule1 if x == "a";
            allow rule2 if y == "b";
        "#,
        )
        .unwrap();

        let diff = PolicyDiff::compare(&old, &new);

        assert!(diff.has_changes());
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.added[0].name, "rule2");
        assert_eq!(diff.removed.len(), 0);
    }

    #[test]
    fn test_diff_removed_rule() {
        let old = compile(
            r#"
            allow rule1 if x == "a";
            allow rule2 if y == "b";
        "#,
        )
        .unwrap();
        let new = compile(r#"allow rule1 if x == "a";"#).unwrap();

        let diff = PolicyDiff::compare(&old, &new);

        assert!(diff.has_changes());
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.removed[0].name, "rule2");
    }

    #[test]
    fn test_diff_modified_effect() {
        let old = compile(r#"allow access if role == "admin";"#).unwrap();
        let new = compile(r#"deny access if role == "admin";"#).unwrap();

        let diff = PolicyDiff::compare(&old, &new);

        assert!(diff.has_changes());
        assert_eq!(diff.modified.len(), 1);
        assert!(diff.modified[0].effect_changed);
    }

    #[test]
    fn test_diff_summary() {
        let old = compile(r#"allow rule1 if x == "a";"#).unwrap();
        let new = compile(r#"allow rule2 if y == "b";"#).unwrap();

        let diff = PolicyDiff::compare(&old, &new);
        let summary = diff.summary();

        assert!(summary.contains("+1 added"));
        assert!(summary.contains("-1 removed"));
    }
}
