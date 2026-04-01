# Karu Roadmap

> **Last updated:** April 2026

This document outlines the development trajectory for Karu, from the current foundation to a production-ready policy engine.

---

## Current Status (v0.1.0)

### ✅ Complete

| Component           | Description                                                                    |
| ------------------- | ------------------------------------------------------------------------------ |
| **Parser & Lexer**  | Full Polar-inspired syntax with `allow`/`deny`, `if`, `and`, `or`, `not`, `in` |
| **Pattern Matcher** | Structural matching, partial object matching, array search                     |
| **Rule Engine**     | Policy evaluation with deny-overrides semantics                                |
| **CLI**             | `eval`, `transpile`, `check`, `import` commands                                |
| **Cedar Interop**   | Bidirectional transpilation (Karu ↔ Cedar)                                     |
| **WASM Target**     | `cdylib` build for browser/edge embedding                                      |
| **Strict Mode**     | Optional Cedar-compatible subset enforcement                                   |
| **Docs**            | ABAC, RBAC, ReBAC modeling guides; Cedar comparison                            |

---

## Phase 1: Developer Experience ✅

**Goal:** Make Karu easy to adopt and integrate.

- [x] **Error Messages** - Human-readable parse errors with line/column spans
- [x] **Playground** - Browser-based WASM playground for live policy testing
- [x] **Language Server** - Full LSP (see below)
- [x] **Crate Docs** - Comprehensive `rustdoc` with examples for all public APIs
- [x] **Formatter** - Canonical formatting with comment preservation

### Language Server Status

| Feature          | Status | Notes                                                               |
| ---------------- | ------ | ------------------------------------------------------------------- |
| Diagnostics      | ✅     | Parse errors with line/column                                       |
| Hover (keywords) | ✅     | `allow`, `deny`, `if`, `and`, `or`, `not`, `in`, `forall`, `exists` |
| Document Sync    | ✅     | Full sync on open/change/close                                      |
| Document Symbols | ✅     | List rule names in outline                                          |
| Completion       | ✅     | Keywords with snippets                                              |
| Go to Definition | ✅     | Jump to rule by name                                                |
| Semantic Tokens  | ✅     | Syntax highlighting via LSP                                         |
| Formatting       | ✅     | Canonical formatting via `format_source`                            |
| **Code Actions** | ❌     | Quick fixes, refactors                                              |

---

## Phase 2: Performance & Scale ✅

**Goal:** Production-ready performance for high-throughput evaluation.

- [x] **Benchmarks** - Comparative benchmarks vs Cedar
- [x] **Compiled Policies** - IndexedPolicy with pre-separated rules
- [x] **Batch Evaluation** - Evaluate multiple requests against one policy

---

## Phase 3: Advanced Matching ✅

**Goal:** Extend pattern matching capabilities for complex use cases.

- [x] **Path-in-Path** - DSL support for `principal.id in resource.adminIds`
- [x] **Wildcards** - Glob patterns for resource paths (e.g., `/data/*`)
- [x] **Negation Patterns** - Match objects that _don't_ contain a field
- [x] **Quantifiers** - `forall` and `exists` for collection predicates
- [x] **Type Constraints** - Type checking patterns (is_string, is_number, etc.)

---

## Phase 4: Ecosystem & Integration ✅

**Goal:** First-class integrations with authorization infrastructure.

- [x] **SDKs** - JavaScript bindings via WASM (simulate, diff, batch)

---

## Phase 5: Governance & Enterprise ✅

**Goal:** Features for multi-team policy management at scale.

- [x] **Policy Namespaces** - Scoped policies per tenant/service
- [x] **Version Control** - Policy versioning with rollback
- [x] **Diff & Merge** - Semantic diff for policy changes
- [x] **Simulation Mode** - "What-if" analysis without enforcement
- [x] **Admin API** - RESTful management for policy CRUD

---

## Phase 6: Distribution & Packaging

**Goal:** Make Karu available across every major platform and package manager.

- [ ] **npm Package** - WASM-based package for Node.js/Bun/Deno (`npm install karu`)
- [ ] **crates.io** - Publish `karu`, `karu-cli` crates
- [ ] **VS Code Marketplace** - Extension with bundled LSP
- [ ] **Open VSX Registry** - Extension for VS Code OSS / Code Server
- [ ] **Go Bindings** - CGo wrapper via C FFI (`bindings/go/`)
- [ ] **Swift Bindings** - SPM package via C FFI (`bindings/swift/`)
- [ ] **Android Libraries** - Shared libs for arm64-v8a, armeabi-v7a, x86_64
- [ ] **Nightly Releases** - Automated nightly builds of all artifacts

---

## Non-Goals

These are explicitly out of scope to keep Karu focused:

- **Full Datalog** - We're pattern-matching, not logic programming
- **Built-in Identity Provider** - Bring your own user/group data
- **Distributed Consensus** - Policies are local; sync is your concern

---

_Questions? Open an issue or start a discussion._

