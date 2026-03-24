# How should `use schema` work in karu?

## Karu's Goals

Karu is a policy language that aims to be fast to write and even faster to evaluate. Karu is not normally typed, but developers may opt in to a typed mode use a special syntax. The typed mode is what enables lossless conversion to and from Cedar.

## Typed Karu?

### Fast Switching Into Typed Mode

In our first release we will ship first class support for Cedar interoperability. This means in we need to be able to parse Cedar policies from ".cedar" files and Cedar schemas from ".cedarschema" files. In Karu we don't force types, but we have a mode to enable it that can be activated by a special instruction `use schema;` at the top of a file. A .karu file may include comments and whitespace before `use schema;` directive, but it must include no other language constructs.

```karu
use schema;
```

### How Should Typed Karu Be Stored?

Typed Karu is still Karu, so it should be stored in ".karu" files.

In Cedar, policies and schemas are stored in different files, with the ".cedar" extension and ".cedarschema" extension respectively.

### The Shape Of Our Solution

Karu features two parsers, one for runtime and one for devtime. The runtime parser is designed to parse fast and fail fast, while the devtime parser is designed to provide rich diagnostics and developer assistance. When introducing support for a third-language, such as Cedar, we likewise **must** the same; a parse fast, fail fast runtime parser and a rich devtime parser. In Cedar's case we actually have to support **two** syntaxes, one for policies and one for schemas. Whether this can be implemented in a unified `.cedar` and `.cedarschema` parser, and a likewise unified devtime parser is an exercise for the reader. It is not immediately obvious that supporting Cedar with 1 + 1 is better or worse that 2 + 2; the policy and schema languages are simple enough that it may work to combine them.

### How Should We Build It?

Our goal is to the fastest at runtime. Without adding types, Karu in WASM clocks in faster that Cedar native in every benchmark we've done! Adding support for types has a measurable cost, and it's much worse when we don't carefully consider the runtime hotpaths.

When Karu parses a .karu file at runtime we should take the shortest path and assume types are not used. If and only if we encounter a legal `use schema;` directive will we activate the typed mode and parse the file as a typed Karu file. In normal mode fewer syntax features are available, in typed mode there is more work to do.

When `use schema;` is detected the following syntax is allowed:

- `mod <namespace> { <decls> }`
- `actor <name> { <fields> }`
- `resource <name> { <fields> }`
- `action <name> { <fields> }`

There is also an additional `assert ...` syntax that we are introducing for both typed and untyped Karu. This feature is effectively a macro that is inlined at compile time. However, `assert` behaves differently in typed and untyped Karu although in every case, the inline pseudo-function that assert produces has the same signature `(actor, action, resource, context) -> Boolean`.

In untyped Karu `assert` allows a developer to shorthand a named fact, for example:

```karu
# actor, action, resource and context are automatically provided by the runtime
assert user_is_owner if actor.name == resource.owner.name;

# use the named fact in a policy
allow Delete if user_is_owner;

# expands to:
allow Delete if actor.name == resource.owner.name;
```

In typed Karu `assert` allows a developer to shorthand a named fact, but they may also specify types, for example:

```karu
# actor, action, resource and context are automatically provided by the runtime, but here the user has specified User and File
assert user_is_owner<User, action, File> if actor.name == resource.owner.name;

# use the named fact in a policy
allow delete if user_is_owner;

# expands to:
allow delete if actor.name == resource.owner.name;
```

The types optionally given to `assert` must be derived from the runtime types `actor`, `action`, `resource` and `context`, and they must be provided in the correct order, although omitting the types is allowed, as the following examples illustrate:

```karu
# untyped, valid because normal mode doesn't care about types
assert user_is_owner if actor.name == resource.owner.name;

# typed, valid assuming `actor User` and `resource File`, `action` is superfluous as all actions are `action`
assert user_is_owner<User, action, File> if actor.name == resource.owner.name;

# typed, valid assuming `actor User` and `resource File`
assert user_is_owner<User, File> if actor.name == resource.owner.name;

# typed, invalid because `.name` is not a known field of base type `actor`
assert user_is_owner<File> if actor.name == resource.owner.name;
```

Here is an example of a typed `.karu` file:

