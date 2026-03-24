> ## Documentation Index
> Fetch the complete documentation index at: https://www.osohq.com/docs/llms.txt
> Use this file to discover all available pages before exploring further.

# Model Field-Level Authorization

Field-level authorization controls access to specific parts of a resource rather than the entire resource. For example, allowing community moderators to edit usernames but not email addresses.

## When to Use Field-Level Authorization

Use field-level authorization when both conditions are true:

**1. The field depends on parent resource**: The field only makes sense in relation to its parent resource. A username belongs to an account and can't exist independently.

**2. The field identifier isn't globally unique**: `Field{"username"}` doesn't uniquely identify a specific username, every account has one.

**Counter-Examples:**

* **Files in folders:** Files can exist independently and have unique paths.
* **Comments on posts:** Comments have unique IDs and can be modeled as separate resources.

### Field-Level vs Attribute-Based Access Control (ABAC)

Field-level authorization and attribute-based access control (ABAC) serve different purposes:

* **Field-level**: Grants permissions *to* attributes (e.g., "allow editing the email field").
* **ABAC**: Grants permissions *based on* attributes (e.g., "allow if user's department = 'HR'").

# Implementation Strategies

Oso Cloud supports two strategies for field-level authorization.

## Fields in Permissions

Encode field permissions directly into the parent resource using dot notation like `"email.read"` and `"username.update"`.

* **Best for:** Simple policies with few fields
* **Pros:** Straightforward policy structure
* **Cons:** Multiplicative permissions to manage, requires client-side processing

### How It Works

Add field-specific permissions to the parent resource using dot notation. The permission name combines the field name and action: `"username.update"`, `"email.read"`.

```polar  theme={null}
resource Account {
  permissions = [
    # resource-level permissions
    "read", "update",
    # field-level permissions
    "username.read", "username.update",
    "email.read", "email.update"
  ];
}
```

### Implementation

We'll model a social app where community admins can update usernames but not emails, and visitors are limited to reading usernames without access to other field-level data.

```polar  theme={null}
actor User {}

resource Organization {
  roles = ["visitor", "member", "community_admin", "admin"];
  permissions = ["read", "update"];

  "visitor" if "member";
  "member" if "community_admin";
  "community_admin" if "admin";

  "update" if "admin";
  "read" if "visitor";
}

resource Account {
  permissions = [
    "read", "update",
    "username.read", "username.update",
    "email.read", "email.update"
  ];
  relations = { parent: Organization, owner: User };

  # Resource-level permissions
  #         relation          | read | update
  # --------------------------|------|--------
  # owner                     |   x  |    x
  # admin on parent           |   x  |    x
  # community_admin on parent |   x  |    x
  # member on parent          |   x  |    -
  # visitor on parent         |   x  |    -

  "update" if "owner";
  "update" if "admin" on "parent";
  "update" if "community_admin" on "parent";
  "read" if "update";
  "read" if "visitor" on "parent";

  # Field-level permissions
  #         relation          |   username   |     email
  # --------------------------|--------------|----------------
  # owner                     | read, update | read, update
  # admin on parent           | read, update | read, update
  # community_admin on parent | read, update | read
  # member on parent          | read         | read
  # visitor on parent         |       -      |       -

  "username.update" if "owner";
  "username.update" if "admin" on "parent";
  "username.update" if "community_admin" on "parent";
  "username.read" if "username.update";
  "username.read" if "member" on "parent";

  "email.update" if "owner";
  "email.read" if "email.update";
  "email.read" if "community_admin" on "parent";
  "email.read" if "member" on "parent";
}

test "admins can update usernames but not other fields" {
  setup {
    has_role(User{"bob"}, "admin", Organization{"acme"});
    has_relation(Account{"amy"}, "parent", Organization{"acme"});
  }

  assert allow(User{"bob"}, "username.update", Account{"amy"});
  assert_not allow(User{"bob"}, "email.update", Account{"amy"});
}


test "visitors can read account username but not other fields" {
  setup {
    has_role(User{"jim"}, "visitor", Organization{"acme"});
    has_relation(Account{"amy"}, "parent", Organization{"acme"});
  }

  assert allow(User{"jim"}, "read", Account{"amy"});
  assert_not allow(User{"jim"}, "email.read", Account{"amy"});
}
```

### Client

The benefit of fields in permissions is that you can derive field-level authorization using the actions subcommand.

To determine a community\_admin's permissions on an account that is not their own:

```bash  theme={null}
oso-cloud actions User:bob Account:alice
```

```bash  theme={null}
email.read
read
update
username.read
username.update
```

With that output, manipulate the text to determine which fields the user can read:

```bash  theme={null}
oso-cloud actions User:bob Account:alice | awk -F '.' '/.*\.read/ {print $1}'
```

```bash  theme={null}
email
username
```

Or update:

```bash  theme={null}
oso-cloud actions User:bob Account:alice | awk -F '.' '/.*\.update/ {print $1}'
```

```bash  theme={null}
username
```

## Fields as Resources

Model each field as its own resource with explicit relationships and custom `allow_field()` rules.

* **Best for:** Complex field logic with many conditional rules
* **Pros:** More flexible, integrates well with Oso's query API
* **Cons:** More complex policy rules, additional resource modeling

### How It Works

Create a separate `Field` resource and use custom `allow_field()` rules to define field-level permissions. This approach treats fields as first-class resources. In this example, community admins on an organization can update the username field on any account belonging to that organization.

```polar  theme={null}
resource Field {
  permissions = ["read", "update"];
}

allow_field(user: User, "update", account: Account, Field{"username"}) if
  has_role(user, "community_admin", org) and
  has_relation(account, "parent", org);
```

