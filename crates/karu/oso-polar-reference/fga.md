> ## Documentation Index
> Fetch the complete documentation index at: https://www.osohq.com/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# Fine-grained Authorization (FGA)

Fine-grained authorization (FGA) controls which users can perform specific actions on **individual resources**, often at the **row level** in a database.

It differs from **coarse-grained authorization**, which grants access to all resources of a type, and from [**field-level authorization**](/develop/policies/field-level-authorization), which governs access to specific **fields (columns)** within a row and is more granular.

FGA can be modeled with **RBAC**, **ReBAC**, **ABAC**, or a combination of all three.

## Fine-grained authorization (FGA) with roles

A common implementation pattern for FGA is permit users access to resources
based on the user's role. For example, an administrator can edit all resources
while a member can only view those resources.

Consider the following policy in Polar.

```polar  theme={null}
# policy.polar
actor User {}

resource Item {
  roles = ["admin", "member"];
  permissions = [
    "edit", "view"
  ];

  # organization level permissions
  "view" if "member";
  "edit" if "admin";

  # role hierarchy:
  # admins inherit all member permissions
  "member" if "admin";
}
```

In this example, `User` who is an `admin` can `edit` all `Items`. A `User` who is
a `member` can only `view` all `Items`.

To learn more about implementing this FGA pattern, check out
[our guide on RBAC](/develop/policies/rbac).

## Fine-grained authorization (FGA) with relationships

Often, FGA is implemented based on the relationships between users and
resources. For example, a user can edit all resources the user owns, while
other users can only view those resources.

Consider the following policy in Polar.

```polar  theme={null}
actor User {}

resource Item {
  roles = ["owner", "viewer"];
  permissions = ["view", "edit"];
  relations = {
    item: Item, creator: User
  };

  "viewer" if "creator";
  "owner" if "creator";

  "view" if "viewer";
  "edit" if "creator" on resource;
}
```

In this example, a `User` who is the `creator` of an `Item` can edit. A `User`
who is a `viewer` can only view the `Item`.

To learn more about implementing this FGA pattern, check out [our guide on
ReBAC](/develop/policies/rebac).

## Fine-grained authorization (FGA) with attributes

FGA can also be implemented to permit users access to resources based on an
attribute of the user. For example, any user can view a resource if it is
public, while if it is private they cannot.

Consider the following policy in Polar.

```polar  theme={null}
actor User {}

resource Item {
  permissions = ["view"];
  
  "view" if is_public(resource);
}
```

In this example, a `User` can view an `Item` if it `is_public`. If the `Item`
is not public, the user cannot.

Attributes that can be leveraged to build permissions can include time based
checks. For example, a user can be entitled to view an item only for a certain
period of time.

Consider the following policy in Polar.

```polar  theme={null}
actor User {}

resource Item {
  roles = ["viewer"];
  permissions = ["view"];

  "view" if "viewer";
}

has_role(actor: Actor, role: String, item: Item) if
  expiration matches Integer and
  has_role_with_expiry(actor, role, item, expiration) and
  expiration > @current_unix_time;
```

To learn more about implementing this FGA pattern, check out [our guide on
ABAC](/develop/policies/abac).

## Combining fine-grained authorization (FGA) implementation patterns

These FGA implementation patterns are not exclusive. They can be used in
combination to control access to specific resources on roles, relationships,
and attributes.

For example, a resource that is premium should be able to be viewed by
subscribers while unsubscribed users should not.

Consider the following policy in Polar.

```polar  theme={null}
actor User{}

resource Item {
  roles = ["subscriber", "member"];
  permissions = ["view"];
}

is_premium(item: Item) if
    has_fact("is_premium", item, true);

allow(user: User, "read", item: Item) if
  has_role(user, "subscriber") and
  is_premium(item);
```

In this example, RBAC and ABAC are combined to permit a `User` to `view` an
`Item` that `is_premium` only if the user has the `subscriber` role.

The combination of FGA patterns (RBAC, ReBAC, ABAC) can model complex authorization policies to decide which users can access which resources. However, FGA typically governs access to entire resources, not to individual fields within them. To control access at the field level, see our guide on [field-level authorization](/develop/policies/field-level-authorization).

## Next Steps

Explore these topics next:

* [Add facts](/develop/facts/overview) - learn how to add your authorization
  data to Oso Cloud.
* [Field Level Authorization](/develop/policies/field-level-authorization) -
  learn to control authorization even deeper with permissions on attributes of
  resources.
* [Get started](/get-started/quickstart) - Learn how to take your first steps
  learning Oso.