```karu
# new keyword syntax tells the parser (runtime and devtime) to expect schema
# indicated by "mod" keyword(s), otherwise the file is as usual
use schema;

# note: a karu file can contain both schema and policy definitions at the same time!
# this is useful for small policies, but for larger policies it's recommended
# to use separate files, in which case we'd do:
#   import "schema.karu";
#   import "policy.karu";
# and so on...

# this "mod" should be cedar namespace compatible!
mod MyCedarNamespace {

  actor User {
    name String,
  };

  resource Folder {};

  resource File in Folder {
    owner    User,
    name     String,
    modified DateTime,
  };

  # action name can be in optional double quotes
  action "Delete" appliesTo {
    actor User,
    resource File | Folder,
    context {
      authenticated Boolean,
      somethingOptional? String,
      somethingNullable String | null,
    }
  };

  # the "abstract" defines a part of a type and in works like cedar schema "type" keyword ie. can be used with actor, resource and action
  abstract SomeFields {
    field1 String,
    field2 Int,
  }
};

# assert keyword makes a "macro" fact that is inlined at compile time
# the <User, action, File> is the minimum compatible type signature
assert user_is_owner<User, action, File> if actor.name == resource.owner.name;

# without <Types> the assertion can be used with any type that has the fields
assert user_has_roles if actor has roles;

# eg. using a typed action directly
# here karu knows that `MyCedarNamespace:Delete` is an action and knows the shape of the context because `appliesTo` is specified
allow MyCedarNamespace:Delete if context.authenticated and user_is_owner;

# eg. using a typed action with assert
assert can_delete_file<User, action, File> if MyCedarNamespace:Delete and user_is_owner;
allow delete_file if can_delete_file;

# eg. as above but this time invalid because `Folder` doesn't have `owner`
assert can_delete_folder_invalid<User, action, Folder> if MyCedarNamespace:Delete and user_is_owner; # <- won't work, we don't have `owner` in `Folder`
allow delete_folder_invalid if can_delete_folder_invalid;

# eg. again as above
assert can_delete_any_invalid if MyCedarNamespace:Delete and user_is_owner; # <- won't work, we don't have `owner` in `Folder`
allow delete_any_invalid if can_delete_any_invalid;

# eg. again as above but this time valid because we test for owner in the action
assert can_delete_any_valid if MyCedarNamespace:Delete and resource has owner and resource.owner == actor;
allow delete_any_valid if can_delete_any_valid;
```

### On Runtime Performance

The fast path for Karu when untyped is fairly clear, but for typed Karu we need to hoist type validation to the runtime. Type validation ought be as straightforward as walking an object and asserting the primitive types of leaf nodes match. There are ultimately only a few primitive types required for Cedar compatibility ie. `String`, `Long`, `Boolean`, `Set<T>` .

### On A Formal Grammar For Karu Schemas

For the time being the grammar for schemas in .karu files is equivalent to Cedar schema grammar, but with different keywords:

| Karu     | Cedar     | Notes                                                                                                        |
| -------- | --------- | ------------------------------------------------------------------------------------------------------------ |
| mod      | namespace |                                                                                                              |
| actor    | principal |                                                                                                              |
| action   | action    |                                                                                                              |
| resource | resource  |                                                                                                              |
| abstract | type      | abstract can be used as a trait to extend a type with additional fields with `is` keyword (see `is Ownable`) |

Note: In typed Karu, `actor`, `action`, `resource` and `context` are types when used in an `assert`, but keywords when used in a `mod`:

```karu
mod MyCedarNamespace {
  actor User;
  abstract Ownable {
    owner User,
  }
  resource File is Ownable;
}

# eg. we check early if resource is a File, this is idiomatic in karu
assert user_is_file_owner<User, action, File> if resource.Owner == actor;

# eg. we check later if resource is a File, this wouldn't be idiomatic in karu
assert user_is_resource_owner<User, action, resource> resource is File and resource.Owner == actor;

# eg. we check later if resource is an Ownable, this is idiomatic in karu (using abstracts as traits is fun!)
assert user_is_ownable_owner<User, action, Ownable> resource.Owner == actor;
```

Additionally, Cedar schema recognises a number of built-in types\*. Karu will use support these same types but in lowercase:

| Karu     | Cedar      | Notes                                 |
| -------- | ---------- | ------------------------------------- |
| bool     | Boolean    |                                       |
| long     | Long       |                                       |
| string   | String     |                                       |
| T[]      | Set<T>     | may be nested                         |
| datetime | datetime() | an Extension in Cedar, native in Karu |
| decimal  | decimal()  | an Extension in Cedar, native in Karu |
| duration | duration() | an Extension in Cedar, native in Karu |
| ip       | ip()       | an Extension in Cedar, native in Karu |

\*record types are functionally compatible with Karu already.

Literal values are supported in the typical way:

| Karu    | Cedar   |
| ------- | ------- |
| true    | true    |
| false   | false   |
| null    | null    |
| 123     | 123     |
| "hello" | "hello" |

### Notes

In many examples in this document, fully qualified types are not used for brevity.

In practice full names would be required:

```karu
mod MyCedarNamespace {
  actor User {
    name String,
  }
}

# strictly valid
allow letBobIn if MyCedarNamespace:User and actor.name == "Bob";

# strictly invalid, but the LSP should provide a quick fix
allow letBobIn if User and actor.name == "Bob";
```

There is however one notable exception, though technically it's not an exception, it's just not Cedar compatible:

```karu
# unnamed file local mod
mod {
  actor User {
    name String,
  }
}

# strictly valid, because User only exists in this file!
allow letBobIn if User and actor.name == "Bob";
```

### Multifile Projects

Karu doesn't have an `export` syntax. Any `.karu` file can import any other `.karu` file but:

1. An import must not create a circular dependency
2. A typed file cannot import an untyped file - once a the directive `use schema;` is used, all imports must use the same directive.

### Definition Of Done

Some Cedar kitchen sink examples are here: https://github.com/cedar-policy/cedar-examples/tree/main/cedar-example-use-cases

