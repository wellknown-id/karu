# Modeling ABAC in Karu

Attribute-Based Access Control (ABAC) controls access based on resource attributes like visibility, status, or classification.

## Public vs Private Resources

```json
{
  "principal": {"id": "anonymous"},
  "action": "read",
  "resource": {
    "type": "Repository",
    "id": "karu",
    "isPublic": true
  }
}
```

```karu
# Anyone can read public resources
allow read_public if
    action == "read" and
    resource.isPublic == true;

# Private resources require authentication
allow read_private if
    action == "read" and
    resource.isPublic == false and
    principal.authenticated == true;
```

## Resource State (Draft/Published/Archived)

```json
{
  "resource": {
    "type": "Document",
    "status": "published"
  }
}
```

```karu
# Only published documents are viewable
allow view if
    action == "view" and
    resource.status == "published";

# Only drafts can be edited
allow edit if
    action == "edit" and
    resource.status == "draft";

# Archived documents are read-only
deny modify if
    resource.status == "archived" and
    action in ["edit", "delete"];
```

## Department/Classification Matching

```json
{
  "principal": {
    "department": "Engineering",
    "clearanceLevel": 3
  },
  "resource": {
    "department": "Engineering",
    "requiredClearance": 2
  }
}
```

```karu
# Same department access
allow dept_access if
    principal.department == resource.department;

# Clearance level check
allow classified if
    principal.clearanceLevel >= resource.requiredClearance;
```

## Time-Based Access

Pass current time in context:

```json
{
  "context": {
    "currentTime": 1706634000
  },
  "resource": {
    "availableFrom": 1706600000,
    "availableUntil": 1706700000
  }
}
```

```karu
# Resource available during time window
allow timed_access if
    context.currentTime >= resource.availableFrom and
    context.currentTime <= resource.availableUntil;
```

## Subscription Tiers (Entitlements)

```json
{
  "principal": {
    "subscription": "premium"
  },
  "resource": {
    "tier": "premium"
  }
}
```

```karu
# Free content available to all
allow free if resource.tier == "free";

# Premium content requires premium subscription
allow premium if
    resource.tier == "premium" and
    principal.subscription == "premium";

# Enterprise includes premium
allow enterprise if
    resource.tier == "premium" and
    principal.subscription == "enterprise";
```

## Geographic Restrictions

```json
{
  "context": {
    "userCountry": "US"
  },
  "resource": {
    "allowedCountries": ["US", "CA", "UK"]
  }
}
```

```karu
allow geo_access if
    context.userCountry in resource.allowedCountries;
```

## Combining ABAC with RBAC

```json
{
  "principal": {
    "roles": ["member"],
    "department": "Engineering"
  },
  "resource": {
    "department": "Engineering",
    "confidential": true
  }
}
```

```karu
# Members can read non-confidential in their department
allow read if
    "member" in principal.roles and
    principal.department == resource.department and
    resource.confidential == false;

# Admins can read confidential
allow read_confidential if
    "admin" in principal.roles and
    principal.department == resource.department;
```

## Feature Flags

```json
{
  "context": {
    "features": {
      "newEditor": true,
      "betaApi": false
    }
  }
}
```

```karu
allow use_new_editor if
    action == "edit" and
    context.features.newEditor == true;

deny beta_api if
    action == "api_v2" and
    context.features.betaApi == false;
```

## Best Practices

1. **Pass time in context** - Don't rely on server time in policies
2. **Pre-compute complex attributes** - Classification, effective permissions
3. **Use deny rules for restrictions** - Block archived, expired, restricted
4. **Combine patterns** - ABAC works well with RBAC/ReBAC

## See Also

- [Modeling RBAC](modeling-rbac.md) - Role-based patterns
- [Modeling ReBAC](modeling-rebac.md) - Relationship-based patterns
