// Node.js benchmark for Cedar WASM
// Run with: node bench_cedar.mjs

import { performance } from 'perf_hooks';

// Cedar WASM (Node.js version - uses fs internally)
const cedar = await import('@cedar-policy/cedar-wasm/nodejs');

const WARMUP = 100;
const RUNS = 1000;

const CEDAR_POLICY = `permit(principal, action, resource) when { principal.role == "admin" };`;
const CEDAR_ENTITIES = '[]';
const CEDAR_REQUEST = {
    principal: { type: "User", id: "alice" },
    action: { type: "Action", id: "access" },
    resource: { type: "Resource", id: "res1" },
    context: {},
    schema: null,
    validateRequest: false,
    policies: { staticPolicies: CEDAR_POLICY, templates: {}, templateLinks: [] },
    entities: CEDAR_ENTITIES
};

function bench(name, fn) {
    // Warmup
    for (let i = 0; i < WARMUP; i++) fn();

    // Benchmark
    const start = performance.now();
    for (let i = 0; i < RUNS; i++) fn();
    const elapsed = performance.now() - start;

    const avgUs = (elapsed * 1000) / RUNS;
    console.log(`${name}: ${avgUs.toFixed(2)} μs/op`);
    return avgUs;
}

console.log("=== Cedar WASM Benchmark (Node.js) ===\n");
console.log(`Cedar version: ${cedar.getCedarVersion()}`);
console.log(`Runs: ${RUNS}, Warmup: ${WARMUP}\n`);

const cedarTime = bench("Cedar isAuthorized", () => cedar.isAuthorized(CEDAR_REQUEST));

console.log("\n=== For Comparison ===");
console.log("Karu native: ~52 ns (1000x faster than any WASM)");
console.log("Karu browser WASM: ~11-18 μs");
