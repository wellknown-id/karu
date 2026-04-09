// SPDX-License-Identifier: MIT

//! Round-trip integration tests for Cedar ↔ Karu conversions.
//!
//! These tests use real Cedar example files from:
//! https://github.com/cedar-policy/cedar-examples/tree/main/cedar-example-use-cases
//!
//! DoD items tested:
//! 1. We can ingest .cedar files correctly
//! 2. We can ingest .cedarschema files correctly
//! 3. Round trip .cedar → .karu → .cedar
//! 4. Round trip .cedar + .cedarschema → .karu → .cedar + .cedarschema

// ============================================================================
// DoD 1: Ingest .cedar files correctly
// ============================================================================

/// Document Cloud policies from Cedar examples repo.
#[test]
fn test_ingest_cedar_document_cloud() {
    let cedar = include_str!("../examples/cedar-examples/document_cloud.cedar");
    let program = karu::from_cedar(cedar).unwrap();
    // Document cloud has 15 policies
    assert_eq!(
        program.rules.len(),
        15,
        "Expected 15 policies in document_cloud.cedar"
    );
    // First rule is a permit
    assert_eq!(program.rules[0].effect, karu::ast::EffectAst::Allow);
    // Last two rules are forbid
    assert_eq!(program.rules[13].effect, karu::ast::EffectAst::Deny);
    assert_eq!(program.rules[14].effect, karu::ast::EffectAst::Deny);
}

/// GitHub example policies from Cedar examples repo.
#[test]
fn test_ingest_cedar_github_example() {
    let cedar = include_str!("../examples/cedar-examples/github_example.cedar");
    let program = karu::from_cedar(cedar).unwrap();
    // GitHub example has 9 policies
    assert_eq!(
        program.rules.len(),
        9,
        "Expected 9 policies in github_example.cedar"
    );
    // All rules are permit
    for rule in &program.rules {
        assert_eq!(rule.effect, karu::ast::EffectAst::Allow);
    }
}

// ============================================================================
// DoD 2: Ingest .cedarschema files correctly
// ============================================================================

/// Document Cloud schema from Cedar examples repo.
#[test]
fn test_ingest_cedarschema_document_cloud() {
    let schema = include_str!("../examples/cedar-examples/document_cloud.cedarschema");
    let modules = karu::from_cedarschema(schema).unwrap();
    // No namespace → unnamed module
    assert_eq!(modules.len(), 1);
    assert!(modules[0].name.is_none());

    // Should have 5 entity types: DocumentShare, Drive, Document, Group, Public, User
    assert_eq!(modules[0].entities.len(), 6, "Expected 6 entities");

    // Verify Document entity has fields
    let doc_entity = modules[0]
        .entities
        .iter()
        .find(|e| e.name == "Document")
        .unwrap();
    assert!(!doc_entity.fields.is_empty(), "Document should have fields");
    assert!(
        doc_entity.fields.iter().any(|f| f.name == "isPrivate"),
        "Document should have isPrivate field"
    );
    assert!(
        doc_entity.fields.iter().any(|f| f.name == "owner"),
        "Document should have owner field"
    );

    // Verify Group entity has parent
    let group = modules[0]
        .entities
        .iter()
        .find(|e| e.name == "Group")
        .unwrap();
    assert_eq!(group.parents, vec!["DocumentShare"]);

    // Verify User entity has Set<User> field
    let user = modules[0]
        .entities
        .iter()
        .find(|e| e.name == "User")
        .unwrap();
    let blocked = user.fields.iter().find(|f| f.name == "blocked").unwrap();
    assert!(matches!(blocked.ty, karu::schema::TypeRef::Set(_)));

    // Should have 10 action declarations (multi-action decls expanded)
    assert_eq!(
        modules[0].actions.len(),
        10,
        "Expected 10 action declarations"
    );

    // Verify ViewDocument action has multiple principals
    let view_doc = modules[0]
        .actions
        .iter()
        .find(|a| a.name == "ViewDocument")
        .unwrap();
    let at = view_doc.applies_to.as_ref().unwrap();
    assert_eq!(at.actors, vec!["User", "Public"]);
    assert_eq!(at.resources, vec!["Document"]);
    assert!(at.context.is_some(), "ViewDocument should have context");
}

/// GitHub example schema from Cedar examples repo.
#[test]
fn test_ingest_cedarschema_github_example() {
    let schema = include_str!("../examples/cedar-examples/github_example.cedarschema");
    let modules = karu::from_cedarschema(schema).unwrap();
    assert_eq!(modules.len(), 1);

    // Should have entity types: Team, UserGroup, Issue, Org, Repository, User
    assert_eq!(modules[0].entities.len(), 6, "Expected 6 entities");

    // Verify Repository has many fields
    let repo = modules[0]
        .entities
        .iter()
        .find(|e| e.name == "Repository")
        .unwrap();
    assert_eq!(repo.fields.len(), 5, "Repository should have 5 fields");

    // Should have 11 individual actions (multi-action decls expanded)
    assert_eq!(
        modules[0].actions.len(),
        11,
        "Expected 11 action declarations"
    );
}

