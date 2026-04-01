// SPDX-License-Identifier: MIT

//! Rule definitions and evaluation for Karu.
//!
//! Rules combine patterns, paths, and operators to express authorization logic.

use crate::matcher::{all_match, any_matches, matches_ref};
use crate::path::Path;
use crate::pattern::Pattern;
use crate::type_registry::{fingerprint_value, TypeShape};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// A host-provided assert callback.
///
/// Takes the evaluation input (the JSON request being authorized)
/// and returns `true` if the assert condition is satisfied.
pub type HostAssertFn = Arc<dyn Fn(&Value) -> bool + Send + Sync>;

/// Evaluation context for host-provided assert hooks.
///
/// When a policy references an assert that was registered by the host
/// (e.g. `resource_is_package_local`), the evaluator looks up the
/// callback in this context and invokes it.
#[derive(Clone, Default)]
pub struct EvalContext {
    host_asserts: HashMap<String, HostAssertFn>,
}

impl EvalContext {
    /// Create an empty evaluation context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a host-provided assert callback.
    pub fn register(
        &mut self,
        name: impl Into<String>,
        f: impl Fn(&Value) -> bool + Send + Sync + 'static,
    ) {
        self.host_asserts.insert(name.into(), Arc::new(f));
    }

    /// Look up a host assert by name.
    pub fn get(&self, name: &str) -> Option<&HostAssertFn> {
        self.host_asserts.get(name)
    }

    /// Check if any host asserts are registered.
    pub fn is_empty(&self) -> bool {
        self.host_asserts.is_empty()
    }

    /// Get the set of registered host assert names.
    pub fn names(&self) -> std::collections::HashSet<String> {
        self.host_asserts.keys().cloned().collect()
    }
}

impl std::fmt::Debug for EvalContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EvalContext")
            .field(
                "host_asserts",
                &self.host_asserts.keys().collect::<Vec<_>>(),
            )
            .finish()
    }
}

/// Comparison operators for conditions.
#[derive(Debug, Clone, PartialEq)]
pub enum Operator {
    /// Exact equality (pattern match).
    Eq,
    /// Not equal.
    Ne,
    /// Less than (numeric).
    Lt,
    /// Greater than (numeric).
    Gt,
    /// Less than or equal (numeric).
    Le,
    /// Greater than or equal (numeric).
    Ge,
    /// Collection search: ANY element matches.
    In,
    /// Inverse collection search: NO element matches.
    NotIn,
    /// Universal quantifier: ALL elements match.
    ForAll,
    /// Existential quantifier: at least one element matches.
    Exists,
    /// Attribute existence check: path resolves to a non-null value.
    Has,
    /// Glob pattern matching (Cedar 'like'): * matches any character sequence.
    Like,
    /// Array containsAll: all elements in the pattern array must exist in the data array.
    ContainsAll,
    /// Array containsAny: at least one element in the pattern array must exist in the data array.
    ContainsAny,
    /// Negated containsAll: at least one pattern element is missing from data array.
    NotContainsAll,
    /// Negated containsAny: none of the pattern elements exist in the data array.
    NotContainsAny,
    // ── Cedar extension function operators ──
    /// ip(path).isInRange(ip("cidr")) - IP address in CIDR range check
    IpIsInRange,
    /// ip(path).isIpv4() - true if valid IPv4 address
    IsIpv4,
    /// ip(path).isIpv6() - true if valid IPv6 address (not a v4-mapped v6)
    IsIpv6,
    /// ip(path).isLoopback() - true if loopback address
    IsLoopback,
    /// ip(path).isMulticast() - true if multicast address
    IsMulticast,
    /// decimal comparison: decimal(path).lessThan(decimal("v"))
    DecimalLt,
    /// decimal comparison: decimal(path).lessThanOrEqual(decimal("v"))
    DecimalLe,
    /// decimal comparison: decimal(path).greaterThan(decimal("v"))
    DecimalGt,
    /// decimal comparison: decimal(path).greaterThanOrEqual(decimal("v"))
    DecimalGe,
}

/// Simple glob matching: `*` matches any sequence of characters, everything else is literal.
/// This implements Cedar's `like` semantics.
fn glob_match(text: &str, pattern: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();

    // No wildcards - must be exact match
    if parts.len() == 1 {
        return text == pattern;
    }

    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if let Some(found) = text[pos..].find(part) {
            // First part must match at the start
            if i == 0 && found != 0 {
                return false;
            }
            pos += found + part.len();
        } else {
            return false;
        }
    }

    // Last part must match at the end (unless pattern ends with *)
    if let Some(last) = parts.last() {
        if !last.is_empty() && !text.ends_with(last) {
            return false;
        }
    }

    true
}

// ── IP address helpers (Cedar extension functions) ──

/// Parse an IP address string (IPv4 or IPv6).
fn parse_ip(s: &str) -> Option<std::net::IpAddr> {
    s.parse::<std::net::IpAddr>().ok()
}

