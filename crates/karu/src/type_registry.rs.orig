// SPDX-License-Identifier: MIT

//! Type registry for Karu's structural type system.
//!
//! Provides compile-time type shape computation and runtime value fingerprinting.
//! Type conformance checks (`value is Type`) use a two-phase approach:
//!
//! 1. **Fast reject** - `u128` presence bitmask: `(value & type) == type`
//! 2. **Exact verify** - sorted field shapes comparison (name hash + type tag)
//!
//! # Example
//!
//! ```text
//! abstract Ownable { owner User };
//! resource File is Ownable { name String };
//!
//! # File's shape = Ownable's fields + File's own fields
//! # resource is File → structural conformance check
//! # resource is Ownable → subset conformance check
//! ```

use crate::schema::{AbstractDef, EntityDef, FieldDef, ModuleDef, TypeRef};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};

// ============================================================================
// Type tags - one byte per primitive JSON type
// ============================================================================

/// Primitive type tags for leaf nodes in a shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum TypeTag {
    String = 0x01,
    Long = 0x02,
    Boolean = 0x03,
    Set = 0x04,
    Record = 0x05,
    Null = 0x06,
}

impl TypeTag {
    /// Infer the type tag from a JSON value.
    pub fn from_value(v: &Value) -> Self {
        match v {
            Value::String(_) => TypeTag::String,
            Value::Number(_) => TypeTag::Long,
            Value::Bool(_) => TypeTag::Boolean,
            Value::Array(_) => TypeTag::Set,
            Value::Object(_) => TypeTag::Record,
            Value::Null => TypeTag::Null,
        }
    }

    /// Map a schema type reference to a type tag.
    /// Named types that are entities/abstracts → Record.
    /// Primitives map directly.
    pub fn from_type_ref(ty: &TypeRef) -> Self {
        match ty {
            TypeRef::Named(name) => match name.as_str() {
                "String" | "string" => TypeTag::String,
                "Long" | "long" | "Int" | "int" => TypeTag::Long,
                "Boolean" | "boolean" | "bool" => TypeTag::Boolean,
                // Extension types stored as strings
                "DateTime" | "datetime" | "Decimal" | "decimal" | "Duration" | "duration"
                | "Ip" | "ip" => TypeTag::String,
                // Named entity/abstract types are records at runtime
                _ => TypeTag::Record,
            },
            TypeRef::Set(_) => TypeTag::Set,
            TypeRef::Record(_) => TypeTag::Record,
            TypeRef::Union(types) => {
                // For unions, use the first non-null type's tag
                types
                    .iter()
                    .map(TypeTag::from_type_ref)
                    .find(|t| *t != TypeTag::Null)
                    .unwrap_or(TypeTag::Null)
            }
        }
    }
}

// ============================================================================
// Field shape - one field's contribution to a type fingerprint
// ============================================================================

/// A single field's shape: its name hash and type tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldShape {
    /// Hash of the field name (for fast sorted comparison).
    pub name_hash: u64,
    /// Primitive type tag.
    pub type_tag: TypeTag,
}

impl FieldShape {
    pub fn new(name: &str, type_tag: TypeTag) -> Self {
        Self {
            name_hash: hash_field_name(name),
            type_tag,
        }
    }
}

/// Compute a deterministic hash for a field name.
fn hash_field_name(name: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    hasher.finish()
}

// ============================================================================
// Type shape - the full fingerprint for a type
// ============================================================================

/// A type's structural shape, used for conformance checking.
///
/// The `presence` bitmask enables O(1) fast rejection:
/// `(value.presence & type.presence) == type.presence`
///
/// The sorted `fields` vector enables exact structural verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeShape {
    /// Bloom-filter style bitmask: bit = hash(field_name) % 128.
    /// Used for fast rejection of non-conforming values.
    pub presence: u128,
    /// Sorted field shapes for exact structural matching.
    pub fields: Vec<FieldShape>,
    /// Sub-shapes for Record-type fields (field name_hash → child TypeShape).
    /// Enables recursive nested type checking.
    pub children: BTreeMap<u64, TypeShape>,
}

