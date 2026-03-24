# Karu Language Specification

Karu is a policy language for authorization. It compiles to a [fast, embeddable
evaluator](BENCHMARKS.md) that answers one question: **given a request, should we allow or deny?**

Karu inherits ideas from [Cedar](https://www.cedarpolicy.com/) and
[Polar](https://docs.osohq.com/), but aims for a syntax that is readable by
non-engineers while remaining precise enough for formal verification.

---

## Table of Contents

1. [Quick Example](#quick-example)
2. [Lexical Structure](#lexical-structure)
3. [Formal Grammar (EBNF)](#formal-grammar-ebnf)
4. [Rules](#rules)
5. [Expressions](#expressions)
6. [Patterns](#patterns)
7. [Inline Tests](#inline-tests)
8. [Imports](#imports)
9. [Schema Mode](#schema-mode)
10. [Assertions](#assertions)
11. [Formatting](#formatting)

---

## Quick Example

```karu
# A simple document access policy

allow view if
    principal == "alice" and
    action == "view";

deny delete if
    action == "delete" and
    principal != "admin";

allow admin if
    principal == "admin";

test "alice can view" {
    principal { id: "alice" }
    action    { id: "view" }
    resource  { id: "doc1" }
    expect allow
}

test "bob cannot delete" {
    principal { id: "bob" }
    action    { id: "delete" }
    resource  { id: "doc1" }
    expect deny
}
```

---

## Lexical Structure

### Comments

Line comments start with `#` and extend to end of line.

```karu
# This is a comment
allow view; # inline comment
```

### Identifiers

Identifiers match `[a-zA-Z_][a-zA-Z0-9_]*`.

### String Literals

Strings are double-quoted with backslash escapes: `"hello \"world\""`.

### Number Literals

Numbers match `-?[0-9]+(\.[0-9]+)?([eE][+-]?[0-9]+)?`.

### Boolean Literals

`true` and `false`.

### Keywords

```
allow  deny  if  and  or  not  in  has  like
forall  exists  is  test  expect
use  import  schema  mod  assert  abstract  appliesTo  actor  resource  action  context
```

### Operators

```
==  !=  <  >  <=  >=  .  ,  :  ;  _
(  )  {  }  [  ]
```

---

## Formal Grammar (EBNF)

```ebnf
(* ============================================================ *)
(* Top-level                                                     *)
(* ============================================================ *)

program         = { rule_def | test_block
                  | "use" "schema" ";"
                  | import_def
                  | module_def
                  | assert_def } ;

(* ============================================================ *)
(* Rules                                                         *)
(* ============================================================ *)

rule_def        = effect , ident , [ "if" , expr ] , ";" ;

effect          = "allow" | "deny" ;

(* ============================================================ *)
(* Expressions                                                   *)
(* ============================================================ *)

expr            = or_expr ;

or_expr         = and_expr , { "or" , and_expr } ;

and_expr        = unary_expr , { "and" , unary_expr } ;

unary_expr      = "not" , unary_expr
                | primary_expr ;

primary_expr    = comparison
                | membership
                | in_literal
                | has_expr
                | like_expr
                | forall_expr
                | exists_expr
                | is_expr
                | ident                          (* bare ref / assert call *)
                | "(" , expr , ")" ;

comparison      = path , compare_op , pattern ;

compare_op      = "==" | "!=" | "<" | ">" | "<=" | ">="
                | "containsAll" | "containsAny"
                | "isInRange" | "isIpv4" | "isIpv6"
                | "isLoopback" | "isMulticast"
                | "decimal_lt" | "decimal_le"
                | "decimal_gt" | "decimal_ge" ;

membership      = pattern , "in" , path ;

in_literal      = path , "in" , "[" , pattern , { "," , pattern } , "]" ;

has_expr        = "has" , path ;

like_expr       = path , "like" , string ;

forall_expr     = "forall" , ident , "in" , path , ":" , expr ;

exists_expr     = "exists" , ident , "in" , path , ":" , expr ;

is_expr         = path , "is" , ident ;

(* ============================================================ *)
(* Paths                                                         *)
(* ============================================================ *)

path            = ident , { "." , ident
                           | "[" , ( number | ident ) , "]" } ;

(* ============================================================ *)
(* Patterns                                                      *)
(* ============================================================ *)

pattern         = string | number | "true" | "false"
                | ident                          (* variable or path ref *)
                | "_"                            (* wildcard *)
                | object_pattern
                | array_pattern ;

object_pattern  = "{" , [ field_pattern , { "," , field_pattern } ] , "}" ;

field_pattern   = ident , ":" , pattern ;

array_pattern   = "[" , [ pattern , { "," , pattern } ] , "]" ;

(* ============================================================ *)
(* Inline Tests                                                  *)
(* ============================================================ *)

test_block      = "test" , string , "{" , { test_item } , "}" ;

test_item       = entity_block | expect_clause ;

entity_block    = ident , "{" , { ident , ":" , test_value , "," } , "}" ;

test_value      = string | number | "true" | "false" ;

expect_clause   = "expect" , ( effect | expect_block ) ;

expect_block    = "{" , { effect , ident , "," } , "}" ;

(* ============================================================ *)
(* Imports                                                       *)
(* ============================================================ *)

import_def      = "import" , string , ";" ;

(* ============================================================ *)
(* Schema Mode  (activated by `use schema;`)                     *)
(* ============================================================ *)

module_def      = "mod" , [ ident ] , "{" , { schema_item } , "}" , ";" ;

schema_item     = entity_def | action_def | abstract_def ;

entity_def      = entity_kind , ident ,
                  [ "in" , ident , { "|" , ident } ] ,
                  [ "is" , ident , { "," , ident } ] ,
                  [ "{" , { field_def } , "}" ] , ";" ;

entity_kind     = "actor" | "resource" ;

action_def      = "action" , string , [ "appliesTo" , applies_to ] , ";" ;

applies_to      = "{" , { applies_entry } , "}" ;

applies_entry   = "actor" , type_ref , ","
                | "resource" , type_ref , ","
                | "context" , "{" , { field_def } , "}" ;

abstract_def    = "abstract" , ident , "{" , { field_def } , "}" , ";" ;

field_def       = ident , [ "?" ] , type_ref ;

type_ref        = ident                          (* named type *)
                | "Set" , "<" , type_ref , ">"   (* set type *)
                | "{" , { field_def } , "}"      (* inline record *)
                | type_ref , "|" , type_ref ;    (* union type *)

(* ============================================================ *)
(* Assertions                                                    *)
(* ============================================================ *)

assert_def      = "assert" , ident ,
                  [ "<" , ident , { "," , ident } , ">" ] ,
                  ( "if" | "is" ) , expr , ";" ;

(* ============================================================ *)
(* Terminals                                                     *)
(* ============================================================ *)

ident           = letter , { letter | digit | "_" } ;
string          = '"' , { char } , '"' ;
number          = [ "-" ] , digit , { digit } ,
                  [ "." , digit , { digit } ] ,
                  [ ( "e" | "E" ) , [ "+" | "-" ] , digit , { digit } ] ;
letter          = "a".."z" | "A".."Z" | "_" ;
digit           = "0".."9" ;
```

---

## Rules

A **rule** is the core building block. It declares whether to **allow** or
**deny** a request, optionally gated by a condition.

```karu
# Unconditional rule
allow everything;

# Conditional rule
deny delete if
    action == "delete" and
    principal != "admin";
```

### Evaluation Model

All rules are evaluated against the request. The final decision is:

1. If **any** `deny` rule matches → **deny**.
2. If **any** `allow` rule matches (and no deny) → **allow**.
3. If **no** rule matches → **deny** (default-deny).

This is identical to Cedar's evaluation model.

---

## Expressions

Expressions form the conditions in rule bodies.

### Boolean Logic

```karu
# AND - both must be true
allow edit if principal == "alice" and action == "edit";

# OR - either suffices
allow view if principal == "alice" or principal == "bob";

# NOT - negation
deny locked if not resource.active;
```

`and` binds tighter than `or`. Use parentheses to override.

### Comparison Operators

| Operator          | Meaning                                 |
| ----------------- | --------------------------------------- |
| `==`              | Equal                                   |
| `!=`              | Not equal                               |
| `<` `>` `<=` `>=` | Ordered comparison                      |
| `containsAll`     | Left set contains all elements of right |
| `containsAny`     | Left set contains any element of right  |

### Path Expressions

Dot-delimited access into request fields:

```karu
principal.role            # field access
resource.tags[0]          # array index
resource.context.args     # nested access
```

### Collection Operators

```karu
# Membership: is value in collection?
allow shared if "public" in resource.tags;

# Inline set membership
allow read if action in ["view", "list", "search"];

# Universal: all items satisfy condition
allow batch if forall item in resource.items: item.approved;

# Existential: at least one item satisfies condition
deny flagged if exists tag in resource.tags: tag == "blocked";
```

### Has (Existence Check)

```karu
allow detailed if has resource.metadata;
```

### Like (Glob Matching)

```karu
allow images if resource.path like "/images/*";
```

### Is (Type Check)

```karu
# Schema mode: check entity type
allow file_access if resource is File;
```

---

## Patterns

Patterns appear on the right side of comparisons and in `in` expressions.

```karu
# Literal values
principal == "alice"
resource.count == 42
resource.active == true

# Wildcard - matches anything
resource.owner == _

# Object pattern - structural match
resource == { type: "document", status: "active" }

# Array pattern
resource.tags == ["public", "featured"]

# Path reference - compare two paths
resource.ownerId == principal.id
```

---

## Inline Tests

Tests are declared alongside rules and run via `karu test` or live in the IDE.

### Simple Expect

```karu
test "alice can view docs" {
    principal { id: "alice", role: "viewer" }
    action    { id: "view" }
    resource  { id: "doc1", type: "document" }
    expect allow
}
```

### Per-Rule Expect

Fine-grained assertions on individual rules:

```karu
test "check specific rules" {
    principal { id: "alice" }
    action    { id: "view" }
    expect {
        allow viewRule,
        deny adminOnlyRule,
    }
}
```

The per-rule form checks each named rule individually, ignoring rules not
mentioned. The simple form checks the final policy decision.

---

## Imports

Karu files can import other `.karu` files with the `import` directive.
Imports must appear at the top of the file, after `use schema;` (if present)
and before any rules, modules, or assertions.

```karu
import "shared/common_rules.karu";
import "lib/assertions.karu";

allow view if is_authenticated;
```

### Ordering

Imports are resolved in order and all imported rules, modules, and assertions
are merged into the importing file's program.

### Constraints

1. **No circular imports** - the import graph must form a DAG. If file A
   imports file B and file B imports file A, this is a compilation error.

2. **Schema consistency** - a `use schema;` file can only import other
   `use schema;` files. However, an untyped file **may** import a typed file
   - every typed file works fine inside an untyped project.

```karu
# ✓ Valid: both files use schema
use schema;
import "types.karu";
```

```karu
# ✓ Valid: untyped file importing typed file
import "schema_types.karu";
```

```karu
# ✗ Invalid: typed file importing untyped file
use schema;
import "untyped_rules.karu";  # error!
```

### Diamond Imports

When multiple files import the same dependency, it is only included once.
This prevents duplicate rules in the merged program.

---

## Schema Mode

Activated by `use schema;` at the top of a file. Enables typed entity
declarations, action definitions, and namespace modules.

```karu
use schema;

mod MyApp {
    actor User {
        name String
        role String
    };

    resource Document in Folder {
        owner User
        title String
        tags Set<String>
    };

    abstract Ownable {
        owner User
    };

    resource Folder {};

    action "View" appliesTo {
        actor User,
        resource Document | Folder,
        context {
            authenticated Boolean
        }
    };

    action "Delete" appliesTo {
        actor User,
        resource Document,
    };
};

allow view if
    principal == resource.owner;

deny delete if
    action == "Delete" and
    not principal.role == "admin";
```

### Entity Kinds

- **`actor`** - principal entity types (users, services, roles).
- **`resource`** - target entity types (documents, folders, files).

### Inheritance

```karu
resource File in Folder { ... };    # File is contained in Folder
resource File is Ownable;           # File has Ownable's fields
```

### Types

| Type              | Description      |
| ----------------- | ---------------- |
| `String`          | Text value       |
| `Boolean`         | `true` / `false` |
| `Long`            | Integer          |
| `Set<T>`          | Set of values    |
| `{ field: Type }` | Inline record    |
| `A \| B`          | Union type       |

Optional fields use `?`: `nickname? String`.

---

## Assertions

Reusable named conditions, inlined at compile time:

```karu
assert is_owner if principal.id == resource.ownerId;

# With type parameters (documentation only)
assert has_role<User, File> if principal.role in resource.allowedRoles;

# Used in rules
allow edit if is_owner and action == "edit";
allow manage if has_role;
```

---

## Formatting

Karu ships with a built-in formatter (`karu fmt`). Canonical style:

- 4-space indentation for rule bodies and test blocks.
- 8-space indentation for entity fields inside test blocks.
- Trailing commas in entity fields.
- `expect` on its own line, no trailing punctuation.
- One blank line between top-level items (rules, tests).

---

## Test Fixtures

The repository includes a collection of `.karu` files that exercise all
language features, located at
[`./tests/lsp_fixtures/`](tests/lsp_fixtures/). These serve as both
integration tests and living examples of the syntax:

| File                      | Covers                                           |
| ------------------------- | ------------------------------------------------ |
| `valid_simple.karu`       | Basic rules                                      |
| `valid_complex.karu`      | Multi-expression conditions, quantifiers         |
| `valid_with_tests.karu`   | Inline test blocks                               |
| `schema_basic.karu`       | Schema mode with modules and entities            |
| `schema_with_assert.karu` | Assertions and abstract types                    |
| `schema_with_tests.karu`  | Schema + inline tests together                   |
| `invalid_*.karu`          | Intentional syntax errors for diagnostic testing |
| `formatting_messy.karu`   | Unformatted input for formatter testing          |

Run `cargo test --test lsp_snapshot_tests --features lsp` to verify them.
