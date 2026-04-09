// SPDX-License-Identifier: MIT

//! Variable bindings for Karu's pattern matching.
//!
//! When patterns contain named variables (`Pattern::Variable(Some(name))`),
//! the matched values are captured into a `Bindings` context for later use.

use serde_json::Value;
use std::collections::HashMap;

/// A binding context that holds captured values from pattern matching.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bindings {
    values: HashMap<String, Value>,
}

impl Bindings {
    /// Create an empty binding context.
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind a value to a name.
    pub fn bind(&mut self, name: impl Into<String>, value: Value) {
        self.values.insert(name.into(), value);
    }

    /// Get a bound value by name.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.values.get(name)
    }

    /// Check if a name is bound.
    pub fn contains(&self, name: &str) -> bool {
        self.values.contains_key(name)
    }

    /// Get all bindings.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.values.iter()
    }

    /// Number of bindings.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Merge another bindings context into this one.
    /// Existing bindings are overwritten if there are conflicts.
    pub fn extend(&mut self, other: Bindings) {
        self.values.extend(other.values);
    }

    /// Create a new bindings context with values from both.
    pub fn merged(&self, other: &Bindings) -> Self {
        let mut result = self.clone();
        result.values.extend(other.values.clone());
        result
    }
}

impl From<HashMap<String, Value>> for Bindings {
    fn from(values: HashMap<String, Value>) -> Self {
        Self { values }
    }
}

impl IntoIterator for Bindings {
    type Item = (String, Value);
    type IntoIter = std::collections::hash_map::IntoIter<String, Value>;

    fn into_iter(self) -> Self::IntoIter {
        self.values.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_bindings_basic() {
        let mut bindings = Bindings::new();
        assert!(bindings.is_empty());

        bindings.bind("x", json!(42));
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings.get("x"), Some(&json!(42)));
        assert!(bindings.contains("x"));
        assert!(!bindings.contains("y"));
    }

    #[test]
    fn test_bindings_overwrite() {
        let mut bindings = Bindings::new();
        bindings.bind("x", json!(1));
        bindings.bind("x", json!(2));
        assert_eq!(bindings.get("x"), Some(&json!(2)));
    }

    #[test]
    fn test_bindings_merge() {
        let mut a = Bindings::new();
        a.bind("x", json!(1));
        a.bind("y", json!(2));

        let mut b = Bindings::new();
        b.bind("y", json!(20)); // Overwrites
        b.bind("z", json!(3));

        let merged = a.merged(&b);
        assert_eq!(merged.get("x"), Some(&json!(1)));
        assert_eq!(merged.get("y"), Some(&json!(20)));
        assert_eq!(merged.get("z"), Some(&json!(3)));
    }

    #[test]
    fn test_bindings_iter() {
        let mut bindings = Bindings::new();
        bindings.bind("a", json!(1));
        bindings.bind("b", json!(2));

        let keys: Vec<_> = bindings.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"a"));
        assert!(keys.contains(&"b"));
    }
}
