// SPDX-License-Identifier: MIT

//! Tests for Cedar policy equivalents.
//!
//! These tests demonstrate Karu equivalents to the Cedar policy examples
//! from https://docs.cedarpolicy.com/policies/policy-examples.html
//!
//! Note: Some Cedar features require alternative syntax in Karu:
//! - OR conditions: Use multiple rules instead
//! - Cross-path comparisons: Use matching literal values or restructure data

use karu::{compile, Effect};
use serde_json::json;

// ============================================================================
// Individual Entity Access
// ============================================================================

/// Cedar: permit(principal == User::"alice", action == Action::"view",
///               resource == Photo::"VacationPhoto94.jpg");
#[test]
fn test_cedar_individual_entity_access() {
    let policy = compile(
        r#"
        allow access if
            principal == "alice" and
            action == "view" and
            resource == "VacationPhoto94.jpg";
        "#,
    )
    .unwrap();

    // Alice viewing VacationPhoto94.jpg - ALLOW
    assert_eq!(
        policy.evaluate(&json!({
            "principal": "alice",
            "action": "view",
            "resource": "VacationPhoto94.jpg"
        })),
        Effect::Allow
    );

    // Bob viewing same photo - DENY (wrong principal)
    assert_eq!(
        policy.evaluate(&json!({
            "principal": "bob",
            "action": "view",
            "resource": "VacationPhoto94.jpg"
        })),
        Effect::Deny
    );

    // Alice deleting photo - DENY (wrong action)
    assert_eq!(
        policy.evaluate(&json!({
            "principal": "alice",
            "action": "delete",
            "resource": "VacationPhoto94.jpg"
        })),
        Effect::Deny
    );
}

// ============================================================================
// Group Access
// ============================================================================

/// Cedar: permit(principal in Group::"alice_friends", action == Action::"view",
///               resource == Photo::"VacationPhoto94.jpg");
#[test]
fn test_cedar_group_membership() {
    let policy = compile(
        r#"
        allow access if
            "alice_friends" in principal.groups and
            action == "view" and
            resource == "VacationPhoto94.jpg";
        "#,
    )
    .unwrap();

    // Principal in alice_friends group - ALLOW
    assert_eq!(
        policy.evaluate(&json!({
            "principal": {"name": "bob", "groups": ["alice_friends", "coworkers"]},
            "action": "view",
            "resource": "VacationPhoto94.jpg"
        })),
        Effect::Allow
    );

    // Principal not in alice_friends - DENY
    assert_eq!(
        policy.evaluate(&json!({
            "principal": {"name": "eve", "groups": ["strangers"]},
            "action": "view",
            "resource": "VacationPhoto94.jpg"
        })),
        Effect::Deny
    );
}

/// Cedar: permit(principal == User::"alice", action == Action::"view",
///               resource in Album::"alice_vacation");
#[test]
fn test_cedar_resource_in_container() {
    let policy = compile(
        r#"
        allow access if
            principal == "alice" and
            action == "view" and
            resource.album == "alice_vacation";
        "#,
    )
    .unwrap();

    // Photo in alice_vacation album - ALLOW
    assert_eq!(
        policy.evaluate(&json!({
            "principal": "alice",
            "action": "view",
            "resource": {"name": "photo1.jpg", "album": "alice_vacation"}
        })),
        Effect::Allow
    );

    // Photo in different album - DENY
    assert_eq!(
        policy.evaluate(&json!({
            "principal": "alice",
            "action": "view",
            "resource": {"name": "photo2.jpg", "album": "work_photos"}
        })),
        Effect::Deny
    );
}

/// Cedar: permit(principal == User::"alice",
///               action in [Action::"view", Action::"edit", Action::"delete"],
///               resource in Album::"alice_vacation");
///
/// Note: Karu uses multiple rules instead of OR conditions
#[test]
fn test_cedar_multiple_actions() {
    let policy = compile(
        r#"
        allow view if
            principal == "alice" and
            resource.album == "alice_vacation" and
            action == "view";
        allow edit if
            principal == "alice" and
            resource.album == "alice_vacation" and
            action == "edit";
        allow delete if
            principal == "alice" and
            resource.album == "alice_vacation" and
            action == "delete";
        "#,
    )
    .unwrap();

    for action in ["view", "edit", "delete"] {
        assert_eq!(
            policy.evaluate(&json!({
                "principal": "alice",
                "action": action,
                "resource": {"album": "alice_vacation"}
            })),
            Effect::Allow,
            "Action {} should be allowed",
            action
        );
    }

    // Other actions - DENY
    assert_eq!(
        policy.evaluate(&json!({
            "principal": "alice",
            "action": "share",
            "resource": {"album": "alice_vacation"}
        })),
        Effect::Deny
    );
}

