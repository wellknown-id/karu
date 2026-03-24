//! Tree-sitter grammar for Cedar schema language (dev-time parser).
//!
//! Provides error-tolerant parsing with span information for LSP tooling
//! on `.cedarschema` files. Complements `cedar_schema_parser.rs`.

#[allow(clippy::manual_non_exhaustive)]
pub mod grammar {
    use rust_sitter::Rule;

    // ── Token types ──────────────────────────────────────────────────────

    /// Identifier token
    #[derive(Debug, Clone, PartialEq, Eq, Rule)]
    #[leaf(pattern(r"[a-zA-Z_][a-zA-Z0-9_]*"))]
    pub struct SchemaIdent;

    /// Type/namespace path: `Foo::Bar`
    #[derive(Debug, Clone, PartialEq, Eq, Rule)]
    #[leaf(pattern(r"[a-zA-Z_][a-zA-Z0-9_]*(::[a-zA-Z_][a-zA-Z0-9_]*)*"))]
    pub struct SchemaPath;

    /// Name or string: identifier or double-quoted string
    #[derive(Debug, Clone, PartialEq, Eq, Rule)]
    #[leaf(pattern(r#"[a-zA-Z_][a-zA-Z0-9_]*|"([^"\\]|\\.)*""#))]
    pub struct SchemaNameOrString;

