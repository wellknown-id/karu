//! Schema types for Karu's typed mode (`use schema`).
//!
//! When a Karu source file begins with `use schema;`, the parser recognises
//! module definitions (`mod`), entity declarations (`actor`/`resource`),
//! action declarations (`action`), and reusable assertions (`assert`).
//!
//! These types form the Schema AST — a layer above the policy AST that
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

/// Entity kind — distinguishes principals from resources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityKind {
    /// `actor` — maps to Cedar principal entity types.
    Actor,
    /// `resource` — maps to Cedar resource entity types.
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