// ============================================================================
// Any Entity (Wildcards)
// ============================================================================

/// Cedar: permit(principal, action == Action::"view",
///               resource in Album::"alice_vacation");
#[test]
fn test_cedar_any_principal() {
    let policy = compile(
        r#"
        allow access if
            action == "view" and
            resource.album == "alice_vacation";
        "#,
    )
    .unwrap();

    // Any user can view - ALLOW
    for user in ["alice", "bob", "charlie", "anonymous"] {
        assert_eq!(
            policy.evaluate(&json!({
                "principal": user,
                "action": "view",
                "resource": {"album": "alice_vacation"}
            })),
            Effect::Allow,
            "User {} should be allowed to view",
            user
        );
    }
}

/// Cedar: permit(principal == User::"alice", action,
///               resource in Album::"jane_vacation");
#[test]
fn test_cedar_any_action() {
    let policy = compile(
        r#"
        allow access if
            principal == "alice" and
            resource.album == "jane_vacation";
        "#,
    )
    .unwrap();

    // Alice can do anything - ALLOW
    for action in ["view", "edit", "delete", "share", "comment"] {
        assert_eq!(
            policy.evaluate(&json!({
                "principal": "alice",
                "action": action,
                "resource": {"album": "jane_vacation"}
            })),
            Effect::Allow,
            "Action {} should be allowed for alice",
            action
        );
    }
}

// ============================================================================
// Attribute-Based Access Control (ABAC)
// ============================================================================

/// Cedar: permit(principal, action in [Action::"listPhotos", Action::"view"],
///               resource in Album::"device_prototypes")
///        when { principal.department == "HardwareEngineering" && principal.jobLevel >= 5 };
///
/// Note: Action list handled via multiple rules
#[test]
fn test_cedar_abac_department_and_level() {
    let policy = compile(
        r#"
        allow list if
            action == "listPhotos" and
            resource.album == "device_prototypes" and
            principal.department == "HardwareEngineering" and
            principal.jobLevel >= 5;
        allow view if
            action == "view" and
            resource.album == "device_prototypes" and
            principal.department == "HardwareEngineering" and
            principal.jobLevel >= 5;
        "#,
    )
    .unwrap();

    // Hardware engineer level 6 - ALLOW
    assert_eq!(
        policy.evaluate(&json!({
            "principal": {"name": "engineer1", "department": "HardwareEngineering", "jobLevel": 6},
            "action": "view",
            "resource": {"album": "device_prototypes"}
        })),
        Effect::Allow
    );

    // Hardware engineer level 4 - DENY (too low)
    assert_eq!(
        policy.evaluate(&json!({
            "principal": {"name": "engineer2", "department": "HardwareEngineering", "jobLevel": 4},
            "action": "view",
            "resource": {"album": "device_prototypes"}
        })),
        Effect::Deny
    );

    // Software engineer level 7 - DENY (wrong department)
    assert_eq!(
        policy.evaluate(&json!({
            "principal": {"name": "dev1", "department": "SoftwareEngineering", "jobLevel": 7},
            "action": "view",
            "resource": {"album": "device_prototypes"}
        })),
        Effect::Deny
    );
}

/// Cedar: permit(principal == User::"alice", action == Action::"view", resource)
///        when { resource.fileType == "JPEG" };
#[test]
fn test_cedar_abac_resource_attribute() {
    let policy = compile(
        r#"
        allow access if
            principal == "alice" and
            action == "view" and
            resource.fileType == "JPEG";
        "#,
    )
    .unwrap();

    // JPEG file - ALLOW
    assert_eq!(
        policy.evaluate(&json!({
            "principal": "alice",
            "action": "view",
            "resource": {"name": "photo.jpg", "fileType": "JPEG"}
        })),
        Effect::Allow
    );

    // PNG file - DENY
    assert_eq!(
        policy.evaluate(&json!({
            "principal": "alice",
            "action": "view",
            "resource": {"name": "image.png", "fileType": "PNG"}
        })),
        Effect::Deny
    );
}