impl TypeShape {
    /// Create an empty shape (no fields).
    pub fn empty() -> Self {
        Self {
            presence: 0,
            fields: Vec::new(),
            children: BTreeMap::new(),
        }
    }

    /// Add a field to this shape.
    pub fn add_field(&mut self, name: &str, type_tag: TypeTag) {
        let bit = (hash_field_name(name) % 128) as u32;
        self.presence |= 1u128 << bit;
        self.fields.push(FieldShape::new(name, type_tag));
    }

    /// Add a field with a nested sub-shape (for Record-type fields like `owner User`).
    pub fn add_field_with_shape(&mut self, name: &str, type_tag: TypeTag, child: TypeShape) {
        let name_hash = hash_field_name(name);
        let bit = (name_hash % 128) as u32;
        self.presence |= 1u128 << bit;
        self.fields.push(FieldShape::new(name, type_tag));
        if !child.fields.is_empty() {
            self.children.insert(name_hash, child);
        }
    }

    /// Finalize the shape by sorting fields for deterministic comparison.
    pub fn finalize(&mut self) {
        self.fields.sort();
        self.fields.dedup();
    }

    /// Fast rejection: does `self` have all the field bits that `required` needs?
    #[inline]
    pub fn has_required_bits(&self, required: &TypeShape) -> bool {
        (self.presence & required.presence) == required.presence
    }

    /// Full structural conformance: does `self` contain all fields in `required`
    /// with matching type tags? Recursively checks sub-shapes for Record fields.
    ///
    /// Assumes both shapes are finalized (sorted).
    pub fn conforms_to(&self, required: &TypeShape) -> bool {
        // Phase 1: fast bitmask rejection
        if !self.has_required_bits(required) {
            return false;
        }

        // Phase 2: verify each required field exists with correct type
        let mut self_idx = 0;
        for req_field in &required.fields {
            // Advance through self's fields to find the matching one
            while self_idx < self.fields.len() && self.fields[self_idx] < *req_field {
                self_idx += 1;
            }
            if self_idx >= self.fields.len() || self.fields[self_idx] != *req_field {
                return false;
            }

            // Phase 3: if this field has a required sub-shape, check it recursively
            if let Some(req_child) = required.children.get(&req_field.name_hash) {
                if let Some(self_child) = self.children.get(&req_field.name_hash) {
                    if !self_child.conforms_to(req_child) {
                        return false;
                    }
                } else {
                    // Required sub-shape but no child shape in self → fail
                    return false;
                }
            }

            self_idx += 1;
        }
        true
    }
}

// ============================================================================
// Runtime fingerprinting - walk a JSON value to produce its shape
// ============================================================================

/// Fingerprint a JSON value by recursively walking its fields.
///
/// For object values, creates a shape with sub-shapes for nested objects.
/// Returns an empty shape for non-object values.
pub fn fingerprint_value(value: &Value) -> TypeShape {
    fingerprint_value_recursive(value)
}

fn fingerprint_value_recursive(value: &Value) -> TypeShape {
    let obj = match value {
        Value::Object(map) => map,
        _ => return TypeShape::empty(),
    };

    let mut shape = TypeShape::empty();
    for (key, val) in obj {
        let tag = TypeTag::from_value(val);
        if tag == TypeTag::Record {
            // Recursively fingerprint nested objects
            let child = fingerprint_value_recursive(val);
            shape.add_field_with_shape(key, tag, child);
        } else {
            shape.add_field(key, tag);
        }
    }
    shape.finalize();
    shape
}

// ============================================================================
// Type registry - compile-time shape computation from schema
// ============================================================================

/// Registry of type shapes built from schema modules.
///
/// Maps type names (potentially namespaced) to their computed shapes.
/// Handles abstract inheritance and field merging.
#[derive(Debug, Clone)]
pub struct TypeRegistry {
    /// Type name → computed shape.
    shapes: HashMap<String, TypeShape>,
}

