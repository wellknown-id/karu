// Node.js benchmark for Cedar vs Karu WASM comparison
// Run with: node bench_node.mjs

import { performance } from 'perf_hooks';
import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import { dirname, join } from 'path';

// Load Karu WASM manually
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const karuPkg = require('./pkg/karu.js');
const { karu_eval_js } = karuPkg;

// Cedar WASM (Node.js version - uses fs internally)
const cedar = await import('@cedar-policy/cedar-wasm/nodejs');

const WARMUP = 100;
const RUNS = 1000;

// Policies
const KARU_POLICY = 'allow access if role == "admin";';
const KARU_INPUT = JSON.stringify({ role: "admin" });

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

console.log("=== Karu vs Cedar WASM Benchmark (Node.js) ===\n");
console.log(`Cedar version: ${cedar.getCedarVersion()}`);
console.log(`Runs: ${RUNS}, Warmup: ${WARMUP}\n`);

const karuTime = bench("Karu eval", () => karu_eval_js(KARU_POLICY, KARU_INPUT));
const cedarTime = bench("Cedar eval", () => cedar.isAuthorized(CEDAR_REQUEST));

console.log("\n=== Results ===");
const ratio = cedarTime / karuTime;
if (karuTime < cedarTime) {
    console.log(`Winner: Karu (${ratio.toFixed(1)}x faster)`);
} else {
    console.log(`Winner: Cedar (${(1 / ratio).toFixed(1)}x faster)`);
}