### Implementation

In this example, admins can edit any account field, community admins can only update usernames, and all members can read all fields.

```polar  theme={null}
actor User {}

resource Organization {
  roles = ["visitor", "member", "community_admin", "admin"];
  permissions = ["read", "update"];

  "visitor" if "member";
  "member" if "community_admin";
  "community_admin" if "admin";

  "update" if "admin";
  "read" if "visitor";
}
# Account permissions
#
#         relation          | read | update
# --------------------------|------|--------
# owner                     |   ✓  |    ✓
# admin on parent           |   ✓  |    ✓
# community_admin on parent |   ✓  |    ✓
# member on parent          |   ✓  |    -
# visitor on parent         |   ✓  |    -

resource Account {
  permissions = ["read", "update"];
  relations = { parent: Organization, owner: User };

  "update" if "owner";
  "update" if "community_admin" on "parent";
  "read" if "update";
  "read" if "visitor" on "parent";
}

# Field permissions
#
#         relation          | read | update
# --------------------------|------|--------
# owner                     |   ✓  |    †
# admin on parent           |   ✓  |    ✓
# community_admin on parent |   ✓  |    *
# member on parent          |   ✓  |    -
# visitor on parent         |   -  |    -
#
# †: owner can update only defined fields on their own account
# *: community_admin can update only `Field{"username"}`

resource Field {
  permissions = ["read", "update"];
  "read" if "update";
}

# Define which fields exist
has_relation(Field{"username"}, "parent", _: Account);
has_relation(Field{"email"}, "parent", _: Account);

# Allow owners to update their own fields
allow_field(user: User, "update", account: Account, field: Field) if
  has_relation(account, "owner", user) and
  has_relation(field, "parent", account);

# Allow admins to update any field, even those whose relationship with an
# account is not defined
allow_field(user: User, "update", account: Account, _field: Field) if
  org matches Organization and
  has_role(user, "admin", org) and
  has_relation(account, "parent", org);

# Allow community admins to update only usernames
allow_field(user: User, "update", account: Account, field: Field) if
  field = Field{"username"} and
  org matches Organization and
  has_role(user, "community_admin", org) and
  has_relation(account, "parent", org) and
  has_permission(user, "update", account) and
  has_relation(field, "parent", account);

# Allow members to read all fields
allow_field(user: User, "read", account: Account, field: Field) if
  org matches Organization and
  has_role(user, "member", org) and
  has_relation(account, "parent", org) and
  has_permission(user, "read", account) and
  has_relation(field, "parent", account);

test "admins can update all fields" {
  setup {
    has_role(User{"bob"}, "admin", Organization{"acme"});
    has_relation(Account{"amy"}, "parent", Organization{"acme"});
  }

  assert allow_field(User{"bob"}, "update", Account{"amy"}, Field{"username"});
  assert allow_field(User{"bob"}, "update", Account{"amy"}, Field{"email"});
}


test "community admins can only update usernames" {
  setup {
    has_role(User{"jim"}, "community_admin", Organization{"acme"});
    has_relation(Account{"amy"}, "parent", Organization{"acme"});
  }

  assert allow_field(User{"jim"}, "update", Account{"amy"}, Field{"username"});
  assert_not allow_field(User{"jim"}, "update", Account{"amy"}, Field{"email"});
}

test "members can only read fields" {
  setup {
    has_role(User{"jim"}, "member", Organization{"acme"});
    has_relation(Account{"amy"}, "parent", Organization{"acme"});
  }

  assert allow_field(User{"jim"}, "read", Account{"amy"}, Field{"username"});
  assert_not allow_field(User{"jim"}, "update", Account{"amy"}, Field{"email"});
}
```

### Client

By modeling fields as resources and introducing a new allow\_field rule, we can use the Oso client query subcommand to determine users' field-level authorization for accounts.

To determine charlie's permissions on alice's Account:

```bash  theme={null}
oso-cloud query allow_field User:bob _ Account:alice Field:_
```

```bash  theme={null}
allow_field(User:bob, String:read, Account:alice, Field:_)
allow_field(User:bob, String:update, Account:alice, Field:username)
```

bob can read any (\_) field from the alice account, but can only update the username field.

For his own account, bob can update all fields:

```bash  theme={null}
oso-cloud query allow_field User:bob _ Account:bob Field:_

allow_field(User:bob, String:read, Account:bob, Field:_)
allow_field(User:bob, String:update, Account:bob, Field:_)
allow_field(User:bob, String:update, Account:bob, Field:username)
```

The redundant update permission comes from the fact that a community\_admin can edit their own username using their community\_admin privileges in addition to the update permissions granted to the account owner.

## Choosing Between Approaches

| Factor                 | Fields in permissions   | Fields as resources          |
| ---------------------- | ----------------------- | ---------------------------- |
| **Policy complexity**  | Simple                  | More complex                 |
| **Number of fields**   | Best for few fields     | Scales well                  |
| **Client integration** | Requires parsing        | Native query support         |
| **Conditional logic**  | Limited                 | Highly flexible              |
| **Performance**        | Fewer rules to evaluate | More rules but more targeted |

**Use Fields in Permissions When:**

* You have a small, stable set of fields
* Field logic is straightforward
* You prefer simpler policies

**Use Fields as Resources When:**

* You have many fields or complex field logic
* You need flexible conditional rules
* You want native query API support

## Further Resources

Field-level authorization enables granular control within resources. Consider these other patterns for your application:

* Combine with the [ABAC patterns](/develop/policies/abac) for attribute-based field access.
* Explore [conditional roles](/develop/policies/patterns/conditional-roles) for dynamic field permissions.
