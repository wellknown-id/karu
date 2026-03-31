/**
 * Built-in example policies for the playground.
 */

export interface Example {
  name: string;
  policy: string;
  input: string;
  /** Language: 'karu' (default) or 'cedar' */
  language?: 'karu' | 'cedar';
}

export const examples: Example[] = [
  // ── Karu Examples ─────────────────────────────────
  {
    name: 'Simple RBAC',
    policy: `// Role-based access control
allow read if
    action == "read" and
    "viewer" in principal.roles;

allow write if
    action == "write" and
    "editor" in principal.roles;

allow admin if
    "admin" in principal.roles;

deny delete if
    action == "delete" and
    not "admin" in principal.roles;`,
    input: JSON.stringify({
      principal: { id: 'alice', roles: ['viewer', 'editor'] },
      action: 'read',
      resource: { type: 'Document', id: 'doc-1' },
    }, null, 2),
  },
  {
    name: 'Pattern Matching',
    policy: `// Structural pattern matching over arrays
allow access if
    { name: "lhs", value: 10 } in resource.args;`,
    input: JSON.stringify({
      principal: { id: 'service' },
      action: 'invoke',
      resource: {
        id: 'fn-1',
        args: [
          { name: 'junk', value: 0 },
          { name: 'lhs', value: 10, type: 'int' },
        ],
      },
    }, null, 2),
  },
  {
    name: 'Quantifiers',
    policy: `// All items must be approved
allow batch if
    forall item in resource.items:
        item.approved == true;

// At least one tag must be "priority"
deny flagged if
    exists tag in resource.tags:
        tag == "blocked";`,
    input: JSON.stringify({
      principal: { id: 'system' },
      action: 'process',
      resource: {
        items: [
          { id: 1, approved: true },
          { id: 2, approved: true },
        ],
        tags: ['normal', 'reviewed'],
      },
    }, null, 2),
  },
  {
    name: 'ABAC with Context',
    policy: `// Attribute-based: department + clearance
allow classified if
    principal.department == resource.department and
    principal.clearance >= resource.requiredClearance;

// Time-based restriction
allow timed if
    context.hour >= 9 and
    context.hour < 17;`,
    input: JSON.stringify({
      principal: { id: 'bob', department: 'Engineering', clearance: 3 },
      action: 'read',
      resource: { department: 'Engineering', requiredClearance: 2 },
      context: { hour: 14 },
    }, null, 2),
  },
  {
    name: 'Deny Overrides',
    policy: `// General access
allow general if
    principal.authenticated == true;

// But deny private resources to non-owners
deny private if
    resource.private == true and
    not principal.id == resource.ownerId;`,
    input: JSON.stringify({
      principal: { id: 'charlie', authenticated: true },
      action: 'view',
      resource: { id: 'secret-doc', private: true, ownerId: 'alice' },
    }, null, 2),
  },

  // ── Cedar Examples ────────────────────────────────
  {
    name: 'Cedar: Basic Permit',
    language: 'cedar',
    policy: `permit(
  principal == User::"alice",
  action == Action::"view",
  resource == Photo::"vacation.jpg"
);`,
    input: JSON.stringify({
      principal: 'alice',
      action: 'view',
      resource: 'vacation.jpg',
    }, null, 2),
  },
  {
    name: 'Cedar: ABAC',
    language: 'cedar',
    policy: `permit(
  principal,
  action == Action::"view",
  resource
)
when {
  principal.department == "Engineering" &&
  principal.clearance >= 3
};`,
    input: JSON.stringify({
      principal: { id: 'bob', department: 'Engineering', clearance: 4 },
      action: 'view',
      resource: { type: 'Document', id: 'secret-plans' },
    }, null, 2),
  },
  {
    name: 'Cedar: Forbid',
    language: 'cedar',
    policy: `permit(
  principal,
  action,
  resource
)
when { principal.authenticated == true };

forbid(
  principal,
  action,
  resource
)
when { resource.private == true }
unless { principal == resource.owner };`,
    input: JSON.stringify({
      principal: { id: 'eve', authenticated: true },
      action: 'read',
      resource: { id: 'secret', private: true, owner: 'alice' },
    }, null, 2),
  },
];
