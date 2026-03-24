# Karu for Cedar Developers

A guide to expressing Cedar policies in Karu's Polar-inspired syntax.

## Overview

Both Cedar and Karu are authorization policy languages. While Cedar uses a declarative entity-attribute model, Karu uses JSON pattern matching with a more programmatic feel. This guide shows how to translate common Cedar patterns into Karu.

## Key Differences

| Aspect                | Cedar                                        | Karu                                                    |
| --------------------- | -------------------------------------------- | ------------------------------------------------------- |
| **Input Model**       | Typed entities (Principal, Action, Resource) | Flat JSON with `principal`, `action`, `resource` fields |
| **Type System**       | Schema-enforced types                        | Duck typing (structural matching)                       |
| **Syntax**            | `permit`/`forbid` with `when`/`unless`       | `allow`/`deny` with `if` conditions                     |
| **Entity References** | `User::"alice"`                              | `principal == "alice"`                                  |
| **Collections**       | `resource in Album::"vacation"`              | `resource.album == "vacation"`                          |
| **OR Conditions**     | Native `action in [...]`                     | Multiple rules or Rust API                              |

## Policy Translation Examples

### Individual Entity Access

**Cedar:**

```
permit(principal == User::"alice", action == Action::"view",
       resource == Photo::"VacationPhoto94.jpg");
```

**Karu:**

```
allow access if
    principal == "alice" and
    action == "view" and
    resource == "VacationPhoto94.jpg";
```

> **Note:** Cedar's typed entity references (`User::"alice"`) become simple string matches.

---

### Group Membership

**Cedar:**

```
permit(principal in Group::"alice_friends", action == Action::"view",
       resource == Photo::"VacationPhoto94.jpg");
```

**Karu:**

```
allow access if
    "alice_friends" in principal.groups and
    action == "view" and
    resource == "VacationPhoto94.jpg";
```

> **Note:** Cedar uses entity hierarchy (`in Group`); Karu uses array membership.

---

### Resource Containers

**Cedar:**

```
permit(principal == User::"alice", action == Action::"view",
       resource in Album::"alice_vacation");
```

**Karu:**

```
allow access if
    principal == "alice" and
    action == "view" and
    resource.album == "alice_vacation";
```

> **Note:** Cedar's hierarchy is modeled via JSON attributes.

---

### Multiple Actions

**Cedar:**

```
permit(principal == User::"alice",
       action in [Action::"view", Action::"edit", Action::"delete"],
       resource in Album::"alice_vacation");
```

**Karu:** _(use multiple rules)_

```
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
```

> **Note:** Cedar's inline action lists require separate rules in Karu's DSL.

---

### Any Principal (Wildcards)

**Cedar:**

```
permit(principal, action == Action::"view",
       resource in Album::"alice_vacation");
```

**Karu:**

```
allow access if
    action == "view" and
    resource.album == "alice_vacation";
```

> **Note:** Omit conditions to match any value (implicit wildcard).

---

### Attribute-Based Access (ABAC)

**Cedar:**

```
permit(principal, action in [Action::"listPhotos", Action::"view"],
       resource in Album::"device_prototypes")
when {
    principal.department == "HardwareEngineering" &&
    principal.jobLevel >= 5
};
```

**Karu:**

```
allow access if
    action == "view" and
    resource.album == "device_prototypes" and
    principal.department == "HardwareEngineering" and
    principal.jobLevel >= 5;
```

> **Note:** Karu integrates `when` conditions directly in the rule body.

---

### Path-to-Path Comparison

**Cedar:**

```
permit(principal, action, resource) when { principal == resource.owner };
```

**Karu:**

```
allow access if
    principal.id == resource.ownerId;
```

> **Note:** Karu supports comparing two paths directly using `PathRef` patterns.

---

### Owner OR Admin Access

**Cedar:**

```
permit(principal, action, resource)
when { principal == resource.owner || resource.admins.contains(principal) };
```

**Karu:**

```
allow owner if principal.id == resource.ownerId;
allow admin if principal.id in resource.adminIds;
```

> **Note:** OR is handled via multiple rules that independently match.

---

### Deny Policies

**Cedar:**

```
forbid(principal, action, resource)
when { resource.private }
unless { principal == resource.owner };
```

**Karu:**

```
allow general;
deny private if
    resource.private == true and
    not principal.id == resource.ownerId;
```

> **Important:** Karu follows "deny overrides" semantics - a matching `deny` always wins.

---

## Evaluation Semantics

| Cedar                             | Karu                         |
| --------------------------------- | ---------------------------- |
| `permit` + `forbid` → forbid wins | `allow` + `deny` → deny wins |
| No match → default deny           | No match → default deny      |
| `unless { cond }`                 | `not cond` in deny rule      |

## Path-in-Path Membership

Karu supports dynamic membership checks where both sides are paths:

**Cedar:**

```
permit(principal, action, resource)
when { resource.admins.contains(principal.id) };
```

**Karu:**

```
allow admin_access if principal.id in resource.adminIds;
```

> **Note:** The left path value is looked up in the array at the right path.

## Context-Based Access

Cedar's `context` record passes request-time attributes (IP, device, time). Karu supports this as well:

**Cedar:**

```
permit(principal, action, resource)
when {
    context.ipAddress == ip("10.0.0.0/16") &&
    context.readOnly == false
};
```

**Karu:**

```
allow access if
    context.ipAddress == "10.0.0.1" and
    context.readOnly == false;
```

> **Note:** When using `--strict` mode, `context` equality checks are extracted and placed in the Cedar `when { }` block during transpilation.

## Complete Example: Photo Sharing Policy

**Cedar:**

```
permit(principal, action, resource)
when { principal == resource.owner };

permit(principal, action == Action::"view", resource)
when { resource.sharedWith.contains(principal) };

forbid(principal, action, resource)
when { resource.private }
unless { principal == resource.owner };
```

**Karu:**

```
# Owners can do anything
allow owner_access if principal.id == resource.ownerId;

# Friends can view
allow friend_view if
    action == "view" and
    principal.id in resource.sharedWith;

# Deny access to private photos for non-owners
deny private_block if
    resource.private == true and
    not principal.id == resource.ownerId;
```

## Input Data Format

Karu policies evaluate against JSON. Structure your input like:

```json
{
  "principal": {
    "id": "alice",
    "department": "Engineering",
    "groups": ["admins"]
  },
  "action": "view",
  "resource": {
    "type": "Photo",
    "name": "vacation.jpg",
    "ownerId": "alice",
    "private": false,
    "sharedWith": ["bob", "charlie"]
  },
  "context": { "readOnly": true }
}
```

## Next Steps

1. See [tests/cedar_equivalents.rs](../tests/cedar_equivalents.rs) for runnable examples
2. Read the main [README.md](../README.md) for Karu fundamentals
3. Explore the Rust API in [src/lib.rs](../src/lib.rs)