/// Check if an IP string represents a valid IPv4 address.
fn is_ipv4(s: &str) -> bool {
    s.parse::<std::net::Ipv4Addr>().is_ok()
}

/// Check if an IP string represents a valid IPv6 address (not v4-mapped).
fn is_ipv6(s: &str) -> bool {
    s.parse::<std::net::Ipv6Addr>().is_ok() && !is_ipv4(s)
}

/// Check if an IP string is a loopback address (127.0.0.0/8 or ::1).
fn is_loopback(s: &str) -> bool {
    parse_ip(s).is_some_and(|ip| ip.is_loopback())
}

/// Check if an IP string is a multicast address (224.0.0.0/4 or ff00::/8).
fn is_multicast(s: &str) -> bool {
    parse_ip(s).is_some_and(|ip| match ip {
        std::net::IpAddr::V4(v4) => v4.is_multicast(),
        std::net::IpAddr::V6(v6) => v6.is_multicast(),
    })
}

/// Check if an IP address is within a CIDR range (e.g., "10.0.0.0/8").
fn ip_in_range(ip_str: &str, cidr_str: &str) -> bool {
    let ip = match parse_ip(ip_str) {
        Some(ip) => ip,
        None => return false,
    };

    // Parse CIDR: "network/prefix"
    let parts: Vec<&str> = cidr_str.splitn(2, '/').collect();
    if parts.len() != 2 {
        return false;
    }
    let network = match parts[0].parse::<std::net::IpAddr>() {
        Ok(n) => n,
        Err(_) => return false,
    };
    let prefix_len: u32 = match parts[1].parse() {
        Ok(p) => p,
        Err(_) => return false,
    };

    match (ip, network) {
        (std::net::IpAddr::V4(ip4), std::net::IpAddr::V4(net4)) => {
            if prefix_len > 32 {
                return false;
            }
            if prefix_len == 0 {
                return true;
            }
            let mask = !0u32 << (32 - prefix_len);
            let ip_bits = u32::from(ip4);
            let net_bits = u32::from(net4);
            (ip_bits & mask) == (net_bits & mask)
        }
        (std::net::IpAddr::V6(ip6), std::net::IpAddr::V6(net6)) => {
            if prefix_len > 128 {
                return false;
            }
            if prefix_len == 0 {
                return true;
            }
            let ip_bits = u128::from(ip6);
            let net_bits = u128::from(net6);
            let mask = !0u128 << (128 - prefix_len);
            (ip_bits & mask) == (net_bits & mask)
        }
        _ => false, // mixed v4/v6
    }
}

// ── Decimal comparison helper ──

/// Compare a data value (string like "1.23" or number) against a pattern string decimal.
fn decimal_cmp(data: &Value, pattern: &Pattern, cmp: impl Fn(f64, f64) -> bool) -> bool {
    let data_num = if let Some(n) = data.as_f64() {
        n
    } else if let Some(s) = data.as_str() {
        match s.parse::<f64>() {
            Ok(n) => n,
            Err(_) => return false,
        }
    } else {
        return false;
    };

    if let Pattern::Literal(Value::String(pat_str)) = pattern {
        if let Ok(pat_num) = pat_str.parse::<f64>() {
            return cmp(data_num, pat_num);
        }
    } else if let Pattern::Literal(lit) = pattern {
        if let Some(pat_num) = as_number(lit) {
            return cmp(data_num, pat_num);
        }
    }
    false
}

/// Extract a numeric value from JSON for comparison.
fn as_number(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_i64().map(|i| i as f64))
}

/// Compare two JSON values numerically.
fn compare_numbers(data: &Value, pattern: &Pattern, cmp: impl Fn(f64, f64) -> bool) -> bool {
    let data_num = match as_number(data) {
        Some(n) => n,
        None => return false,
    };

    // Pattern must be a literal number
    if let Pattern::Literal(lit) = pattern {
        if let Some(pat_num) = as_number(lit) {
            return cmp(data_num, pat_num);
        }
    }
    false
}

/// Quantifier mode for exists/forall conditions.
#[derive(Debug, Clone, PartialEq)]
pub enum QuantifierMode {
    /// exists var in path: condition - ANY match succeeds
    Exists,
    /// forall var in path: condition - ALL must match
    ForAll,
}

/// Information for quantified conditions (exists/forall with variable binding).
#[derive(Debug, Clone)]
pub struct QuantifierInfo {
    /// The quantifier mode (exists or forall).
    pub mode: QuantifierMode,
    /// The variable name bound during iteration.
    pub var: String,
    /// The path to iterate over.
    pub source_path: Path,
    /// The body condition expression to evaluate with bindings.
    pub body: Box<ConditionExpr>,
}

