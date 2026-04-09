// SPDX-License-Identifier: MIT

// Equivalent policies for Karu and Cedar
// These express the SAME authorization logic

export const SCENARIOS = {
    // Simple: single condition
    simple: {
        name: 'Simple (1 condition)',
        karu: 'allow access if role == "admin";',
        cedar: 'permit(principal, action, resource) when { principal.role == "admin" };',
        input: { role: "admin" },
        cedarRequest: (ctx) => ({
            principal: { type: "User", id: "alice", role: ctx.role },
            action: { type: "Action", id: "access" },
            resource: { type: "Resource", id: "res1" },
            context: {}
        })
    },

    // Multi-condition: 4 conditions ANDed
    multiCondition: {
        name: 'Multi-condition (4 ANDed)',
        karu: `allow access if 
      role == "admin" and 
      active == true and 
      level >= 5 and
      department == "engineering";`,
        cedar: `permit(principal, action, resource) when { 
      principal.role == "admin" && 
      principal.active == true && 
      principal.level >= 5 &&
      principal.department == "engineering"
    };`,
        input: { role: "admin", active: true, level: 10, department: "engineering" },
        cedarRequest: (ctx) => ({
            principal: { type: "User", id: "alice", ...ctx },
            action: { type: "Action", id: "access" },
            resource: { type: "Resource", id: "res1" },
            context: {}
        })
    },

    // Nested path: deep object access
    nestedPath: {
        name: 'Nested path (6 levels)',
        karu: 'allow access if a.b.c.d.e.f == true;',
        cedar: 'permit(principal, action, resource) when { context.a.b.c.d.e.f == true };',
        input: { a: { b: { c: { d: { e: { f: true } } } } } },
        cedarRequest: (ctx) => ({
            principal: { type: "User", id: "alice" },
            action: { type: "Action", id: "access" },
            resource: { type: "Resource", id: "res1" },
            context: ctx
        })
    },

    // Complex: 20 rules
    complex: {
        name: 'Complex (20 rules)',
        karu: Array.from({ length: 20 }, (_, i) =>
            `allow rule${i} if field${i} == "value${i}";`
        ).join('\n'),
        cedar: Array.from({ length: 20 }, (_, i) =>
            `permit(principal, action == Action::"rule${i}", resource) when { context.field${i} == "value${i}" };`
        ).join('\n'),
        input: Object.fromEntries(
            Array.from({ length: 20 }, (_, i) => [`field${i}`, `value${i}`])
        ),
        cedarRequest: (ctx) => ({
            principal: { type: "User", id: "alice" },
            action: { type: "Action", id: "rule0" },
            resource: { type: "Resource", id: "res1" },
            context: ctx
        })
    }
};

// Generate batch inputs
export function generateBatchInputs(count, scenario) {
    return Array.from({ length: count }, () => ({ ...scenario.input }));
}
