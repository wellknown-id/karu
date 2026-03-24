// Cedar WASM benchmark functions
import * as cedar from '@cedar-policy/cedar-wasm';

let initialized = false;

export async function initCedar() {
    if (!initialized) {
        const start = performance.now();
        // Cedar initializes automatically on import with Vite
        initialized = true;
        // Warm up with a simple call
        cedar.getCedarVersion();
        return performance.now() - start;
    }
    return 0;
}

export function getCedarVersion() {
    return cedar.getCedarVersion();
}

export function cedarEval(policy, request, entities = '[]') {
    return cedar.isAuthorized({
        principal: request.principal,
        action: request.action,
        resource: request.resource,
        context: request.context || {},
        schema: null,
        validateRequest: false,
        policies: { staticPolicies: policy, templates: {}, templateLinks: [] },
        entities: entities
    });
}

export function benchCedar(policy, requestFn, input, runs = 100, warmup = 10) {
    const request = requestFn(input);

    // Warmup
    for (let i = 0; i < warmup; i++) {
        cedar.isAuthorized({
            ...request,
            schema: null,
            validateRequest: false,
            policies: { staticPolicies: policy, templates: {}, templateLinks: [] },
            entities: '[]'
        });
    }

    // Benchmark
    const start = performance.now();
    for (let i = 0; i < runs; i++) {
        cedar.isAuthorized({
            ...request,
            schema: null,
            validateRequest: false,
            policies: { staticPolicies: policy, templates: {}, templateLinks: [] },
            entities: '[]'
        });
    }
    return ((performance.now() - start) * 1000) / runs; // μs
}