/// A single condition within a rule.
#[derive(Debug, Clone)]
pub struct Condition {
    /// Path to traverse to get the data.
    pub path: Path,
    /// Operator to apply.
    pub op: Operator,
    /// Pattern to match against.
    pub pattern: Pattern,
    /// Optional quantifier info for exists/forall with variable binding.
    pub quantifier: Option<QuantifierInfo>,
}

impl Condition {
    /// Create a new condition.
    pub fn new(path: impl Into<Path>, op: Operator, pattern: Pattern) -> Self {
        Self {
            path: path.into(),
            op,
            pattern,
            quantifier: None,
        }
    }

    /// Shorthand: `path == pattern`
    pub fn eq(path: impl Into<Path>, pattern: Pattern) -> Self {
        Self::new(path, Operator::Eq, pattern)
    }

    /// Shorthand: `path != pattern`
    pub fn ne(path: impl Into<Path>, pattern: Pattern) -> Self {
        Self::new(path, Operator::Ne, pattern)
    }

    /// Shorthand: `pattern in path` (any element matches)
    pub fn contains(path: impl Into<Path>, pattern: Pattern) -> Self {
        Self::new(path, Operator::In, pattern)
    }

    /// Shorthand: `pattern not in path` (no element matches)
    pub fn not_contains(path: impl Into<Path>, pattern: Pattern) -> Self {
        Self::new(path, Operator::NotIn, pattern)
    }

    /// Shorthand: `forall item in path: pattern(item)` (all elements match)
    pub fn for_all(path: impl Into<Path>, pattern: Pattern) -> Self {
        Self::new(path, Operator::ForAll, pattern)
    }

    /// Shorthand: `path < value`
    pub fn lt(path: impl Into<Path>, value: impl Into<Value>) -> Self {
        Self::new(path, Operator::Lt, Pattern::Literal(value.into()))
    }

    /// Shorthand: `path > value`
    pub fn gt(path: impl Into<Path>, value: impl Into<Value>) -> Self {
        Self::new(path, Operator::Gt, Pattern::Literal(value.into()))
    }

    /// Shorthand: `path <= value`
    pub fn le(path: impl Into<Path>, value: impl Into<Value>) -> Self {
        Self::new(path, Operator::Le, Pattern::Literal(value.into()))
    }

    /// Shorthand: `path >= value`
    pub fn ge(path: impl Into<Path>, value: impl Into<Value>) -> Self {
        Self::new(path, Operator::Ge, Pattern::Literal(value.into()))
    }

    /// Evaluate this condition against input data.
    #[inline]
    pub fn evaluate(&self, input: &Value) -> bool {
        // Fast path: skip bindings machinery for non-quantified conditions (the common case)
        if self.quantifier.is_none() {
            return self.evaluate_fast(input);
        }
        self.evaluate_with_bindings(input, &std::collections::HashMap::new())
    }

    /// Evaluate this condition with variable bindings.
    pub fn evaluate_with_bindings(
        &self,
        input: &Value,
        bindings: &std::collections::HashMap<String, &Value>,
    ) -> bool {
        // Handle quantified conditions (exists/forall with variable binding)
        if let Some(ref quant) = self.quantifier {
            let source_arr = match quant.source_path.resolve_with_bindings(input, bindings) {
                Some(v) => v,
                None => return false,
            };

            let arr = match source_arr.as_array() {
                Some(a) => a,
                None => return false,
            };

            match quant.mode {
                QuantifierMode::Exists => {
                    // ANY item makes body conditions pass
                    arr.iter().any(|item| {
                        let mut new_bindings = bindings.clone();
                        new_bindings.insert(quant.var.clone(), item);
                        quant.body.evaluate_with_bindings(input, &new_bindings)
                    })
                }
                QuantifierMode::ForAll => {
                    // ALL items must make body conditions pass
                    arr.iter().all(|item| {
                        let mut new_bindings = bindings.clone();
                        new_bindings.insert(quant.var.clone(), item);
                        quant.body.evaluate_with_bindings(input, &new_bindings)
                    })
                }
            }
        } else {
            self.evaluate_simple(input, bindings)
        }
    }

    /// Fast evaluation path for the common case (no quantifiers, no bindings).
    ///
    /// Avoids HashMap creation, pattern cloning, and full matcher dispatch.
    #[inline]
    fn evaluate_fast(&self, input: &Value) -> bool {
        let data = match self.path.resolve_fast(input) {
            Some(d) => d,
            None => return false,
        };

        // For PathRef patterns, fall back to the full path (rare case)
        if let Pattern::PathRef(_) = &self.pattern {
            return self.evaluate_simple(input, &std::collections::HashMap::new());
        }

        self.dispatch_op(data, &self.pattern, input)
    }

