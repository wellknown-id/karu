// Karu WASM benchmark functions
import init, { karu_eval_js, karu_check_js, karu_batch_js } from '../pkg/karu.js';

let initialized = false;
let initPromise = null;

// Pre-fetch WASM during page load (hides network latency)
const wasmUrl = new URL('../pkg/karu_bg.wasm', import.meta.url);
const prefetchPromise = fetch(wasmUrl).then(r => r.arrayBuffer());

export async function initKaru() {
    if (initialized) return 0;
    if (initPromise) return initPromise;

    initPromise = (async () => {
        const start = performance.now();
        // Use pre-fetched WASM for faster instantiation
        const wasmBytes = await prefetchPromise;
        await init(wasmBytes);
        initialized = true;
        return performance.now() - start;
    })();

    return initPromise;
}

export function karuEval(policy, input) {
    return karu_eval_js(policy, JSON.stringify(input));
}

export function karuCheck(policy) {
    return karu_check_js(policy);
}

export function karuBatch(policy, inputs) {
    return karu_batch_js(policy, JSON.stringify(inputs));
}

export function benchKaru(policy, input, runs = 100, warmup = 10) {
    const inputStr = JSON.stringify(input);

    // Warmup
    for (let i = 0; i < warmup; i++) karu_eval_js(policy, inputStr);

    // Benchmark
    const start = performance.now();
    for (let i = 0; i < runs; i++) karu_eval_js(policy, inputStr);
    return ((performance.now() - start) * 1000) / runs; // μs
}