// ============================================================================
// DoD 3: Round-trip .cedar → .karu → .cedar
// ============================================================================

/// Verify .cedar → Karu AST → .cedar produces valid Cedar.
/// We can't compare exact text (formatting differs), so we verify:
/// 1. The re-emitted Cedar re-parses without errors
/// 2. The re-parsed Cedar has the same number of policies
/// 3. All policies preserve their effect (permit/forbid)
#[test]
fn test_roundtrip_cedar_document_cloud() {
    let cedar = include_str!("../examples/cedar-examples/document_cloud.cedar");

    // Cedar → Karu AST
    let program = karu::from_cedar(cedar).unwrap();

    // Karu AST → Cedar
    let cedar_out = karu::to_cedar(&program).unwrap();

    // Re-parse the output
    let program2 = karu::from_cedar(&cedar_out).unwrap();

    // Same number of policies
    assert_eq!(
        program.rules.len(),
        program2.rules.len(),
        "Round-trip should preserve policy count"
    );

    // Same effects
    for (a, b) in program.rules.iter().zip(program2.rules.iter()) {
        assert_eq!(a.effect, b.effect, "Round-trip should preserve effect");
    }
}

#[test]
fn test_roundtrip_cedar_github_example() {
    let cedar = include_str!("../examples/cedar-examples/github_example.cedar");

    let program = karu::from_cedar(cedar).unwrap();
    let cedar_out = karu::to_cedar(&program).unwrap();
    let program2 = karu::from_cedar(&cedar_out).unwrap();

    assert_eq!(program.rules.len(), program2.rules.len());
    for (a, b) in program.rules.iter().zip(program2.rules.iter()) {
        assert_eq!(a.effect, b.effect);
    }
}

// ============================================================================
// DoD 4: Round-trip .cedar + .cedarschema → .karu → .cedar + .cedarschema
// ============================================================================

/// Full round-trip: cedar + cedarschema → Karu program → cedar + cedarschema.
/// Verifies both policy and schema preservation.
#[test]
fn test_roundtrip_with_schema_document_cloud() {
    let cedar = include_str!("../examples/cedar-examples/document_cloud.cedar");
    let schema = include_str!("../examples/cedar-examples/document_cloud.cedarschema");

    // Import both
    let program = karu::from_cedar_with_schema(cedar, schema).unwrap();
    assert!(program.use_schema);
    assert!(!program.modules.is_empty());

    // Re-emit policies
    let cedar_out = karu::to_cedar(&program).unwrap();
    let program2 = karu::from_cedar(&cedar_out).unwrap();
    assert_eq!(program.rules.len(), program2.rules.len());

    // Re-emit schema
    let schema_out = karu::to_cedarschema(&program.modules);

    // Re-parse the emitted schema
    let modules2 = karu::from_cedarschema(&schema_out).unwrap();

    // Verify structural preservation
    assert_eq!(
        program.modules.len(),
        modules2.len(),
        "Round-trip should preserve module count"
    );

    let m1 = &program.modules[0];
    let m2 = &modules2[0];

    assert_eq!(
        m1.entities.len(),
        m2.entities.len(),
        "Round-trip should preserve entity count"
    );
    assert_eq!(
        m1.actions.len(),
        m2.actions.len(),
        "Round-trip should preserve action count"
    );
    assert_eq!(
        m1.abstracts.len(),
        m2.abstracts.len(),
        "Round-trip should preserve type declaration count"
    );

    // Verify entity names preserved
    for (e1, e2) in m1.entities.iter().zip(m2.entities.iter()) {
        assert_eq!(e1.name, e2.name, "Entity name should be preserved");
        assert_eq!(
            e1.parents, e2.parents,
            "Entity parents should be preserved for {}",
            e1.name
        );
        assert_eq!(
            e1.fields.len(),
            e2.fields.len(),
            "Field count should be preserved for {}",
            e1.name
        );
    }

    // Verify action names preserved
    for (a1, a2) in m1.actions.iter().zip(m2.actions.iter()) {
        assert_eq!(a1.name, a2.name, "Action name should be preserved");
    }
}

#[test]
fn test_roundtrip_with_schema_github_example() {
    let cedar = include_str!("../examples/cedar-examples/github_example.cedar");
    let schema = include_str!("../examples/cedar-examples/github_example.cedarschema");

    let program = karu::from_cedar_with_schema(cedar, schema).unwrap();
    assert!(program.use_schema);

    let cedar_out = karu::to_cedar(&program).unwrap();
    let program2 = karu::from_cedar(&cedar_out).unwrap();
    assert_eq!(program.rules.len(), program2.rules.len());

    let schema_out = karu::to_cedarschema(&program.modules);
    let modules2 = karu::from_cedarschema(&schema_out).unwrap();

    let m1 = &program.modules[0];
    let m2 = &modules2[0];

    assert_eq!(m1.entities.len(), m2.entities.len());
    assert_eq!(m1.actions.len(), m2.actions.len());

    for (e1, e2) in m1.entities.iter().zip(m2.entities.iter()) {
        assert_eq!(e1.name, e2.name);
        assert_eq!(e1.fields.len(), e2.fields.len());
    }
}