    /// Dispatch the operator comparison. Works with a pattern by reference (no clone).
    #[inline]
    fn dispatch_op(&self, data: &Value, pattern: &Pattern, _input: &Value) -> bool {
        match self.op {
            // Fast path: Literal Eq/Ne bypasses full matcher, just compare Values directly
            Operator::Eq => {
                if let Pattern::Literal(lit) = pattern {
                    data == lit
                } else {
                    matches_ref(data, pattern)
                }
            }
            Operator::Ne => {
                if let Pattern::Literal(lit) = pattern {
                    data != lit
                } else {
                    !matches_ref(data, pattern)
                }
            }
            Operator::Lt => compare_numbers(data, pattern, |a, b| a < b),
            Operator::Gt => compare_numbers(data, pattern, |a, b| a > b),
            Operator::Le => compare_numbers(data, pattern, |a, b| a <= b),
            Operator::Ge => compare_numbers(data, pattern, |a, b| a >= b),
            Operator::In => {
                // Fast path: scalar membership in literal array (e.g., role in ["admin", "editor"])
                // Avoids all matcher/bindings allocations.
                if let Pattern::Literal(Value::Array(arr)) = pattern {
                    return arr.iter().any(|item| item == data);
                }
                any_matches(data, pattern)
            }
            Operator::NotIn => {
                if let Pattern::Literal(Value::Array(arr)) = pattern {
                    return !arr.iter().any(|item| item == data);
                }
                !any_matches(data, pattern)
            }
            Operator::ForAll => all_match(data, pattern),
            Operator::Exists => any_matches(data, pattern),
            // Has: if we reach dispatch_op, the path already resolved, so the attribute exists.
            // This is true even for null values - {field: null} means the field EXISTS.
            Operator::Has => true,
            Operator::Like => {
                if let (Some(text), Pattern::Literal(Value::String(pat))) = (data.as_str(), pattern)
                {
                    glob_match(text, pat)
                } else {
                    false
                }
            }
            Operator::ContainsAll => {
                if let (Some(data_arr), Pattern::Literal(Value::Array(pat_arr))) =
                    (data.as_array(), pattern)
                {
                    pat_arr.iter().all(|p| data_arr.contains(p))
                } else {
                    false
                }
            }
            Operator::ContainsAny => {
                if let (Some(data_arr), Pattern::Literal(Value::Array(pat_arr))) =
                    (data.as_array(), pattern)
                {
                    pat_arr.iter().any(|p| data_arr.contains(p))
                } else {
                    false
                }
            }
            Operator::NotContainsAll => {
                if let (Some(data_arr), Pattern::Literal(Value::Array(pat_arr))) =
                    (data.as_array(), pattern)
                {
                    !pat_arr.iter().all(|p| data_arr.contains(p))
                } else {
                    true // if data isn't an array, it certainly doesn't contain all
                }
            }
            Operator::NotContainsAny => {
                if let (Some(data_arr), Pattern::Literal(Value::Array(pat_arr))) =
                    (data.as_array(), pattern)
                {
                    !pat_arr.iter().any(|p| data_arr.contains(p))
                } else {
                    true
                }
            }
            // ── Extension function operators ──
            Operator::IpIsInRange => {
                if let (Some(ip_str), Pattern::Literal(Value::String(cidr_str))) =
                    (data.as_str(), pattern)
                {
                    ip_in_range(ip_str, cidr_str)
                } else {
                    false
                }
            }
            Operator::IsIpv4 => data.as_str().is_some_and(is_ipv4),
            Operator::IsIpv6 => data.as_str().is_some_and(is_ipv6),
            Operator::IsLoopback => data.as_str().is_some_and(is_loopback),
            Operator::IsMulticast => data.as_str().is_some_and(is_multicast),
            Operator::DecimalLt => decimal_cmp(data, pattern, |a, b| a < b),
            Operator::DecimalLe => decimal_cmp(data, pattern, |a, b| a <= b),
            Operator::DecimalGt => decimal_cmp(data, pattern, |a, b| a > b),
            Operator::DecimalGe => decimal_cmp(data, pattern, |a, b| a >= b),
        }
    }

    /// Simple condition evaluation (non-quantified, with bindings support).
    fn evaluate_simple(
        &self,
        input: &Value,
        bindings: &std::collections::HashMap<String, &Value>,
    ) -> bool {
        let data = match self.path.resolve_with_bindings(input, bindings) {
            Some(d) => d,
            None => return false, // Path doesn't exist
        };

        // Handle PathRef patterns: resolve the referenced path
        if let Pattern::PathRef(ref_path) = &self.pattern {
            if matches!(self.op, Operator::In | Operator::NotIn) {
                // PathRef + In/NotIn: one side is the needle, one is the haystack (array).
                // DSL compiles as: Condition(path=haystack, In, PathRef(needle))
                // Rust API may use: Condition(path=needle, In, PathRef(haystack))
                // We detect which side is the array and do the check accordingly.
                let ref_val = match ref_path.resolve_with_bindings(input, bindings) {
                    Some(val) => val,
                    None => return false,
                };
                let found = match (data.as_array(), ref_val.as_array()) {
                    // data is array, ref_val is needle (DSL ordering)
                    (Some(arr), _) => arr.iter().any(|item| item == ref_val),
                    // ref_val is array, data is needle (Rust API ordering)
                    (_, Some(arr)) => arr.iter().any(|item| item == data),
                    // Neither is array
                    _ => data == ref_val,
                };
                return match self.op {
                    Operator::In => found,
                    Operator::NotIn => !found,
                    _ => unreachable!(),
                };
            }
            // For other operators with PathRef: resolve and create a temporary Literal
            let resolved = match ref_path.resolve_with_bindings(input, bindings) {
                Some(val) => Pattern::Literal(val.clone()),
                None => return false,
            };
            return self.dispatch_op(data, &resolved, input);
        }

        // Non-PathRef: use the pattern by reference (no clone!)
        self.dispatch_op(data, &self.pattern, input)
    }
}

