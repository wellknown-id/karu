// SPDX-License-Identifier: MIT

//! JSON path traversal for Karu.
//!
//! Allows navigating into nested JSON structures using dot-separated paths
//! like `resource.context.namedArguments`.

use serde_json::Value;
use std::collections::HashMap;

/// A path segment for traversing JSON structures.
#[derive(Debug, Clone, PartialEq)]
pub enum PathSegment {
    /// Access an object field by key.
    Field(String),
    /// Access an array element by index.
    Index(usize),
    /// Access using a variable's value (resolved at runtime from bindings).
    Variable(String),
}

/// A complete path through a JSON structure.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Path {
    segments: Vec<PathSegment>,
}

impl Path {
    /// Create an empty path (root).
    pub fn root() -> Self {
        Self::default()
    }

    /// Parse a dot-separated path string.
    ///
    /// Supports:
    /// - Field access: `foo.bar`
    /// - Array indexing: `items[0]` or `items.0`
    ///
    /// # Examples
    /// ```
    /// use karu::Path;
    ///
    /// let path = Path::parse("resource.context.namedArguments");
    /// assert_eq!(path.len(), 3);
    /// ```
    pub fn parse(s: &str) -> Self {
        if s.is_empty() {
            return Self::root();
        }

        let segments = s
            .split('.')
            .filter(|s| !s.is_empty())
            .flat_map(|segment| {
                // Check for array indexing: field[0]
                if let Some(bracket_pos) = segment.find('[') {
                    // This segment has both a field and an index
                    // For now, we just handle the simple case
                    if segment.ends_with(']') {
                        let field = &segment[..bracket_pos];
                        let idx_str = &segment[bracket_pos + 1..segment.len() - 1];
                        if let Ok(idx) = idx_str.parse::<usize>() {
                            return vec![
                                PathSegment::Field(field.to_string()),
                                PathSegment::Index(idx),
                            ];
                        }
                    }
                }

                // Check for numeric segment (array index)
                if let Ok(idx) = segment.parse::<usize>() {
                    vec![PathSegment::Index(idx)]
                } else {
                    vec![PathSegment::Field(segment.to_string())]
                }
            })
            .collect();

        Self { segments }
    }

    /// Create a path from segments.
    pub fn from_segments(segments: Vec<PathSegment>) -> Self {
        Self { segments }
    }

    /// Push a field access onto the path.
    pub fn field(mut self, name: impl Into<String>) -> Self {
        self.segments.push(PathSegment::Field(name.into()));
        self
    }

    /// Push an array index onto the path.
    pub fn index(mut self, idx: usize) -> Self {
        self.segments.push(PathSegment::Index(idx));
        self
    }

    /// Get the number of segments in the path.
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// Check if the path is empty (root).
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Get the segments of this path.
    pub fn segments(&self) -> &[PathSegment] {
        &self.segments
    }

    /// Traverse a JSON value using this path.
    ///
    /// Returns `None` if any segment cannot be resolved.
    pub fn resolve<'a>(&self, value: &'a Value) -> Option<&'a Value> {
        self.resolve_fast(value)
    }

    /// Fast path resolution without variable bindings.
    ///
    /// Avoids creating an empty HashMap for the common case where paths
    /// contain only Field and Index segments. Falls back to
    /// `resolve_with_bindings` when a Variable segment is encountered.
    #[inline]
    pub fn resolve_fast<'a>(&self, value: &'a Value) -> Option<&'a Value> {
        let mut current = value;

        for segment in &self.segments {
            current = match segment {
                PathSegment::Field(name) => current.get(name)?,
                PathSegment::Index(idx) => current.get(idx)?,
                PathSegment::Variable(_) => {
                    // Fall back to full resolver with empty bindings
                    return self.resolve_with_bindings(value, &HashMap::new());
                }
            };
        }

        Some(current)
    }

    /// Traverse a JSON value using this path, with variable bindings.
    ///
    /// Variables in the path are resolved using the bindings map.
    /// Supports special `@path:` prefix for resolving nested path expressions.
    pub fn resolve_with_bindings<'a>(
        &self,
        value: &'a Value,
        bindings: &HashMap<String, &'a Value>,
    ) -> Option<&'a Value> {
        let mut current = value;
        let mut used_binding = false;

        for (i, segment) in self.segments.iter().enumerate() {
            current = match segment {
                PathSegment::Field(name) => {
                    // If this is the first segment and the name matches a binding variable,
                    // use the bound value instead of field lookup. This handles paths like
                    // `item.approved` inside `forall item in resource.items: item.approved == true`.
                    if i == 0 && !used_binding {
                        if let Some(&bound) = bindings.get(name) {
                            used_binding = true;
                            bound
                        } else {
                            current.get(name)?
                        }
                    } else {
                        current.get(name)?
                    }
                }
                PathSegment::Index(idx) => current.get(idx)?,
                PathSegment::Variable(var_name) => {
                    // Check for @path: prefix indicating a path expression
                    if let Some(path_str) = var_name.strip_prefix("@path:") {
                        // Resolve the path expression against the root value
                        let inner_path = Path::parse(path_str);
                        let resolved = inner_path.resolve_with_bindings(value, bindings)?;
                        // Use the resolved value as a key
                        if let Some(key) = resolved.as_str() {
                            current.get(key)?
                        } else if let Some(idx) = resolved.as_u64() {
                            current.get(idx as usize)?
                        } else {
                            return None;
                        }
                    } else {
                        // Look up the variable value from bindings (for exists/forall)
                        let var_value = bindings.get(var_name)?;
                        // Use the variable value as either a string key or integer index
                        if let Some(key) = var_value.as_str() {
                            current.get(key)?
                        } else if let Some(idx) = var_value.as_u64() {
                            current.get(idx as usize)?
                        } else {
                            return None;
                        }
                    }
                }
            };
        }

        Some(current)
    }
}