impl TypeRegistry {
    /// Build a type registry from a list of module definitions.
    pub fn from_modules(modules: &[ModuleDef]) -> Self {
        let mut registry = TypeRegistry {
            shapes: HashMap::new(),
        };

        for module in modules {
            // First pass: register all abstracts
            for abs in &module.abstracts {
                let shape = registry.build_abstract_shape(abs);
                let name = Self::qualified_name(&module.name, &abs.name);
                // Register both qualified and unqualified names
                registry.shapes.insert(abs.name.clone(), shape.clone());
                registry.shapes.insert(name, shape);
            }

            // Second pass: register all entities (can reference abstracts via `is`)
            for entity in &module.entities {
                let shape = registry.build_entity_shape(entity);
                let name = Self::qualified_name(&module.name, &entity.name);
                // Register both qualified and unqualified names
                registry.shapes.insert(entity.name.clone(), shape.clone());
                registry.shapes.insert(name, shape);
            }
        }

        registry
    }

    /// Look up a type's shape by name.
    pub fn get(&self, name: &str) -> Option<&TypeShape> {
        self.shapes.get(name)
    }

    /// Look up by qualified name (namespace:name).
    pub fn get_qualified(&self, namespace: Option<&str>, name: &str) -> Option<&TypeShape> {
        if let Some(ns) = namespace {
            let qualified = format!("{}:{}", ns, name);
            self.shapes
                .get(&qualified)
                .or_else(|| self.shapes.get(name))
        } else {
            self.shapes.get(name)
        }
    }

    /// Build a qualified name from an optional namespace + type name.
    fn qualified_name(namespace: &Option<String>, name: &str) -> String {
        match namespace {
            Some(ns) => format!("{}:{}", ns, name),
            None => name.to_string(),
        }
    }

    /// Build the shape for an abstract type.
    fn build_abstract_shape(&self, abs: &AbstractDef) -> TypeShape {
        let mut shape = TypeShape::empty();
        for field in &abs.fields {
            if !field.optional {
                self.add_typed_field(&mut shape, &field.name, &field.ty);
            }
        }
        shape.finalize();
        shape
    }

    /// Build the shape for an entity, merging in abstract trait fields.
    fn build_entity_shape(&self, entity: &EntityDef) -> TypeShape {
        let mut shape = TypeShape::empty();

        // Merge fields from all traits (abstracts), including children
        for trait_name in &entity.traits {
            if let Some(trait_shape) = self.shapes.get(trait_name) {
                shape.presence |= trait_shape.presence;
                shape.fields.extend(trait_shape.fields.iter().cloned());
                for (k, v) in &trait_shape.children {
                    shape.children.insert(*k, v.clone());
                }
            }
        }

        // Add entity's own fields
        for field in &entity.fields {
            if !field.optional {
                self.add_typed_field(&mut shape, &field.name, &field.ty);
            }
        }

        shape.finalize();
        shape
    }

    /// Build shapes for all fields in a field list (used for context shapes, etc.).
    pub fn build_fields_shape(&self, fields: &[FieldDef]) -> TypeShape {
        let mut shape = TypeShape::empty();
        for field in fields {
            if !field.optional {
                self.add_typed_field(&mut shape, &field.name, &field.ty);
            }
        }
        shape.finalize();
        shape
    }