/// A condition expression tree supporting AND, OR, NOT, and leaf conditions.
///
/// This enables complex nested boolean logic like:
/// `(action == "read" or action == "write") and not (resource == "secret")`
#[derive(Debug, Clone)]
pub enum ConditionExpr {
    /// A single leaf condition.
    Leaf(Condition),
    /// All sub-expressions must be true.
    And(Vec<ConditionExpr>),
    /// At least one sub-expression must be true.
    Or(Vec<ConditionExpr>),
    /// Negation of a sub-expression.
    Not(Box<ConditionExpr>),
    /// Structural type check: `path is TypeShape`.
    /// Fingerprints the value at `path` and checks conformance.
    IsType { path: Path, shape: TypeShape },
    /// Host-provided assert - resolved at evaluation time via callback.
    ///
    /// The string is the assert name (e.g. `"resource_is_package_local"`).
    /// When evaluated without an `EvalContext`, defaults to `false`.
    HostAssert(String),
}

impl ConditionExpr {
    /// Evaluate the condition expression against input data.
    #[inline]
    pub fn evaluate(&self, input: &Value) -> bool {
        match self {
            ConditionExpr::Leaf(c) => c.evaluate(input),
            ConditionExpr::And(exprs) => exprs.iter().all(|e| e.evaluate(input)),
            ConditionExpr::Or(exprs) => exprs.iter().any(|e| e.evaluate(input)),
            ConditionExpr::Not(expr) => !expr.evaluate(input),
            ConditionExpr::IsType { path, shape } => match path.resolve(input) {
                Some(value) => {
                    let val_shape = fingerprint_value(value);
                    val_shape.conforms_to(shape)
                }
                None => false,
            },
            ConditionExpr::HostAssert(_) => false, // no context → default false
        }
    }

    /// Evaluate with host assert context.
    ///
    /// When a `HostAssert` node is reached, the callback registered in
    /// the `EvalContext` is invoked with the input data.
    pub fn evaluate_with_context(&self, input: &Value, ctx: &EvalContext) -> bool {
        match self {
            ConditionExpr::Leaf(c) => c.evaluate(input),
            ConditionExpr::And(exprs) => exprs.iter().all(|e| e.evaluate_with_context(input, ctx)),
            ConditionExpr::Or(exprs) => exprs.iter().any(|e| e.evaluate_with_context(input, ctx)),
            ConditionExpr::Not(expr) => !expr.evaluate_with_context(input, ctx),
            ConditionExpr::IsType { path, shape } => match path.resolve(input) {
                Some(value) => fingerprint_value(value).conforms_to(shape),
                None => false,
            },
            ConditionExpr::HostAssert(name) => ctx.get(name).map_or(false, |f| f(input)),
        }
    }

    /// Evaluate with variable bindings (for quantifiers).
    pub fn evaluate_with_bindings(
        &self,
        input: &Value,
        bindings: &std::collections::HashMap<String, &Value>,
    ) -> bool {
        match self {
            ConditionExpr::Leaf(c) => c.evaluate_with_bindings(input, bindings),
            ConditionExpr::And(exprs) => exprs
                .iter()
                .all(|e| e.evaluate_with_bindings(input, bindings)),
            ConditionExpr::Or(exprs) => exprs
                .iter()
                .any(|e| e.evaluate_with_bindings(input, bindings)),
            ConditionExpr::Not(expr) => !expr.evaluate_with_bindings(input, bindings),
            ConditionExpr::IsType { path, shape } => match path.resolve(input) {
                Some(value) => fingerprint_value(value).conforms_to(shape),
                None => false,
            },
            ConditionExpr::HostAssert(_) => false, // no context available in bindings path
        }
    }

    /// Construct from a flat list of conditions (implicit AND, for backwards compat).
    pub fn from_conditions(conditions: Vec<Condition>) -> Option<Self> {
        match conditions.len() {
            0 => None,
            1 => Some(ConditionExpr::Leaf(conditions.into_iter().next().unwrap())),
            _ => Some(ConditionExpr::And(
                conditions.into_iter().map(ConditionExpr::Leaf).collect(),
            )),
        }
    }
}

/// The effect of a rule when it matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Allow,
    Deny,
}

