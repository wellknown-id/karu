// SPDX-License-Identifier: MIT

//! Schema types for Karu's typed mode (`use schema`).
//!
//! When a Karu source file begins with `use schema;`, the parser recognises
//! module definitions (`mod`), entity declarations (`actor`/`resource`),
//! action declarations (`action`), and reusable assertions (`assert`).
//!
//! These types form the Schema AST - a layer above the policy AST that
//! enables type checking, Cedar-compatible namespaces, and strict
//! evaluation.

use crate::ast::ExprAst;

// ============================================================================
// Type references
// ============================================================================

/// A reference to a type used in field declarations and action scopes.
#[derive(Debug, Clone)]
pub enum TypeRef {
    /// Named type: `String`, `Boolean`, `Long`, `DateTime`, `User`, etc.
    Named(String),
    /// Set type: `Set<T>`
    Set(Box<TypeRef>),
    /// Inline record type: `{ field: Type, ... }`
    Record(Vec<FieldDef>),
    /// Union type: `A | B` (for action appliesTo, nullable fields)
    Union(Vec<TypeRef>),
}

/// A field definition in a record or entity shape.
#[derive(Debug, Clone)]
pub struct FieldDef {
    /// Field name.
    pub name: String,
    /// Field type.
    pub ty: TypeRef,
    /// Whether the field is optional (`field?`).
    pub optional: bool,
}

// ============================================================================
// Entity declarations
// ============================================================================

/// Entity kind - distinguishes principals from resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityKind {
    /// `actor` - maps to Cedar principal entity types.
    Actor,
    /// `resource` - maps to Cedar resource entity types.
    Resource,
}

/// An entity type declaration inside a `mod` block.
///
/// ```text
/// actor User { name String };
/// resource File in Folder { owner User, name String };
/// resource File is Ownable;
/// ```
#[derive(Debug, Clone)]
pub struct EntityDef {
    /// Whether this is an `actor` or `resource`.
    pub kind: EntityKind,
    /// Entity type name.
    pub name: String,
    /// Parent types (`in Folder` → `vec!["Folder"]`).
    pub parents: Vec<String>,
    /// Trait types (`is Ownable` → `vec!["Ownable"]`).
    pub traits: Vec<String>,
    /// Fields (shape) of this entity.
    pub fields: Vec<FieldDef>,
}

// ============================================================================
// Action declarations
// ============================================================================

/// An action declaration inside a `mod` block.
///
/// ```text
/// action "Delete" appliesTo {
///     actor User,
///     resource File | Folder,
///     context { authenticated Boolean }
/// };
/// ```
#[derive(Debug, Clone)]
pub struct ActionDef {
    /// Action name (can be an identifier or quoted string).
    pub name: String,
    /// What entities/context this action applies to.
    pub applies_to: Option<ActionAppliesTo>,
}

/// Specifies which principal/resource/context an action applies to.
#[derive(Debug, Clone)]
pub struct ActionAppliesTo {
    /// Principal (actor) types this action applies to.
    pub actors: Vec<String>,
    /// Resource types this action applies to.
    pub resources: Vec<String>,
    /// Context shape (if specified).
    pub context: Option<Vec<FieldDef>>,
}

// ============================================================================
// Module (namespace) definitions
// ============================================================================

/// A module (namespace) definition.
///
/// ```text
/// mod MyCedarNamespace {
///     actor User { name String };
///     resource File in Folder { owner User };
///     action "Delete" appliesTo { actor User, resource File };
///     abstract Ownable { owner User };
/// };
/// ```
///
/// Unnamed modules are also supported for file-local types:
/// ```text
/// mod { actor User { name String }; };
/// ```
#[derive(Debug, Clone)]
pub struct ModuleDef {
    /// Namespace name. `None` for unnamed file-local modules.
    pub name: Option<String>,
    /// Entity declarations in this module.
    pub entities: Vec<EntityDef>,
    /// Action declarations in this module.
    pub actions: Vec<ActionDef>,
    /// Abstract type declarations (Cedar's `type` keyword).
    pub abstracts: Vec<AbstractDef>,
}

// ============================================================================
// Abstract type declarations
// ============================================================================

/// An abstract type declaration (maps to Cedar's `type` keyword).
///
/// Abstracts define reusable field sets that can be composed into entities
/// using the `is` keyword, similar to traits.
///
/// ```text
/// abstract Ownable { owner User };
/// resource File is Ownable { name String };
/// ```
#[derive(Debug, Clone)]
pub struct AbstractDef {
    /// Abstract type name.
    pub name: String,
    /// Fields defined by this abstract type.
    pub fields: Vec<FieldDef>,
}