/// Cedar: permit(principal == User::"alice", action, resource)
///        when { context has readOnly && context.readOnly == true };
#[test]
fn test_cedar_abac_context_attribute() {
    let policy = compile(
        r#"
        allow access if
            principal == "alice" and
            context.readOnly == true;
        "#,
    )
    .unwrap();

    // Read-only context - ALLOW
    assert_eq!(
        policy.evaluate(&json!({
            "principal": "alice",
            "action": "view",
            "resource": "any",
            "context": {"readOnly": true}
        })),
        Effect::Allow
    );

    // Non read-only context - DENY
    assert_eq!(
        policy.evaluate(&json!({
            "principal": "alice",
            "action": "view",
            "resource": "any",
            "context": {"readOnly": false}
        })),
        Effect::Deny
    );
}

/// Cedar: permit(principal, action, resource) when { principal == resource.owner };
///
/// Note: Karu uses flatten comparison via a shared ID field
#[test]
fn test_cedar_owner_access() {
    // In Karu, we compare by a common identifier rather than entity reference
    let policy = compile(
        r#"
        allow access if
            principal.id == resource.ownerId;
        "#,
    )
    .unwrap();

    // Owner accessing their resource - ALLOW
    assert_eq!(
        policy.evaluate(&json!({
            "principal": {"id": "user123"},
            "action": "delete",
            "resource": {"name": "myfile.txt", "ownerId": "user123"}
        })),
        Effect::Allow
    );

    // Non-owner accessing resource - DENY
    assert_eq!(
        policy.evaluate(&json!({
            "principal": {"id": "user456"},
            "action": "delete",
            "resource": {"name": "myfile.txt", "ownerId": "user123"}
        })),
        Effect::Deny
    );
}

/// Cedar: permit(principal, action == Action::"view", resource)
///        when { principal.department == resource.owner.department };
///
/// Note: For nested cross-path comparison, we flatten the data structure
#[test]
fn test_cedar_department_match() {
    // Flatten the comparison by using a department field directly on resource
    let policy = compile(
        r#"
        allow access if
            action == "view" and
            principal.department == resource.department;
        "#,
    )
    .unwrap();

    // Same department - ALLOW
    assert_eq!(
        policy.evaluate(&json!({
            "principal": {"name": "alice", "department": "Engineering"},
            "action": "view",
            "resource": {"department": "Engineering"}
        })),
        Effect::Allow
    );

    // Different department - DENY
    assert_eq!(
        policy.evaluate(&json!({
            "principal": {"name": "bob", "department": "Marketing"},
            "action": "view",
            "resource": {"department": "Engineering"}
        })),
        Effect::Deny
    );
}

/// Cedar: permit(principal, action, resource)
///        when { principal == resource.owner || resource.admins.contains(principal) };
///
/// Note: Use Rust API for the admin check since path-in-path needs Rust
#[test]
fn test_cedar_owner_or_admin() {
    use karu::{Condition, Operator, Path, Pattern, Rule};

    // The owner rule can use the DSL
    let mut policy = compile(
        r#"
        allow owner if principal.id == resource.ownerId;
        "#,
    )
    .unwrap();

    // Admin rule uses Rust API since "path in path" not yet in DSL
    // Check if list at resource.adminIds contains value at principal.id
    // We flip it: check if principal.id (the value) is IN resource.adminIds (the array)
    policy.add_rule(Rule::allow(
        "admin",
        vec![Condition::new(
            Path::from("principal.id"),
            Operator::In,
            Pattern::PathRef(Path::from("resource.adminIds")),
        )],
    ));

    // Owner - ALLOW
    assert_eq!(
        policy.evaluate(&json!({
            "principal": {"id": "owner123"},
            "action": "anything",
            "resource": {"ownerId": "owner123", "adminIds": ["admin1", "admin2"]}
        })),
        Effect::Allow
    );

    // Admin - ALLOW
    assert_eq!(
        policy.evaluate(&json!({
            "principal": {"id": "admin1"},
            "action": "anything",
            "resource": {"ownerId": "owner123", "adminIds": ["admin1", "admin2"]}
        })),
        Effect::Allow
    );

    // Neither - DENY
    assert_eq!(
        policy.evaluate(&json!({
            "principal": {"id": "random"},
            "action": "anything",
            "resource": {"ownerId": "owner123", "adminIds": ["admin1", "admin2"]}
        })),
        Effect::Deny
    );
}