/// A complete rule with a name, body expression, and an effect.
#[derive(Debug, Clone)]
pub struct Rule {
    pub name: String,
    pub effect: Effect,
    /// The condition body (None = unconditional match).
    pub body: Option<ConditionExpr>,
}

impl Rule {
    /// Create a new rule from a flat list of conditions (AND semantics).
    pub fn new(name: impl Into<String>, effect: Effect, conditions: Vec<Condition>) -> Self {
        Self {
            name: name.into(),
            effect,
            body: ConditionExpr::from_conditions(conditions),
        }
    }

    /// Create a new rule with a condition expression body.
    pub fn with_body(name: impl Into<String>, effect: Effect, body: Option<ConditionExpr>) -> Self {
        Self {
            name: name.into(),
            effect,
            body,
        }
    }

    /// Create an allow rule.
    pub fn allow(name: impl Into<String>, conditions: Vec<Condition>) -> Self {
        Self::new(name, Effect::Allow, conditions)
    }

    /// Create a deny rule.
    pub fn deny(name: impl Into<String>, conditions: Vec<Condition>) -> Self {
        Self::new(name, Effect::Deny, conditions)
    }

    /// Evaluate the rule against input data.
    ///
    /// Returns `Some(effect)` if all conditions pass, `None` otherwise.
    #[inline]
    pub fn evaluate(&self, input: &Value) -> Option<Effect> {
        let matched = match &self.body {
            Some(body) => body.evaluate(input),
            None => true, // unconditional
        };
        if matched {
            Some(self.effect)
        } else {
            None
        }
    }

    /// Evaluate with host assert context.
    pub fn evaluate_with_context(&self, input: &Value, ctx: &EvalContext) -> Option<Effect> {
        let matched = match &self.body {
            Some(body) => body.evaluate_with_context(input, ctx),
            None => true,
        };
        if matched {
            Some(self.effect)
        } else {
            None
        }
    }
}

/// A policy is a collection of rules.
///
/// Evaluation follows "deny overrides" semantics:
/// - If ANY deny rule matches → Deny
/// - Else if ANY allow rule matches → Allow
/// - Else → Default deny
#[derive(Debug, Clone, Default)]
pub struct Policy {
    pub rules: Vec<Rule>,
}

impl Policy {
    /// Create an empty policy.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a rule to the policy.
    pub fn add_rule(&mut self, rule: Rule) {
        self.rules.push(rule);
    }

    /// Builder pattern: add a rule and return self.
    pub fn with_rule(mut self, rule: Rule) -> Self {
        self.add_rule(rule);
        self
    }

    /// Evaluate the policy against input data.
    ///
    /// Uses "deny overrides" semantics.
    #[inline]
    pub fn evaluate(&self, input: &Value) -> Effect {
        let mut has_allow = false;

        for rule in &self.rules {
            if let Some(effect) = rule.evaluate(input) {
                match effect {
                    Effect::Deny => return Effect::Deny, // Deny overrides
                    Effect::Allow => has_allow = true,
                }
            }
        }

        if has_allow {
            Effect::Allow
        } else {
            Effect::Deny // Default deny
        }
    }

    /// Evaluate the policy against input data with host assert context.
    pub fn evaluate_with_context(&self, input: &Value, ctx: &EvalContext) -> Effect {
        let mut has_allow = false;

        for rule in &self.rules {
            if let Some(effect) = rule.evaluate_with_context(input, ctx) {
                match effect {
                    Effect::Deny => return Effect::Deny,
                    Effect::Allow => has_allow = true,
                }
            }
        }

        if has_allow {
            Effect::Allow
        } else {
            Effect::Deny
        }
    }

    /// Evaluate multiple requests in a single batch.
    ///
    /// Returns results in the same order as inputs.
    ///
    /// # Example
    ///
    /// ```rust
    /// use karu::compile;
    /// use karu::rule::Effect;
    /// use serde_json::json;
    ///
    /// let policy = compile(r#"allow access if role == "admin";"#).unwrap();
    /// let requests = vec![
    ///     json!({"role": "admin"}),
    ///     json!({"role": "user"}),
    /// ];
    /// let results = policy.evaluate_batch(&requests);
    /// assert_eq!(results, vec![Effect::Allow, Effect::Deny]);
    /// ```
    pub fn evaluate_batch(&self, inputs: &[Value]) -> Vec<Effect> {
        inputs.iter().map(|input| self.evaluate(input)).collect()
    }

    /// Evaluate multiple requests in parallel using rayon.
    ///
    /// Requires the `parallel` feature.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Requires: cargo add karu --features parallel
    /// let results = policy.evaluate_parallel(&requests);
    /// ```
    #[cfg(feature = "parallel")]
    pub fn evaluate_parallel(&self, inputs: &[Value]) -> Vec<Effect> {
        use rayon::prelude::*;
        inputs
            .par_iter()
            .map(|input| self.evaluate(input))
            .collect()
    }
}

