// SPDX-License-Identifier: MIT

//! Simulation mode for "what-if" policy analysis.
//!
//! Evaluate policies without enforcement, with detailed decision rationale.
//!
//! # Example
//!
//! ```rust
//! use karu::compile;
//! use karu::simulate::Simulator;
//! use karu::rule::Effect;
//! use serde_json::json;
//!
//! let policy = compile(r#"
//!     allow admin_access if role == "admin";
//!     deny guest_block if role == "guest";
//! "#).unwrap();
//!
//! let simulator = Simulator::new(policy);
//! let result = simulator.simulate(&json!({"role": "admin"}));
//!
//! assert_eq!(result.decision, Effect::Allow);
//! assert_eq!(result.matched_rules, vec!["admin_access"]);
//! ```

use crate::rule::{Effect, Policy, Rule};
use serde_json::Value;

/// Result of a policy simulation.
#[derive(Debug, Clone)]
pub struct SimulationResult {
    /// The decision that would be made.
    pub decision: Effect,
    /// Names of rules that matched.
    pub matched_rules: Vec<String>,
    /// Names of rules that were evaluated but didn't match.
    pub evaluated_rules: Vec<String>,
    /// Detailed trace of rule evaluation.
    pub trace: Vec<RuleTrace>,
}

/// Trace of a single rule evaluation.
#[derive(Debug, Clone)]
pub struct RuleTrace {
    /// Rule name.
    pub name: String,
    /// Rule effect.
    pub effect: Effect,
    /// Whether the rule matched.
    pub matched: bool,
    /// Which conditions passed/failed.
    pub conditions: Vec<ConditionTrace>,
}

/// Trace of a condition evaluation.
#[derive(Debug, Clone)]
pub struct ConditionTrace {
    /// Path being evaluated.
    pub path: String,
    /// Whether this condition passed.
    pub passed: bool,
}

/// Policy simulator for what-if analysis.
#[derive(Debug)]
pub struct Simulator {
    policy: Policy,
}

impl Simulator {
    /// Create a new simulator for a policy.
    pub fn new(policy: Policy) -> Self {
        Self { policy }
    }

    /// Simulate evaluation of a request.
    pub fn simulate(&self, input: &Value) -> SimulationResult {
        let mut matched_rules = Vec::new();
        let mut evaluated_rules = Vec::new();
        let mut trace = Vec::new();
        let mut final_decision = None;

        for rule in &self.policy.rules {
            let (matched, conditions) = self.evaluate_rule(rule, input);
            evaluated_rules.push(rule.name.clone());

            trace.push(RuleTrace {
                name: rule.name.clone(),
                effect: rule.effect,
                matched,
                conditions,
            });

            if matched {
                matched_rules.push(rule.name.clone());

                match rule.effect {
                    Effect::Deny => {
                        // Deny overrides - stop immediately
                        return SimulationResult {
                            decision: Effect::Deny,
                            matched_rules,
                            evaluated_rules,
                            trace,
                        };
                    }
                    Effect::Allow => {
                        if final_decision.is_none() {
                            final_decision = Some(Effect::Allow);
                        }
                    }
                }
            }
        }

        SimulationResult {
            decision: final_decision.unwrap_or(Effect::Deny),
            matched_rules,
            evaluated_rules,
            trace,
        }
    }

    fn evaluate_rule(&self, rule: &Rule, input: &Value) -> (bool, Vec<ConditionTrace>) {
        let matched = match &rule.body {
            Some(body) => body.evaluate(input),
            None => true,
        };
        let conditions = if rule.body.is_some() {
            vec![ConditionTrace {
                path: format!("{:?}", rule.body),
                passed: matched,
            }]
        } else {
            vec![]
        };
        (matched, conditions)
    }

    /// Compare simulation against another policy (for what-if analysis).
    pub fn compare(&self, other: &Policy, input: &Value) -> ComparisonResult {
        let current = self.simulate(input);
        let proposed = Simulator::new(other.clone()).simulate(input);

        ComparisonResult {
            current_decision: current.decision,
            proposed_decision: proposed.decision,
            would_change: current.decision != proposed.decision,
            current,
            proposed,
        }
    }
}

/// Result of comparing two policy simulations.
#[derive(Debug)]
pub struct ComparisonResult {
    /// Decision from current policy.
    pub current_decision: Effect,
    /// Decision from proposed policy.
    pub proposed_decision: Effect,
    /// Whether the decision would change.
    pub would_change: bool,
    /// Full current simulation.
    pub current: SimulationResult,
    /// Full proposed simulation.
    pub proposed: SimulationResult,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile;
    use serde_json::json;

    #[test]
    fn test_simulation_basic() {
        let policy = compile(r#"allow access if role == "admin";"#).unwrap();
        let simulator = Simulator::new(policy);

        let result = simulator.simulate(&json!({"role": "admin"}));
        assert_eq!(result.decision, Effect::Allow);
        assert_eq!(result.matched_rules, vec!["access"]);
    }

    #[test]
    fn test_simulation_trace() {
        let policy = compile(
            r#"
            allow rule1 if x == "one";
            allow rule2 if y == "two";
        "#,
        )
        .unwrap();

        let simulator = Simulator::new(policy);
        let result = simulator.simulate(&json!({"x": "one", "y": "three"}));

        assert_eq!(result.decision, Effect::Allow);
        assert_eq!(result.trace.len(), 2);
        assert!(result.trace[0].matched);
        assert!(!result.trace[1].matched);
    }

    #[test]
    fn test_comparison() {
        let current = compile(r#"allow access if role == "admin";"#).unwrap();
        let proposed = compile(r#"allow access if role == "user";"#).unwrap();

        let simulator = Simulator::new(current);
        let comparison = simulator.compare(&proposed, &json!({"role": "user"}));

        assert!(comparison.would_change);
        assert_eq!(comparison.current_decision, Effect::Deny);
        assert_eq!(comparison.proposed_decision, Effect::Allow);
    }
}
