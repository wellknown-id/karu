# Modeling RBAC in Karu

Role-Based Access Control (RBAC) groups permissions into roles. Users are assigned roles instead of individual permissions.

## Basic Organization Roles

**Polar equivalent:**

```polar
resource Organization {
  roles = ["admin", "member"];
  permissions = ["read", "add_member"];
  "read" if "member";
  "add_member" if "admin";
  "member" if "admin";  # role hierarchy
}
```

**Karu approach:**

```karu
# Members can read
allow read if
    action == "read" and
    "member" in principal.roles;

# Admins can add members
allow add_member if
    action == "add_member" and
    "admin" in principal.roles;

# Admins also get member permissions (explicit)
allow read if
    action == "read" and
    "admin" in principal.roles;
```

**Input data:**

```json
{
  "principal": { "id": "alice", "roles": ["admin"] },
  "action": "read",
  "resource": { "type": "Organization", "id": "acme" }
}
```

## Role Hierarchy with Pre-computed Effective Roles

For complex hierarchies, pre-compute effective roles in your application:

```json
{
  "principal": {
    "id": "alice",
    "role": "admin",
    "effectiveRoles": ["admin", "member", "viewer"]
  }
}
```

**Simplified policy:**

```karu
allow read if "viewer" in principal.effectiveRoles;
allow write if "member" in principal.effectiveRoles;
allow admin if "admin" in principal.effectiveRoles;
```

## Resource-Scoped Roles

Users can have different roles on different resources:

```json
{
  "principal": {
    "id": "alice",
    "resourceRoles": {
      "repo:anvil": ["admin"],
      "repo:other": ["member"]
    }
  },
  "resource": { "type": "Repository", "id": "anvil" }
}
```

**Policy using path construction:**

```karu
# Build the lookup key and check roles
allow admin if
    "admin" in principal.resourceRoles[resource.id];
```

This dynamically looks up the role list using `resource.id` as the key.

**Alternative - flatten to resource:**

```json
{
  "principal": { "id": "alice" },
  "resource": {
    "id": "anvil",
    "userRoles": { "alice": ["admin"] }
  }
}
```

```karu
allow admin if
    "admin" in resource.userRoles[principal.id];
```

## Global Roles (Super Admin)

```json
{
  "principal": { "id": "alice" },
  "context": { "globalRoles": ["super_admin"] }
}
```

```karu
# Super admins can do anything
allow super if "super_admin" in context.globalRoles;

# Regular role checks
allow read if "member" in principal.roles;
```

## Complete RBAC Example

```karu
# Organization permissions
allow org_read if
    resource.type == "Organization" and
    action == "read" and
    ("member" in principal.roles or "admin" in principal.roles);

allow org_write if
    resource.type == "Organization" and
    action == "write" and
    "admin" in principal.roles;

# Repository inherits from organization
allow repo_read if
    resource.type == "Repository" and
    action == "read" and
    "member" in principal.orgRoles;

allow repo_delete if
    resource.type == "Repository" and
    action == "delete" and
    "admin" in principal.orgRoles;
```

## Best Practices

1. **Pre-compute role hierarchies** in your application layer
2. **Use `effectiveRoles`** array for inheritance
3. **Separate resource-level roles** from organization-level roles
4. **Keep policies flat** - avoid complex nesting

## See Also

- [Modeling ReBAC](MODELING-REBAC.md) - Relationship-based patterns
- [Modeling ABAC](MODELING-ABAC.md) - Attribute-based patterns
- [Cedar Comparison](CEDAR-COMPARISON.md) - Migration from Cedar