/// An indexed policy for faster evaluation.
///
/// Pre-separates deny and allow rules at construction time, avoiding
/// the need to check effect types during evaluation.
///
/// # Example
///
/// ```rust
/// use karu::compile;
/// use karu::rule::{Effect, IndexedPolicy};
/// use serde_json::json;
///
/// let policy = compile(r#"allow access if role == "admin";"#).unwrap();
/// let indexed = IndexedPolicy::from(policy);
///
/// assert_eq!(indexed.evaluate(&json!({"role": "admin"})), Effect::Allow);
/// ```
#[derive(Debug, Clone)]
pub struct IndexedPolicy {
    deny_rules: Vec<Rule>,
    allow_rules: Vec<Rule>,
}

impl IndexedPolicy {
    /// Create an indexed policy from rules.
    pub fn new(rules: Vec<Rule>) -> Self {
        let mut deny_rules = Vec::new();
        let mut allow_rules = Vec::new();

        for rule in rules {
            match rule.effect {
                Effect::Deny => deny_rules.push(rule),
                Effect::Allow => allow_rules.push(rule),
            }
        }

        Self {
            deny_rules,
            allow_rules,
        }
    }

    /// Evaluate the policy against input data.
    ///
    /// Uses "deny overrides" semantics with optimized rule ordering.
    #[inline]
    pub fn evaluate(&self, input: &Value) -> Effect {
        // Check deny rules first (early exit on deny)
        for rule in &self.deny_rules {
            if rule.evaluate(input) == Some(Effect::Deny) {
                return Effect::Deny;
            }
        }

        // Check allow rules
        for rule in &self.allow_rules {
            if rule.evaluate(input) == Some(Effect::Allow) {
                return Effect::Allow;
            }
        }

        Effect::Deny // Default deny
    }

    /// Evaluate multiple requests in a single batch.
    pub fn evaluate_batch(&self, inputs: &[Value]) -> Vec<Effect> {
        inputs.iter().map(|input| self.evaluate(input)).collect()
    }

    /// Evaluate multiple requests in parallel using rayon.
    #[cfg(feature = "parallel")]
    pub fn evaluate_parallel(&self, inputs: &[Value]) -> Vec<Effect> {
        use rayon::prelude::*;
        inputs
            .par_iter()
            .map(|input| self.evaluate(input))
            .collect()
    }
}

