# Modeling ReBAC in Karu

Relationship-Based Access Control (ReBAC) grants permissions based on relationships between resources. For example, accessing a folder grants access to its files.

## Files & Folders Pattern

**Polar equivalent:**

```polar
resource File {
  relations = { folder: Folder };
  role if role on "folder";
  "read" if "reader";
}
```

**Karu approach - flatten relationships:**

```json
{
  "principal": { "id": "alice", "folderRoles": { "folder-123": ["reader"] } },
  "action": "read",
  "resource": {
    "type": "File",
    "id": "test.py",
    "folderId": "folder-123"
  }
}
```

```karu
allow read if
    action == "read" and
    "reader" in principal.folderRoles[resource.folderId];
```

## Owner/Creator Pattern

Users who create a resource get special permissions:

```json
{
  "principal": { "id": "alice" },
  "resource": {
    "type": "Issue",
    "id": "537",
    "creatorId": "alice"
  }
}
```

```karu
// Creators can update and close their issues
allow update if
    action == "update" and
    principal.id == resource.creatorId;

allow close if
    action == "close" and
    principal.id == resource.creatorId;
```

## Parent-Child Inheritance

Permissions cascade through resource hierarchies:

```json
{
  "principal": { "id": "alice" },
  "resource": {
    "type": "Document",
    "id": "doc-1",
    "organizationId": "acme"
  },
  "context": {
    "orgRoles": { "acme": ["member"] }
  }
}
```

```karu
// Org members can read documents in their org
allow read if
    action == "read" and
    "member" in context.orgRoles[resource.organizationId];

// Org admins can delete documents
allow delete if
    action == "delete" and
    "admin" in context.orgRoles[resource.organizationId];
```

## Resource Sharing

Share resources with specific users:

```json
{
  "principal": { "id": "bob" },
  "resource": {
    "type": "Photo",
    "id": "vacation.jpg",
    "ownerId": "alice",
    "sharedWith": ["bob", "charlie"]
  }
}
```

```karu
// Owner has full access
allow owner if principal.id == resource.ownerId;

// Shared users can view
allow view if
    action == "view" and
    principal.id in resource.sharedWith;
```

## Team/Group Membership

Grant access to team members:

```json
{
  "principal": {
    "id": "alice",
    "teams": ["engineering", "security"]
  },
  "resource": {
    "type": "Repository",
    "id": "backend",
    "teamAccess": {
      "engineering": ["read", "write"],
      "security": ["read"]
    }
  }
}
```

```karu
// Check if user's team has the required permission
allow access if
    resource.teamAccess[principal.teams[0]][action] == true;
```

> **Simpler approach:** Pre-compute permissions server-side.

## Nested Resources (Deep Hierarchy)

For deeply nested resources, flatten the ancestry:

```json
{
  "resource": {
    "type": "File",
    "id": "test.py",
    "ancestors": ["repo:backend", "folder:src", "folder:tests"]
  },
  "principal": {
    "resourceRoles": {
      "repo:backend": ["reader"]
    }
  }
}
```

Check if user has role on any ancestor:

```karu
// Reader on any ancestor can read
allow read if
    action == "read" and
    exists ancestor in resource.ancestors:
        "reader" in principal.resourceRoles[ancestor];
```

The `exists` quantifier iterates through each ancestor and checks if the user has the "reader" role on any of them.

## Best Practices

1. **Flatten relationships** into the input JSON
2. **Pre-compute inheritance** in your application layer
3. **Use arrays for shared access** (`sharedWith`, `teamAccess`)
4. **Include ancestor IDs** for hierarchical resources

## See Also

- [Modeling RBAC](MODELING-RBAC.md) - Role-based patterns
- [Modeling ABAC](MODELING-ABAC.md) - Attribute-based patterns
