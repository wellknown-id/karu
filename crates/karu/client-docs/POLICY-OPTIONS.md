# Policy Language Requirements for kodus

This document outlines the policy language requirements for kodus, enabling stakeholders to evaluate alternatives to our current Cedar implementation.

## Executive Summary

kodus is a security-first WebAssembly runtime with built-in package management and NATS-based federation. Policy evaluation is a **core architectural pillar**—not a bolt-on feature. The policy language governs:

- **Filesystem access** (read/write/create/delete)
- **Network access** (HTTP fetch with domain whitelisting)
- **Process execution** (subprocess spawning)
- **Module invocation** (RPC calls between WASM modules)
- **Package permissions** (install-time consent for package capabilities)

Any alternative must satisfy the requirements below without compromising security, performance, or developer experience.

---

## Core Requirements

### 1. Principal-Action-Resource-Context (PARC) Model

The policy language must support fine-grained decisions based on four dimensions:

| Dimension     | Description                      | Examples                                                   |
| ------------- | -------------------------------- | ---------------------------------------------------------- |
| **Principal** | The entity making the request    | `Module::"app.wasm"`, `User::"alice"`                      |
| **Action**    | The operation being performed    | `read`, `write`, `create`, `invoke`                        |
| **Resource**  | The target of the operation      | `File::"/tmp/data.json"`, `URL::"https://api.example.com"` |
| **Context**   | Environmental/runtime attributes | `timestamp`, `hour_utc`, `day_of_week`, CLI arguments      |

**Current usage**: Every policy decision in kodus evaluates all four dimensions. Partial models (e.g., action-only RBAC) are insufficient.

---

### 2. Permit/Forbid Semantics with Explicit Deny Override

The language must support:

- **Permit rules**: Explicitly allow specific operations
- **Forbid rules**: Explicitly deny operations (must override permits)
- **Default deny**: If no rule matches, the request is denied

**Example pattern** (protecting sensitive files):

```
// Permit general file reads
permit(principal, action == read, resource)
  when { resource.type == "file" };

// But forbid reading .env or .git (overrides the permit)
forbid(principal, action, resource)
  when { resource.path like "*/.env" || resource.path like "*/.git/*" };
```

**Criticality**: This is essential for "principle of least privilege"—allowing broad access with surgical blacklists.

---

### 3. Attribute-Based Conditions (ABAC)

Policies must support conditional expressions based on attributes:

#### Resource Attributes

- `resource.path` - filesystem path
- `resource.host` - URL hostname for network requests
- `resource.command` - executable name for subprocess spawning
- `resource.args` - command arguments for process execution

#### Context Attributes

- `context.timestamp_unix` - current Unix timestamp
- `context.hour_utc` - hour of day (0-23)
- `context.day_of_week` - weekday (0=Sunday, 6=Saturday)
- `context.arguments["0"]` - first argument to an invoked function
- `context.resource.pathInCwd` - boolean flag (computed by host)

**Example** (time-based restriction):

```
permit(principal, action == invoke, resource)
  when { context.hour_utc >= 9 && context.hour_utc < 17 };  // Business hours only
```

---

### 4. Pattern Matching for Paths and URLs

The language must support wildcard pattern matching (not just equality):

| Pattern                                         | Matches                        |
| ----------------------------------------------- | ------------------------------ |
| `resource.path like "*/tmp/*"`                  | Any file under a tmp directory |
| `resource.path like "*/.env"`                   | Any .env file                  |
| `resource.url like "https://api.example.com/*"` | API subdomain requests         |

**Critical limitation to document**: Cedar's `like` operator requires **string literals**—it cannot use variables. This means dynamic scoping (e.g., "allow access under current working directory") requires **host-side prefix validation** with a boolean context flag like `context.resource.pathInCwd == true`.

Any alternative must either:

1. Support variable patterns (preferred), or
2. Accept the same host-side workaround pattern

---

### 5. Typed Entity System

Principals, actions, and resources are **typed entities**, not bare strings:

- `Module::"app.wasm"` (type = Module, id = app.wasm)
- `Action::"filesystem:read"` (type = Action, id = filesystem:read)
- `File::"/etc/passwd"` (type = File, id = /etc/passwd)
- `Resource::"fetch"` (type = Resource, id = fetch)

This enables:

- Type-safe policy authoring
- Clear separation between different entity categories
- Extensibility (new resource types without language changes)

---

### 6. Container-like Membership (Optional but Valuable)

Support for set membership via `in` operator:

```
// Resource belongs to a directory hierarchy
resource in Directory::"/home/user/projects"
```

This simplifies hierarchical scoping compared to path pattern matching.

---

### 7. Record/Structured Data Support

Policies must access structured data, particularly for function arguments:

```
// Check first argument of an invoked function
context.arguments["0"].host == "api.example.com"

// Check if command arguments contain a flag
resource.args.contains("-m")
```

**Current usage**: WASIP2 fetch policies extract URL components (scheme, host, port, path, query) into structured context data.

---

## Operational Requirements

### 8. Go Native Library

kodus is written in Go. The policy engine **must** provide:

- A Go library (not just CLI or external service)
- Synchronous evaluation (blocking call returns decision immediately)
- In-process evaluation (no network calls to policy server)

**Rationale**: Policy decisions are on the hot path for every filesystem and network operation. External service latency is unacceptable.

---

### 9. AST Introspection API

kodus uses AST introspection to:

1. **Generate human-readable policy summaries** for `consent show` commands
2. **Avoid the "Comment Trap"**: Ensuring reported permissions reflect only _active_ rules, not commented-out examples
3. **Extract permissions programmatically** for automated tooling

**Required capabilities**:

- Parse policy text into AST
- Walk/iterate over policies
- Extract principal, action, resource scopes from each rule
- Identify condition expressions

---

### 10. Performance Suitable for WASI Interception

Policy evaluation happens at the WASI boundary—potentially thousands of times per module invocation:

| Operation                | Tolerable Latency |
| ------------------------ | ----------------- |
| Single policy evaluation | < 100μs           |
| Cached decision lookup   | < 1μs             |

kodus implements a **decision cache** keyed by `(path, action, policyRef)`. Any alternative should support similar caching strategies.

---

### 11. Composable Policy Sets

Policies come from multiple sources with layered evaluation:

1. **Built-in policies**: `ro` (read-only), `rw` (read-write)
2. **Named policies**: From `~/.kodus/policies/<name>.cedar`
3. **Project-local policies**: From `.kodus/policies.cedar`
4. **Package policies**: From `run.cedar` in installed packages

The engine must support combining/merging multiple policy sets with predictable composition semantics.

---

### 12. Consent and Cryptographic Signing Integration

kodus's consent system stores:

- Policy hash (SHA256)
- Ed25519 signature from user's identity key
- Timestamp and user identifier

The policy language itself doesn't handle signing, but the implementation must produce **deterministic hashes** from policy content (canonical serialization).

---

### 13. Policy Recording and Suggestion

kodus includes a **Policy Recorder** that:

1. Captures policy denials during "record mode"
2. Saves violations as structured JSON
3. Generates suggested `permit` rules from violations

Any alternative must support:

- Structured diagnostics on denial (which policy denied? why?)
- Sufficient information to regenerate equivalent permit rules

---

## Developer Experience Requirements

### 14. Human-Readable Syntax

Policies are authored by developers and reviewed during consent. The syntax should be:

- Readable without deep language knowledge
- Obvious in intent (what does this policy allow/deny?)
- Editable in standard text editors

**Negative example** (too cryptic):

```
p(*, r, *) :- r.t = "f", r.p =~ "^/tmp"
```

**Acceptable example**:

```
permit(principal, action == read, resource)
  when { resource.type == "file" && resource.path like "/tmp/*" };
```

---

### 15. Error Messages and Validation

The policy engine must provide:

- **Syntax validation** with line/column numbers
- **Type checking** (e.g., comparing string to number should error)
- **Clear error messages** for invalid policies

Policies are user-provided via `.kodus/policies.cedar`—runtime parsing errors must be actionable.

---

### 16. Documentation and Learning Resources

The language should have:

- Official reference documentation
- Pattern/cookbook examples
- Active maintenance (not abandoned)

---

## Current Cedar Features We Use

For reference, here are specific Cedar features kodus relies on:

| Feature                    | Usage                             |
| -------------------------- | --------------------------------- |
| `permit` / `forbid`        | All policies                      |
| `when` clauses             | Attribute-based conditions        |
| `unless` clauses           | Negative conditions               |
| `like` operator            | Path/URL pattern matching         |
| `==` equality              | Entity and attribute comparison   |
| `&&`, `\|\|`               | Boolean logic in conditions       |
| `.contains()`              | Set membership (limited)          |
| Entity UIDs (`Type::"id"`) | All principal/resource references |
| `cedar.PolicySet` (Go)     | Loading/parsing policies          |
| `ps.IsAuthorized()` (Go)   | Policy evaluation                 |
| `ast.Policy` (Go)          | AST introspection                 |

---

## Non-Requirements (Nice-to-Have)

These are not blocking requirements but would be valuable:

- **Schema validation**: Checking policies against resource/action schemas
- **Policy simulation/testing**: "What if" evaluation without side effects
- **Partial evaluation**: Pre-computing parts of policies for performance
- **Delegation**: One principal delegating rights to another
- **Role hierarchies**: User → Role → Permission inheritance

---

## questions for Alternatives

When evaluating alternatives, please address:

1. How does PARC mapping work? Is it native or requires adaptation?
2. What is the Go integration story? Library, bindings, or FFI?
3. How is pattern matching for paths/URLs handled?
4. What is the performance profile for high-frequency evaluation?
5. Is AST introspection available for policy analysis?
6. How are multiple policy sources composed/merged?
7. What is the syntax learning curve for developers?
8. Is the project actively maintained with a clear roadmap?
9. What is the licensing model (especially for commercial use)?

---

## Appendix: Example Policies from kodus

### Filesystem Scoping

```cedar
// Allow reads within data/ subdirectory
permit (
    principal,
    action == Action::"filesystem:read",
    resource
) when {
    resource.path like "*/data/*"
};
```

### Network Domain Whitelisting

```cedar
// Allow fetch only to Hetzner Cloud API
permit(
    principal,
    action == Action::"invoke",
    resource
) when {
    resource.fullname == "fetch" &&
    context.arguments["0"].host == "api.hetzner.cloud"
};
```

### Process Execution Control

```cedar
// Allow only specific binaries
permit(
    principal,
    action == Action::"create",
    resource
) when {
    resource.type == "process" &&
    resource.command == "/usr/bin/python3"
};
```

---

_Document generated: 2026-02-05_
_kodus version: current development_