// ============================================================================
// Assertions (reusable macro conditions)
// ============================================================================

/// A reusable condition assertion, inlined at compile time.
///
/// ```text
/// assert user_is_owner<User, action, File> if actor.name == resource.owner.name;
/// assert user_has_roles is actor has roles;
/// ```
#[derive(Debug, Clone)]
pub struct AssertDef {
    /// Assertion name (used as identifier in rule bodies).
    pub name: String,
    /// Optional type parameters for documentation/validation.
    /// `<User, action, File>` → `vec!["User", "action", "File"]`
    pub type_params: Vec<String>,
    /// The condition expression body.
    pub body: ExprAst,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ExprAst, PathAst, PathSegmentAst};

    #[test]
    fn test_type_ref() {
        let named = TypeRef::Named("String".to_string());
        let set = TypeRef::Set(Box::new(named.clone()));
        let field = FieldDef {
            name: "field".to_string(),
            ty: named.clone(),
            optional: false,
        };
        let record = TypeRef::Record(vec![field]);
        let union = TypeRef::Union(vec![named.clone(), set.clone()]);

        assert!(format!("{:?}", named).contains("Named(\"String\")"));
        assert!(format!("{:?}", set).contains("Set"));
        assert!(format!("{:?}", record).contains("Record"));
        assert!(format!("{:?}", union).contains("Union"));

        let _cloned = union.clone();
    }

    #[test]
    fn test_entity_def() {
        let field = FieldDef {
            name: "id".to_string(),
            ty: TypeRef::Named("String".to_string()),
            optional: true,
        };

        let entity = EntityDef {
            kind: EntityKind::Actor,
            name: "User".to_string(),
            parents: vec!["Folder".to_string()],
            traits: vec!["Ownable".to_string()],
            fields: vec![field],
        };

        assert_eq!(entity.kind, EntityKind::Actor);
        assert_eq!(entity.name, "User");
        assert_eq!(entity.parents, vec!["Folder".to_string()]);
        assert_eq!(entity.traits, vec!["Ownable".to_string()]);
        assert_eq!(entity.fields.len(), 1);

        let _cloned = entity.clone();
        assert!(format!("{:?}", entity).contains("Actor"));
    }

    #[test]
    fn test_action_def() {
        let applies_to = ActionAppliesTo {
            actors: vec!["User".to_string()],
            resources: vec!["File".to_string()],
            context: Some(vec![FieldDef {
                name: "ip".to_string(),
                ty: TypeRef::Named("String".to_string()),
                optional: false,
            }]),
        };

        let action = ActionDef {
            name: "Delete".to_string(),
            applies_to: Some(applies_to),
        };

        assert_eq!(action.name, "Delete");
        assert!(action.applies_to.is_some());

        let _cloned = action.clone();
        assert!(format!("{:?}", action).contains("Delete"));
    }

    #[test]
    fn test_module_def() {
        let module = ModuleDef {
            name: Some("MyNamespace".to_string()),
            entities: vec![],
            actions: vec![],
            abstracts: vec![],
        };

        assert_eq!(module.name, Some("MyNamespace".to_string()));
        assert!(module.entities.is_empty());

        let _cloned = module.clone();
        assert!(format!("{:?}", module).contains("MyNamespace"));
    }

    #[test]
    fn test_abstract_def() {
        let abstract_def = AbstractDef {
            name: "Ownable".to_string(),
            fields: vec![FieldDef {
                name: "owner".to_string(),
                ty: TypeRef::Named("User".to_string()),
                optional: false,
            }],
        };

        assert_eq!(abstract_def.name, "Ownable");
        assert_eq!(abstract_def.fields.len(), 1);

        let _cloned = abstract_def.clone();
        assert!(format!("{:?}", abstract_def).contains("Ownable"));
    }

    #[test]
    fn test_assert_def() {
        let assert_def = AssertDef {
            name: "is_admin".to_string(),
            type_params: vec!["User".to_string()],
            body: ExprAst::Has {
                path: PathAst { segments: vec![PathSegmentAst::Field("is_admin".to_string())] }
            },
        };

        assert_eq!(assert_def.name, "is_admin");
        assert_eq!(assert_def.type_params, vec!["User".to_string()]);

        let _cloned = assert_def.clone();
        assert!(format!("{:?}", assert_def).contains("is_admin"));
    }
}