    /// Action reference: ident path with optional trailing ::"string", or plain string
    #[derive(Debug, Clone, PartialEq, Eq, Rule)]
    #[leaf(pattern(
        r#"[a-zA-Z_][a-zA-Z0-9_]*(::[a-zA-Z_][a-zA-Z0-9_]*)*(::"([^"\\]|\\.)*")?|"([^"\\]|\\.)*""#
    ))]
    pub struct SchemaActionRefToken;

    // ========================================================================
    // Root
    // ========================================================================

    /// A Cedar schema file is a list of declarations or namespaces.
    #[derive(Debug, Rule)]
    #[language]
    #[extras(re(r"\s"), re(r"//[^\n]*"))]
    #[word(SchemaIdent)]
    pub struct Schema {
        pub items: Vec<SchemaItem>,
    }

    // ========================================================================
    // Top-level items — annotations are hoisted here to avoid conflicts
    // ========================================================================

    /// A schema item with leading annotations.
    #[derive(Debug, Rule)]
    pub enum SchemaItem {
        Namespace(NamespaceDecl),
        Entity(EntityDecl),
        Action(ActionDecl),
        TypeDecl(TypeAliasDecl),
    }

    // ========================================================================
    // Namespace
    // ========================================================================

    /// `namespace Foo::Bar { decl* }`
    #[derive(Debug, Rule)]
    pub struct NamespaceDecl {
        #[leaf("namespace")]
        _kw: (),
        #[leaf(SchemaPath)]
        pub path: String,
        #[leaf("{")]
        _lb: (),
        pub decls: Vec<NamespaceInnerDecl>,
        #[leaf("}")]
        _rb: (),
    }

    /// Declarations inside a namespace.
    #[derive(Debug, Rule)]
    pub enum NamespaceInnerDecl {
        Entity(EntityDecl),
        Action(ActionDecl),
        TypeDecl(TypeAliasDecl),
    }

    // ========================================================================
    // Entity
    // ========================================================================

    /// `entity Foo [in Bar] [{ field: Type, ... }] ;`
    #[derive(Debug, Rule)]
    pub struct EntityDecl {
        #[leaf("entity")]
        _kw: (),
        #[leaf(SchemaIdent)]
        pub name: String,
        pub extra_names: Vec<ExtraName>,
        pub parents: Option<EntityParents>,
        pub body: Option<RecordType>,
        #[leaf(";")]
        _semi: (),
    }

    /// `, Name` for multi-entity declarations
    #[derive(Debug, Rule)]
    pub struct ExtraName {
        #[leaf(",")]
        _comma: (),
        #[leaf(SchemaIdent)]
        pub name: String,
    }

    /// `in Parent` or `in [Parent1, Parent2]`
    #[derive(Debug, Rule)]
    pub enum EntityParents {
        Single(#[leaf("in")] (), #[leaf(SchemaPath)] String),
        List(
            #[leaf("in")] (),
            #[leaf("[")] (),
            #[sep_by(",")] Vec<TypePath>,
            #[leaf("]")] (),
        ),
    }

    /// A type path like `Foo::Bar` — uses SchemaIdent (the word token) as base
    /// so that tree-sitter keyword extraction works for `Set`, `Long`, etc.
    #[derive(Debug, Rule)]
    pub struct TypePath {
        #[leaf(SchemaIdent)]
        pub head: String,
        pub tail: Vec<TypePathSegment>,
    }

    /// `::Ident` segment in a type path
    #[derive(Debug, Rule)]
    pub struct TypePathSegment {
        #[leaf("::")]
        _sep: (),
        #[leaf(SchemaIdent)]
        pub name: String,
    }

    // ========================================================================
    // Action
    // ========================================================================

    /// `action "doThing" [in SomeAction] [appliesTo { ... }] ;`
    #[derive(Debug, Rule)]
    pub struct ActionDecl {
        #[leaf("action")]
        _kw: (),
        #[leaf(SchemaNameOrString)]
        pub name: String,
        pub extra_names: Vec<ExtraActionName>,
        pub parents: Option<ActionParents>,
        pub applies_to: Option<AppliesTo>,
        #[leaf(";")]
        _semi: (),
    }

    /// `, "name"` for multi-action declarations
    #[derive(Debug, Rule)]
    pub struct ExtraActionName {
        #[leaf(",")]
        _comma: (),
        #[leaf(SchemaNameOrString)]
        pub name: String,
    }

    /// `in ActionRef` or `in [ActionRef, ...]`
    #[derive(Debug, Rule)]
    pub enum ActionParents {
        Single(#[leaf("in")] (), #[leaf(SchemaActionRefToken)] String),
        List(
            #[leaf("in")] (),
            #[leaf("[")] (),
            #[sep_by(",")] Vec<ActionRef>,
            #[leaf("]")] (),
        ),
    }

    /// A reference to an action (ident, string, or qualified path)
    #[derive(Debug, Rule)]
    pub struct ActionRef {
        #[leaf(SchemaActionRefToken)]
        pub name: String,
    }

    // ========================================================================
    // AppliesTo
    // ========================================================================

    /// `appliesTo { principal: ..., resource: ..., context: ... }`
    #[derive(Debug, Rule)]
    pub struct AppliesTo {
        #[leaf("appliesTo")]
        _kw: (),
        #[leaf("{")]
        _lb: (),
        pub decls: Vec<AppliesToDecl>,
        #[leaf("}")]
        _rb: (),
    }

    /// A single `principal: Type` / `resource: Type` / `context: Type` declaration.
    #[derive(Debug, Rule)]
    pub enum AppliesToDecl {
        Principal(
            #[leaf("principal")] (),
            #[leaf(":")] (),
            TypeExpr,
            Option<TrailingComma>,
        ),
        Resource(
            #[leaf("resource")] (),
            #[leaf(":")] (),
            TypeExpr,
            Option<TrailingComma>,
        ),
        Context(
            #[leaf("context")] (),
            #[leaf(":")] (),
            TypeExpr,
            Option<TrailingComma>,
        ),
    }

    #[derive(Debug, Rule)]
    pub struct TrailingComma {
        #[leaf(",")]
        _comma: (),
    }

    // ========================================================================
    // Type expressions
    // ========================================================================

    /// A Cedar schema type.
    /// NOTE: `SetType` must come before `Named` so tree-sitter matches the
    /// keyword `"Set"` before `TypePath` (which also matches `Set` as an ident).
    #[derive(Debug, Rule)]
    pub enum TypeExpr {
        /// `Set<Type>`
        SetType(
            #[leaf("Set")] (),
            #[leaf("<")] (),
            Box<TypeExpr>,
            #[leaf(">")] (),
        ),
        /// Named type: `Long`, `String`, `Bool`, `MyType`, `Namespace::Type`
        Named(TypePath),
        /// Record type: `{ field: Type, ... }`
        Record(RecordType),
    }

    /// `{ field?: Type, ... }`
    #[derive(Debug, Rule)]
    pub struct RecordType {
        #[leaf("{")]
        _lb: (),
        #[sep_by(",")]
        pub fields: Vec<FieldDecl>,
        #[leaf("}")]
        _rb: (),
    }

    /// A field in a record type: `name?: Type`
    #[derive(Debug, Rule)]
    pub struct FieldDecl {
        #[leaf(SchemaNameOrString)]
        pub name: String,
        pub optional: Option<QuestionMark>,
        #[leaf(":")]
        _colon: (),
        pub ty: TypeExpr,
    }

    #[derive(Debug, Rule)]
    pub struct QuestionMark {
        #[leaf("?")]
        _q: (),
    }

    // ========================================================================
    // Type alias
    // ========================================================================

    /// `type MyType = Long;`
    #[derive(Debug, Rule)]
    pub struct TypeAliasDecl {
        #[leaf("type")]
        _kw: (),
        #[leaf(SchemaIdent)]
        pub name: String,
        #[leaf("=")]
        _eq: (),
        pub ty: TypeExpr,
        #[leaf(";")]
        _semi: (),
    }
}

// ============================================================================
// Conversion to Karu schema AST
// ============================================================================

use grammar::*;

impl TypePath {
    /// Reconstruct the full path string, e.g. `"Foo::Bar"`.
    fn full_path(&self) -> String {
        let mut path = self.head.clone();
        for seg in &self.tail {
            path.push_str("::");
            path.push_str(&seg.name);
        }
        path
    }
}

/// Strip surrounding double quotes from a string literal token.
fn strip_quotes(s: &str) -> String {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

impl Schema {
    /// Convert to Karu `ModuleDef` list.
    pub fn to_schema_ast(&self) -> Vec<crate::schema::ModuleDef> {
        let mut modules = Vec::new();
        let mut default_module = crate::schema::ModuleDef {
            name: None,
            entities: vec![],
            actions: vec![],
            abstracts: vec![],
        };

        for item in &self.items {
            match item {
                SchemaItem::Namespace(ns) => {
                    modules.push(ns.to_module_def());
                }
                SchemaItem::Entity(e) => {
                    default_module.entities.extend(e.to_entity_defs());
                }
                SchemaItem::Action(a) => {
                    default_module.actions.extend(a.to_action_defs());
                }
                SchemaItem::TypeDecl(t) => {
                    default_module.abstracts.push(t.to_abstract_def());
                }
            }
        }

        if !default_module.entities.is_empty()
            || !default_module.actions.is_empty()
            || !default_module.abstracts.is_empty()
        {
            modules.insert(0, default_module);
        }

        modules
    }
}

impl NamespaceDecl {
    fn to_module_def(&self) -> crate::schema::ModuleDef {
        let mut module = crate::schema::ModuleDef {
            name: Some(self.path.clone()),
            entities: vec![],
            actions: vec![],
            abstracts: vec![],
        };

        for decl in &self.decls {
            match decl {
                NamespaceInnerDecl::Entity(e) => {
                    module.entities.extend(e.to_entity_defs());
                }
                NamespaceInnerDecl::Action(a) => {
                    module.actions.extend(a.to_action_defs());
                }
                NamespaceInnerDecl::TypeDecl(t) => {
                    module.abstracts.push(t.to_abstract_def());
                }
            }
        }

        module
    }
}

impl EntityDecl {
    fn to_entity_defs(&self) -> Vec<crate::schema::EntityDef> {
        let parents = match &self.parents {
            None => vec![],
            Some(EntityParents::Single(_, name)) => vec![name.clone()],
            Some(EntityParents::List(_, _, types, _)) => {
                types.iter().map(|t| t.full_path()).collect()
            }
        };

        let fields = match &self.body {
            None => vec![],
            Some(rec) => rec.to_field_defs(),
        };

        let mut names = vec![self.name.clone()];
        for extra in &self.extra_names {
            names.push(extra.name.clone());
        }

        names
            .into_iter()
            .map(|name| crate::schema::EntityDef {
                kind: crate::schema::EntityKind::Resource,
                name,
                parents: parents.clone(),
                traits: vec![],
                fields: fields.clone(),
            })
            .collect()
    }
}

impl ActionDecl {
    fn to_action_defs(&self) -> Vec<crate::schema::ActionDef> {
        let applies_to = self.applies_to.as_ref().map(|at| at.to_action_applies_to());

        let mut names = vec![strip_quotes(&self.name)];
        for extra in &self.extra_names {
            names.push(strip_quotes(&extra.name));
        }

        names
            .into_iter()
            .map(|name| crate::schema::ActionDef {
                name,
                applies_to: applies_to.clone(),
            })
            .collect()
    }
}

impl AppliesTo {
    fn to_action_applies_to(&self) -> crate::schema::ActionAppliesTo {
        let mut actors = vec![];
        let mut resources = vec![];
        let mut context = None;

        for decl in &self.decls {
            match decl {
                AppliesToDecl::Principal(_, _, ty, _) => {
                    actors = ty.to_type_names();
                }
                AppliesToDecl::Resource(_, _, ty, _) => {
                    resources = ty.to_type_names();
                }
                AppliesToDecl::Context(_, _, ty, _) => {
                    context = Some(ty.to_context_fields());
                }
            }
        }

        crate::schema::ActionAppliesTo {
            actors,
            resources,
            context,
        }
    }
}

impl TypeExpr {
    fn to_type_ref(&self) -> crate::schema::TypeRef {
        match self {
            TypeExpr::Named(tp) => crate::schema::TypeRef::Named(tp.full_path()),
            TypeExpr::SetType(_, _, inner, _) => {
                crate::schema::TypeRef::Set(Box::new(inner.to_type_ref()))
            }
            TypeExpr::Record(rec) => crate::schema::TypeRef::Record(rec.to_field_defs()),
        }
    }

    fn to_type_names(&self) -> Vec<String> {
        match self {
            TypeExpr::Named(tp) => vec![tp.full_path()],
            _ => vec![],
        }
    }

    fn to_context_fields(&self) -> Vec<crate::schema::FieldDef> {
        match self {
            TypeExpr::Record(rec) => rec.to_field_defs(),
            _ => vec![],
        }
    }
}

impl RecordType {
    fn to_field_defs(&self) -> Vec<crate::schema::FieldDef> {
        self.fields.iter().map(|f| f.to_field_def()).collect()
    }
}

impl FieldDecl {
    fn to_field_def(&self) -> crate::schema::FieldDef {
        crate::schema::FieldDef {
            name: strip_quotes(&self.name),
            ty: self.ty.to_type_ref(),
            optional: self.optional.is_some(),
        }
    }
}

impl TypeAliasDecl {
    fn to_abstract_def(&self) -> crate::schema::AbstractDef {
        // Cedar `type X = RecordType` maps to AbstractDef with fields
        let fields = match &self.ty {
            TypeExpr::Record(rec) => rec.to_field_defs(),
            _ => vec![],
        };
        crate::schema::AbstractDef {
            name: self.name.clone(),
            fields,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::grammar;
    use rust_sitter::Language;

    #[test]
    fn test_parse_empty_namespace() {
        let result = grammar::Schema::parse("namespace Foo { }");
        let schema = result.result.expect("should parse");
        assert_eq!(schema.items.len(), 1);
    }

    #[test]
    fn test_parse_entity() {
        let result = grammar::Schema::parse(r#"entity User { name: String, age?: Long };"#);
        let schema = result.result.expect("should parse");
        assert_eq!(schema.items.len(), 1);
    }

    #[test]
    fn test_parse_entity_with_parents() {
        let result = grammar::Schema::parse(r#"entity Admin in [User, Manager] { level: Long };"#);
        let schema = result.result.expect("should parse");
        let modules = schema.to_schema_ast();
        assert!(!modules.is_empty());
        assert_eq!(modules[0].entities[0].parents.len(), 2);
    }

    #[test]
    fn test_parse_action() {
        let result = grammar::Schema::parse(
            r#"action read appliesTo { principal: User, resource: Document };"#,
        );
        let schema = result.result.expect("should parse");
        assert_eq!(schema.items.len(), 1);
    }

    #[test]
    fn test_parse_type_alias() {
        let result = grammar::Schema::parse(r#"type Context = { ip: String };"#);
        let schema = result.result.expect("should parse");
        assert_eq!(schema.items.len(), 1);
    }

    #[test]
    fn test_parse_set_type() {
        let result = grammar::Schema::parse(r#"entity Group { members: Set<String> };"#);
        let schema = result.result.expect("should parse");
        let modules = schema.to_schema_ast();
        assert_eq!(modules[0].entities[0].fields.len(), 1);
    }

    #[test]
    fn test_roundtrip_to_schema_ast() {
        let result = grammar::Schema::parse(
            r#"
            namespace MyApp {
                entity User { name: String };
                action read appliesTo { principal: User, resource: Document };
            }
            "#,
        );
        let schema = result.result.expect("should parse");
        let modules = schema.to_schema_ast();
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, Some("MyApp".to_string()));
        assert_eq!(modules[0].entities.len(), 1);
        assert_eq!(modules[0].actions.len(), 1);
    }
}