    /// Add a field to a shape, resolving named entity types into sub-shapes.
    fn add_typed_field(&self, shape: &mut TypeShape, name: &str, ty: &TypeRef) {
        let tag = TypeTag::from_type_ref(ty);
        if tag == TypeTag::Record {
            // This is a named entity/abstract type - look up its sub-shape
            if let TypeRef::Named(ref type_name) = ty {
                if let Some(child_shape) = self.shapes.get(type_name) {
                    shape.add_field_with_shape(name, tag, child_shape.clone());
                    return;
                }
            }
        }
        shape.add_field(name, tag);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::*;

    fn make_field(name: &str, ty: &str) -> FieldDef {
        FieldDef {
            name: name.to_string(),
            ty: TypeRef::Named(ty.to_string()),
            optional: false,
        }
    }

    fn make_optional_field(name: &str, ty: &str) -> FieldDef {
        FieldDef {
            name: name.to_string(),
            ty: TypeRef::Named(ty.to_string()),
            optional: true,
        }
    }

    #[test]
    fn test_type_tag_from_value() {
        assert_eq!(
            TypeTag::from_value(&Value::String("hi".into())),
            TypeTag::String
        );
        assert_eq!(TypeTag::from_value(&serde_json::json!(42)), TypeTag::Long);
        assert_eq!(TypeTag::from_value(&Value::Bool(true)), TypeTag::Boolean);
        assert_eq!(
            TypeTag::from_value(&serde_json::json!([1, 2])),
            TypeTag::Set
        );
        assert_eq!(
            TypeTag::from_value(&serde_json::json!({"a": 1})),
            TypeTag::Record
        );
        assert_eq!(TypeTag::from_value(&Value::Null), TypeTag::Null);
    }

    #[test]
    fn test_type_tag_from_type_ref() {
        assert_eq!(
            TypeTag::from_type_ref(&TypeRef::Named("String".into())),
            TypeTag::String
        );
        assert_eq!(
            TypeTag::from_type_ref(&TypeRef::Named("string".into())),
            TypeTag::String
        );
        assert_eq!(
            TypeTag::from_type_ref(&TypeRef::Named("Long".into())),
            TypeTag::Long
        );
        assert_eq!(
            TypeTag::from_type_ref(&TypeRef::Named("Boolean".into())),
            TypeTag::Boolean
        );
        assert_eq!(
            TypeTag::from_type_ref(&TypeRef::Named("User".into())),
            TypeTag::Record
        );
        assert_eq!(
            TypeTag::from_type_ref(&TypeRef::Set(Box::new(TypeRef::Named("String".into())))),
            TypeTag::Set
        );
    }

    #[test]
    fn test_fingerprint_json_object() {
        let val = serde_json::json!({
            "name": "alice",
            "age": 30,
            "admin": true
        });
        let shape = fingerprint_value(&val);
        assert_eq!(shape.fields.len(), 3);
        // Check that presence bits are set
        assert_ne!(shape.presence, 0);
    }

    #[test]
    fn test_fingerprint_non_object() {
        let shape = fingerprint_value(&serde_json::json!("just a string"));
        assert_eq!(shape, TypeShape::empty());
    }

    #[test]
    fn test_shape_conforms_exact() {
        // Build a type shape for File { name String, owner String }
        let mut file_shape = TypeShape::empty();
        file_shape.add_field("name", TypeTag::String);
        file_shape.add_field("owner", TypeTag::String);
        file_shape.finalize();

        // Fingerprint a matching value
        let val = serde_json::json!({"name": "doc.txt", "owner": "alice"});
        let val_shape = fingerprint_value(&val);

        assert!(val_shape.conforms_to(&file_shape));
    }

    #[test]
    fn test_shape_conforms_superset() {
        // File has more fields than Ownable
        let mut ownable_shape = TypeShape::empty();
        ownable_shape.add_field("owner", TypeTag::String);
        ownable_shape.finalize();

        // Value has owner + name + extra
        let val = serde_json::json!({"owner": "alice", "name": "doc.txt", "size": 1024});
        let val_shape = fingerprint_value(&val);

        // Value conforms to Ownable (has owner field)
        assert!(val_shape.conforms_to(&ownable_shape));
    }

    #[test]
    fn test_shape_rejects_missing_field() {
        let mut file_shape = TypeShape::empty();
        file_shape.add_field("name", TypeTag::String);
        file_shape.add_field("owner", TypeTag::String);
        file_shape.finalize();

        // Value missing "owner"
        let val = serde_json::json!({"name": "doc.txt"});
        let val_shape = fingerprint_value(&val);

        assert!(!val_shape.conforms_to(&file_shape));
    }

    #[test]
    fn test_shape_rejects_wrong_type() {
        let mut file_shape = TypeShape::empty();
        file_shape.add_field("name", TypeTag::String);
        file_shape.add_field("owner", TypeTag::String);
        file_shape.finalize();

        // Value has "owner" as number, not string
        let val = serde_json::json!({"name": "doc.txt", "owner": 42});
        let val_shape = fingerprint_value(&val);

        assert!(!val_shape.conforms_to(&file_shape));
    }

    #[test]
    fn test_registry_from_modules() {
        let modules = vec![ModuleDef {
            name: Some("Ns".to_string()),
            entities: vec![
                EntityDef {
                    kind: EntityKind::Actor,
                    name: "User".to_string(),
                    parents: vec![],
                    traits: vec![],
                    fields: vec![make_field("name", "String")],
                },
                EntityDef {
                    kind: EntityKind::Resource,
                    name: "File".to_string(),
                    parents: vec![],
                    traits: vec!["Ownable".to_string()],
                    fields: vec![make_field("name", "String")],
                },
            ],
            actions: vec![],
            abstracts: vec![AbstractDef {
                name: "Ownable".to_string(),
                fields: vec![make_field("owner", "User")],
            }],
        }];

        let registry = TypeRegistry::from_modules(&modules);

        // Ownable shape should have 1 field (owner)
        let ownable = registry.get("Ownable").unwrap();
        assert_eq!(ownable.fields.len(), 1);

        // File shape should have 2 fields (name + owner from Ownable)
        let file = registry.get("File").unwrap();
        assert_eq!(file.fields.len(), 2);

        // A File-shaped value should conform to both File and Ownable
        let val = serde_json::json!({"name": "doc.txt", "owner": {"name": "alice"}});
        let val_shape = fingerprint_value(&val);
        assert!(val_shape.conforms_to(file));
        assert!(val_shape.conforms_to(ownable));
    }

    #[test]
    fn test_registry_abstract_only_value() {
        // A value with just owner should conform to Ownable but not File
        let modules = vec![ModuleDef {
            name: Some("Ns".to_string()),
            entities: vec![EntityDef {
                kind: EntityKind::Resource,
                name: "File".to_string(),
                parents: vec![],
                traits: vec!["Ownable".to_string()],
                fields: vec![make_field("name", "String")],
            }],
            actions: vec![],
            abstracts: vec![AbstractDef {
                name: "Ownable".to_string(),
                fields: vec![make_field("owner", "User")],
            }],
        }];

        let registry = TypeRegistry::from_modules(&modules);
        let ownable = registry.get("Ownable").unwrap();
        let file = registry.get("File").unwrap();

        // Value with only "owner" conforms to Ownable but not File
        let val = serde_json::json!({"owner": {"name": "alice"}});
        let val_shape = fingerprint_value(&val);
        assert!(val_shape.conforms_to(ownable));
        assert!(!val_shape.conforms_to(file));
    }

    #[test]
    fn test_optional_fields_not_required() {
        let modules = vec![ModuleDef {
            name: None,
            entities: vec![EntityDef {
                kind: EntityKind::Resource,
                name: "Doc".to_string(),
                parents: vec![],
                traits: vec![],
                fields: vec![
                    make_field("name", "String"),
                    make_optional_field("description", "String"),
                ],
            }],
            actions: vec![],
            abstracts: vec![],
        }];

        let registry = TypeRegistry::from_modules(&modules);
        let doc = registry.get("Doc").unwrap();

        // Value without optional "description" should still conform
        let val = serde_json::json!({"name": "readme.md"});
        let val_shape = fingerprint_value(&val);
        assert!(val_shape.conforms_to(doc));
    }

    #[test]
    fn test_qualified_lookup() {
        let modules = vec![ModuleDef {
            name: Some("MyNs".to_string()),
            entities: vec![EntityDef {
                kind: EntityKind::Resource,
                name: "File".to_string(),
                parents: vec![],
                traits: vec![],
                fields: vec![make_field("name", "String")],
            }],
            actions: vec![],
            abstracts: vec![],
        }];

        let registry = TypeRegistry::from_modules(&modules);

        // Should find by qualified name
        assert!(registry.get("MyNs:File").is_some());
        // Should also find by unqualified name
        assert!(registry.get("File").is_some());
        // get_qualified should work both ways
        assert!(registry.get_qualified(Some("MyNs"), "File").is_some());
        assert!(registry.get_qualified(None, "File").is_some());
    }

    #[test]
    fn test_empty_type() {
        let modules = vec![ModuleDef {
            name: Some("Ns".to_string()),
            entities: vec![EntityDef {
                kind: EntityKind::Resource,
                name: "Folder".to_string(),
                parents: vec![],
                traits: vec![],
                fields: vec![],
            }],
            actions: vec![],
            abstracts: vec![],
        }];

        let registry = TypeRegistry::from_modules(&modules);
        let folder = registry.get("Folder").unwrap();

        // Empty type should conform with any object
        let val = serde_json::json!({"anything": true});
        let val_shape = fingerprint_value(&val);
        assert!(val_shape.conforms_to(folder));

        // Even empty objects
        let empty = serde_json::json!({});
        let empty_shape = fingerprint_value(&empty);
        assert!(empty_shape.conforms_to(folder));
    }

    #[test]
    fn test_recursive_nested_type_checking() {
        // File { owner User, name String }
        // User { name String, email String }
        // `resource is File` should verify owner conforms to User's shape
        let modules = vec![ModuleDef {
            name: Some("Ns".to_string()),
            entities: vec![
                EntityDef {
                    kind: EntityKind::Actor,
                    name: "User".to_string(),
                    parents: vec![],
                    traits: vec![],
                    fields: vec![make_field("name", "String"), make_field("email", "String")],
                },
                EntityDef {
                    kind: EntityKind::Resource,
                    name: "File".to_string(),
                    parents: vec![],
                    traits: vec![],
                    fields: vec![make_field("owner", "User"), make_field("name", "String")],
                },
            ],
            actions: vec![],
            abstracts: vec![],
        }];

        let registry = TypeRegistry::from_modules(&modules);
        let file = registry.get("File").unwrap();

        // File shape should have a child sub-shape for "owner"
        assert!(
            !file.children.is_empty(),
            "File should have child sub-shapes"
        );

        // Valid: owner has all User fields
        let valid = serde_json::json!({
            "name": "doc.txt",
            "owner": {"name": "alice", "email": "alice@example.com"}
        });
        let valid_shape = fingerprint_value(&valid);
        assert!(valid_shape.conforms_to(file), "valid File should conform");

        // Invalid: owner is an object but missing "email" field
        let invalid = serde_json::json!({
            "name": "doc.txt",
            "owner": {"name": "alice"}
        });
        let invalid_shape = fingerprint_value(&invalid);
        assert!(
            !invalid_shape.conforms_to(file),
            "owner missing email should not conform"
        );

        // Invalid: owner is an object with wrong field types
        let wrong_type = serde_json::json!({
            "name": "doc.txt",
            "owner": {"name": 42, "email": "alice@example.com"}
        });
        let wrong_shape = fingerprint_value(&wrong_type);
        assert!(
            !wrong_shape.conforms_to(file),
            "owner with wrong name type should not conform"
        );

        // Invalid: owner has garbage fields
        let garbage = serde_json::json!({
            "name": "doc.txt",
            "owner": {"garbage": true}
        });
        let garbage_shape = fingerprint_value(&garbage);
        assert!(
            !garbage_shape.conforms_to(file),
            "owner with garbage fields should not conform"
        );
    }
}