impl From<Policy> for IndexedPolicy {
    fn from(policy: Policy) -> Self {
        Self::new(policy.rules)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ========== Condition Tests ==========

    #[test]
    fn test_condition_eq() {
        let cond = Condition::eq("action", Pattern::literal("read"));
        assert!(cond.evaluate(&json!({"action": "read"})));
        assert!(!cond.evaluate(&json!({"action": "write"})));
    }

    #[test]
    fn test_condition_ne() {
        let cond = Condition::ne("action", Pattern::literal("read"));
        assert!(!cond.evaluate(&json!({"action": "read"})));
        assert!(cond.evaluate(&json!({"action": "write"})));
    }

    #[test]
    fn test_condition_in() {
        let cond = Condition::contains(
            "permissions",
            Pattern::object([("name", Pattern::literal("admin"))]),
        );
        let data = json!({
            "permissions": [
                {"name": "user"},
                {"name": "admin", "level": 10}
            ]
        });
        assert!(cond.evaluate(&data));
    }

    #[test]
    fn test_condition_not_in() {
        let cond = Condition::not_contains("blocked", Pattern::literal("alice"));
        assert!(cond.evaluate(&json!({"blocked": ["bob", "charlie"]})));
        assert!(!cond.evaluate(&json!({"blocked": ["alice", "bob"]})));
    }

    #[test]
    fn test_condition_for_all() {
        let cond = Condition::for_all(
            "users",
            Pattern::object([("verified", Pattern::literal(true))]),
        );
        assert!(cond.evaluate(&json!({
            "users": [
                {"verified": true, "name": "a"},
                {"verified": true, "name": "b"}
            ]
        })));
        assert!(!cond.evaluate(&json!({
            "users": [
                {"verified": true},
                {"verified": false}
            ]
        })));
    }

    #[test]
    fn test_condition_missing_path() {
        let cond = Condition::eq("nonexistent.path", Pattern::literal(42));
        assert!(!cond.evaluate(&json!({"other": "data"})));
    }

    // ========== Comparison Operator Tests ==========

    #[test]
    fn test_condition_lt() {
        let cond = Condition::lt("age", 18);
        assert!(cond.evaluate(&json!({"age": 17})));
        assert!(!cond.evaluate(&json!({"age": 18})));
        assert!(!cond.evaluate(&json!({"age": 19})));
    }

    #[test]
    fn test_condition_gt() {
        let cond = Condition::gt("score", 100);
        assert!(!cond.evaluate(&json!({"score": 99})));
        assert!(!cond.evaluate(&json!({"score": 100})));
        assert!(cond.evaluate(&json!({"score": 101})));
    }

    #[test]
    fn test_condition_le() {
        let cond = Condition::le("count", 10);
        assert!(cond.evaluate(&json!({"count": 9})));
        assert!(cond.evaluate(&json!({"count": 10})));
        assert!(!cond.evaluate(&json!({"count": 11})));
    }

    #[test]
    fn test_condition_ge() {
        let cond = Condition::ge("level", 5);
        assert!(!cond.evaluate(&json!({"level": 4})));
        assert!(cond.evaluate(&json!({"level": 5})));
        assert!(cond.evaluate(&json!({"level": 6})));
    }

    #[test]
    fn test_condition_comparison_with_floats() {
        let cond = Condition::gt("price", 99.99);
        assert!(cond.evaluate(&json!({"price": 100.0})));
        assert!(!cond.evaluate(&json!({"price": 99.99})));
    }

    #[test]
    fn test_condition_comparison_non_numeric() {
        // Should fail gracefully when comparing non-numeric values
        let cond = Condition::lt("name", 10);
        assert!(!cond.evaluate(&json!({"name": "alice"})));
    }

    // ========== Rule Tests ==========

    #[test]
    fn test_rule_all_conditions_must_pass() {
        let rule = Rule::allow(
            "admin_read",
            vec![
                Condition::eq("principal.role", Pattern::literal("admin")),
                Condition::eq("action", Pattern::literal("read")),
            ],
        );

        // Both pass
        assert_eq!(
            rule.evaluate(&json!({
                "principal": {"role": "admin"},
                "action": "read"
            })),
            Some(Effect::Allow)
        );

        // Only one passes
        assert_eq!(
            rule.evaluate(&json!({
                "principal": {"role": "user"},
                "action": "read"
            })),
            None
        );
    }

    #[test]
    fn test_rule_empty_conditions_always_matches() {
        let rule = Rule::allow("allow_all", vec![]);
        assert_eq!(
            rule.evaluate(&json!({"anything": "here"})),
            Some(Effect::Allow)
        );
    }

    // ========== Policy Tests ==========

    #[test]
    fn test_policy_deny_overrides() {
        let policy = Policy::new()
            .with_rule(Rule::allow(
                "allow_users",
                vec![Condition::eq("principal", Pattern::literal("alice"))],
            ))
            .with_rule(Rule::deny(
                "deny_dangerous",
                vec![Condition::eq("action", Pattern::literal("delete"))],
            ));

        // Allow wins when no deny
        assert_eq!(
            policy.evaluate(&json!({"principal": "alice", "action": "read"})),
            Effect::Allow
        );

        // Deny overrides allow
        assert_eq!(
            policy.evaluate(&json!({"principal": "alice", "action": "delete"})),
            Effect::Deny
        );
    }

    #[test]
    fn test_policy_default_deny() {
        let policy = Policy::new().with_rule(Rule::allow(
            "only_admin",
            vec![Condition::eq("role", Pattern::literal("admin"))],
        ));

        // No matching rules → default deny
        assert_eq!(policy.evaluate(&json!({"role": "user"})), Effect::Deny);
    }

    #[test]
    fn test_policy_readme_scenario() {
        // Recreate the README example as a policy test
        let policy = Policy::new().with_rule(Rule::allow(
            "allow_call_with_lhs_arg",
            vec![
                Condition::eq("action", Pattern::literal("call")),
                Condition::contains(
                    "resource.context.namedArguments",
                    Pattern::object([
                        ("name", Pattern::literal("lhs")),
                        ("value", Pattern::literal(10)),
                    ]),
                ),
            ],
        ));

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

        // Missing the lhs argument
        let input_no_lhs = json!({
            "principal": "alice",
            "action": "call",
            "resource": {
                "context": {
                    "namedArguments": [
                        {"name": "other", "value": 10}
                    ]
                }
            }
        });

        assert_eq!(policy.evaluate(&input_no_lhs), Effect::Deny);
    }

    #[test]
    fn test_complex_permission_check() {
        // Role-based access with capability search
        let policy = Policy::new()
            .with_rule(Rule::deny(
                "deny_blocked_users",
                vec![Condition::contains(
                    "blocked_users",
                    Pattern::var("principal"), // Matches any principal in blocked list
                )],
            ))
            .with_rule(Rule::allow(
                "allow_with_capability",
                vec![Condition::contains(
                    "principal.capabilities",
                    Pattern::object([
                        ("action", Pattern::literal("write")),
                        ("resource", Pattern::literal("/data/*")),
                    ]),
                )],
            ));

        let alice = json!({
            "principal": {
                "name": "alice",
                "capabilities": [
                    {"action": "read", "resource": "*"},
                    {"action": "write", "resource": "/data/*"}
                ]
            },
            "blocked_users": []
        });

        assert_eq!(policy.evaluate(&alice), Effect::Allow);
    }
}