// ============================================================================
// Deny Access (Forbid)
// ============================================================================

/// Cedar: forbid(principal == User::"alice", action, resource)
///        unless { action in Action::"readOnly" };
#[test]
fn test_cedar_forbid_unless() {
    let policy = compile(
        r#"
        allow readonly if principal == "alice" and action == "view";
        deny non_readonly if principal == "alice" and not action == "view";
        "#,
    )
    .unwrap();

    // Alice read-only - ALLOW
    assert_eq!(
        policy.evaluate(&json!({
            "principal": "alice",
            "action": "view",
            "resource": "anything"
        })),
        Effect::Allow
    );

    // Alice write action - DENY
    assert_eq!(
        policy.evaluate(&json!({
            "principal": "alice",
            "action": "delete",
            "resource": "anything"
        })),
        Effect::Deny
    );
}

/// Cedar: forbid(principal, action, resource)
///        when { resource.private } unless { principal == resource.owner };
///
/// Note: Use flattened ID comparison
#[test]
fn test_cedar_forbid_private_unless_owner() {
    let policy = compile(
        r#"
        allow general;
        deny private if
            resource.private == true and
            not principal.id == resource.ownerId;
        "#,
    )
    .unwrap();

    // Owner accessing private resource - ALLOW
    assert_eq!(
        policy.evaluate(&json!({
            "principal": {"id": "user1"},
            "action": "view",
            "resource": {"private": true, "ownerId": "user1"}
        })),
        Effect::Allow
    );

    // Non-owner accessing private resource - DENY
    assert_eq!(
        policy.evaluate(&json!({
            "principal": {"id": "user2"},
            "action": "view",
            "resource": {"private": true, "ownerId": "user1"}
        })),
        Effect::Deny
    );

    // Anyone accessing public resource - ALLOW
    assert_eq!(
        policy.evaluate(&json!({
            "principal": {"id": "anyone"},
            "action": "view",
            "resource": {"private": false, "ownerId": "user1"}
        })),
        Effect::Allow
    );
}

// ============================================================================
// Complex Combined Policies
// ============================================================================

/// Combined policy demonstrating multiple rules working together
#[test]
fn test_cedar_combined_photo_sharing() {
    use karu::{Condition, Operator, Path, Pattern, Rule};

    // Start with DSL-compatible rules
    let mut policy = compile(
        r#"
        // Owners can do anything
        allow owner_access if principal.id == resource.ownerId;
        
        // Deny access to private photos for non-owners
        deny private_block if
            resource.private == true and
            not principal.id == resource.ownerId;
        "#,
    )
    .unwrap();

    // Friend view rule uses Rust API for path-in-path
    policy.add_rule(Rule::allow(
        "friend_view",
        vec![
            Condition::eq(Path::from("action"), Pattern::literal("view")),
            Condition::new(
                Path::from("principal.id"),
                Operator::In,
                Pattern::PathRef(Path::from("resource.sharedWith")),
            ),
        ],
    ));

    let photo = json!({
        "name": "vacation.jpg",
        "private": false,
        "ownerId": "alice",
        "sharedWith": ["bob", "charlie"]
    });

    let private_photo = json!({
        "name": "private.jpg",
        "private": true,
        "ownerId": "alice",
        "sharedWith": ["bob"]
    });

    // Owner can do anything on public photo
    assert_eq!(
        policy.evaluate(&json!({
            "principal": {"id": "alice"},
            "action": "delete",
            "resource": photo.clone()
        })),
        Effect::Allow
    );

    // Friend can view public photo
    assert_eq!(
        policy.evaluate(&json!({
            "principal": {"id": "bob"},
            "action": "view",
            "resource": photo.clone()
        })),
        Effect::Allow
    );

    // Owner can access private photo
    assert_eq!(
        policy.evaluate(&json!({
            "principal": {"id": "alice"},
            "action": "view",
            "resource": private_photo.clone()
        })),
        Effect::Allow
    );

    // Friend CANNOT access private photo (deny overrides)
    assert_eq!(
        policy.evaluate(&json!({
            "principal": {"id": "bob"},
            "action": "view",
            "resource": private_photo.clone()
        })),
        Effect::Deny
    );
}