impl From<&str> for Path {
    fn from(s: &str) -> Self {
        Path::parse(s)
    }
}

impl From<String> for Path {
    fn from(s: String) -> Self {
        Path::parse(&s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_parse_simple_path() {
        let path = Path::parse("foo.bar.baz");
        assert_eq!(path.len(), 3);
        assert_eq!(
            path.segments(),
            &[
                PathSegment::Field("foo".into()),
                PathSegment::Field("bar".into()),
                PathSegment::Field("baz".into()),
            ]
        );
    }

    #[test]
    fn test_parse_empty_path() {
        let path = Path::parse("");
        assert!(path.is_empty());
    }

    #[test]
    fn test_parse_with_numeric_index() {
        let path = Path::parse("items.0.name");
        assert_eq!(path.len(), 3);
        assert_eq!(
            path.segments(),
            &[
                PathSegment::Field("items".into()),
                PathSegment::Index(0),
                PathSegment::Field("name".into()),
            ]
        );
    }

    #[test]
    fn test_parse_with_bracket_index() {
        let path = Path::parse("items[0].name");
        assert_eq!(path.len(), 3);
        assert_eq!(
            path.segments(),
            &[
                PathSegment::Field("items".into()),
                PathSegment::Index(0),
                PathSegment::Field("name".into()),
            ]
        );
    }

    #[test]
    fn test_resolve_simple() {
        let data = json!({"a": {"b": {"c": 42}}});
        let path = Path::parse("a.b.c");
        assert_eq!(path.resolve(&data), Some(&json!(42)));
    }

    #[test]
    fn test_resolve_with_array() {
        let data = json!({
            "items": [
                {"name": "first"},
                {"name": "second"}
            ]
        });
        let path = Path::parse("items.0.name");
        assert_eq!(path.resolve(&data), Some(&json!("first")));

        let path2 = Path::parse("items.1.name");
        assert_eq!(path2.resolve(&data), Some(&json!("second")));
    }

    #[test]
    fn test_resolve_missing_field() {
        let data = json!({"a": 1});
        let path = Path::parse("a.b.c");
        assert_eq!(path.resolve(&data), None);
    }

    #[test]
    fn test_resolve_index_out_of_bounds() {
        let data = json!({"items": [1, 2, 3]});
        let path = Path::parse("items.10");
        assert_eq!(path.resolve(&data), None);
    }

    #[test]
    fn test_resolve_root() {
        let data = json!(42);
        let path = Path::root();
        assert_eq!(path.resolve(&data), Some(&json!(42)));
    }

    #[test]
    fn test_builder_pattern() {
        let path = Path::root()
            .field("resource")
            .field("context")
            .field("args");
        assert_eq!(path.len(), 3);
    }

    #[test]
    fn test_readme_scenario() {
        let data = json!({
            "principal": "alice",
            "action": "call",
            "resource": {
                "method": "add",
                "context": {
                    "namedArguments": [
                        {"name": "lhs", "value": 10},
                        {"name": "rhs", "value": 5}
                    ]
                }
            }
        });

        let path = Path::parse("resource.context.namedArguments");
        let result = path.resolve(&data);
        assert!(result.is_some());
        assert!(result.unwrap().is_array());
        assert_eq!(result.unwrap().as_array().unwrap().len(), 2);
    }
}