- [ ] We can ingest .cedar files correctly
- [ ] We can ingest .cedarschema files correctly
- [ ] We can round trip a kitchen sink .cedar file to a .karu file and back to .cedar
- [ ] We can round trip a kitchen sink .cedar and .cedarschema file pair to a single .karu file and back to .cedar and .cedarschema

### References

For more specific details on Cedar schema see https://docs.cedarpolicy.com/schema/human-readable-schema.html

The Cedar policy grammar is as follows:

```ebnf-ish
Policy ::= {Annotation} Effect '(' Scope ')' {Conditions} ';'
Effect ::= 'permit' | 'forbid'
Scope ::= Principal ',' Action ',' Resource
Principal ::= 'principal' [(['is' PATH] ['in' (Entity | '?principal')]) | ('==' (Entity | '?principal'))]
Action ::= 'action' [( '==' Entity | 'in' ('[' EntList ']' | Entity) )]
Resource ::= 'resource' [(['is' PATH] ['in' (Entity | '?resource')]) | ('==' (Entity | '?resource'))]
Condition ::= ('when' | 'unless') '{' Expr '}'
Expr ::= Or | 'if' Expr 'then' Expr 'else' Expr
Or ::= And {'||' And}
And ::= Relation {'&&' Relation}
Relation ::= Add [RELOP Add] | Add 'has' (IDENT | STR) | Add 'like' PAT | Add 'is' Path ('in' Add)?
Add ::= Mult {('+' | '-') Mult}
Mult ::= Unary { '*' Unary}
Unary ::= ['!' | '-']x4 Member
Member ::= Primary {Access}
Annotation ::= '@' ANYIDENT ( '('STR')' )?
Access ::= '.' IDENT ['(' [ExprList] ')'] | '[' STR ']'
Primary ::= LITERAL
           | VAR
           | Entity
           | ExtFun '(' [ExprList] ')'
           | '(' Expr ')'
           | '[' [ExprList] ']'
           | '{' [RecInits] '}'
Path ::= IDENT {'::' IDENT}
Entity ::= Path '::' STR
EntList ::= Entity {',' Entity}
ExprList ::= Expr {',' Expr}
ExtFun ::= [Path '::'] IDENT
RecInits ::= (IDENT | STR) ':' Expr {',' (IDENT | STR) ':' Expr}
RELOP ::= '<' | '<=' | '>=' | '>' | '!=' | '==' | 'in'
ANYIDENT ::= ['_''a'-'z''A'-'Z']['_''a'-'z''A'-'Z''0'-'9']*
IDENT ::= ANYIDENT - RESERVED
STR ::= Fully-escaped Unicode surrounded by '"'s
PAT ::= STR with `\*` allowed as an escape
LITERAL ::= BOOL | INT | STR
BOOL ::= 'true' | 'false'
INT ::= '-'? ['0'-'9']+
RESERVED ::= BOOL | 'if' | 'then' | 'else' | 'in' | 'like' | 'has' | 'is' | '__cedar'
VAR ::= 'principal' | 'action' | 'resource' | 'context'
```

The Cedar schema grammar is as follows:

```ebnf-ish
Annotation := '@' IDENT '(' STR ')'
Annotations := {Annotations}
Schema    := {Namespace}
Namespace := (Annotations 'namespace' Path '{' {Decl} '}') | Decl
Decl      := Entity | Action | TypeDecl
Entity    := Annotations 'entity' Idents ['in' EntOrTyps] [['='] RecType] ['tags' Type] ';' | Annotations 'entity' Idents 'enum' '[' STR+ ']' ';'
Action    := Annotations 'action' Names ['in' RefOrRefs] [AppliesTo]';'
TypeDecl  := Annotations 'type' TYPENAME '=' Type ';'
Type      := Path | SetType | RecType
EntType   := Path
SetType   := 'Set' '<' Type '>'
RecType   := '{' [AttrDecls] '}'
AttrDecls := Annotations Name ['?'] ':' Type [',' | ',' AttrDecls]
AppliesTo := 'appliesTo' '{' AppDecls '}'
AppDecls  := ('principal' | 'resource') ':' EntOrTyps [',' | ',' AppDecls]
           | 'context' ':' (Path | RecType) [',' | ',' AppDecls]
Path      := IDENT {'::' IDENT}
Ref       := Path '::' STR | Name
RefOrRefs := Ref | '[' [RefOrRefs] ']'
EntTypes  := Path {',' Path}
EntOrTyps := EntType | '[' [EntTypes] ']'
Name      := IDENT | STR
Names     := Name {',' Name}
Idents    := IDENT {',' IDENT}

IDENT     := ['_''a'-'z''A'-'Z']['_''a'-'z''A'-'Z''0'-'9']*
TYPENAME  := IDENT - RESERVED
STR       := Fully-escaped Unicode surrounded by '"'s
PRIMTYPE  := 'Long' | 'String' | 'Bool'
WHITESPC  := Unicode whitespace
COMMENT   := '//' ~NEWLINE* NEWLINE
RESERVED  := 'Bool' | 'Boolean' | 'Entity' | 'Extension' | 'Long' | 'Record' | 'Set' | 'String'
```
