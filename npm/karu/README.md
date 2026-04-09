# karu

An embeddable policy engine focusing on structural pattern matching over arbitrary JSON data.

## Install

```bash
npm install @wellknown.id/karu
```

## Quick Start

```javascript
import init, { karu_eval_js, karu_check_js, karu_simulate_js } from 'karu';

// Initialize the WASM module
await init();

// Check a policy for syntax errors
const check = karu_check_js('allow access if role == "admin";');
console.log(check); // { ok: true, rules: 1 }

// Evaluate a policy against JSON input
const result = karu_eval_js(
  'allow access if role == "admin";',
  '{"role": "admin"}'
);
console.log(result); // { result: "ALLOW" }

// Simulate with detailed trace
const sim = karu_simulate_js(
  'allow access if role == "admin";',
  '{"role": "admin"}'
);
console.log(sim); // { decision: "ALLOW", matched_rules: ["access"] }
```

## API

| Function | Description |
|---|---|
| `karu_eval_js(policy, json)` | Evaluate a Karu policy against JSON input |
| `karu_check_js(policy)` | Check a policy for syntax errors |
| `karu_simulate_js(policy, json)` | Simulate with detailed match trace |
| `karu_batch_js(policy, jsonArray)` | Batch evaluate multiple inputs |
| `karu_diff_js(oldPolicy, newPolicy)` | Semantic diff between policies |
| `karu_transpile_js(policy)` | Transpile Karu → Cedar |
| `karu_eval_cedar_js(cedar, json)` | Evaluate a Cedar policy via Karu |
| `karu_diagnostics_js(source)` | Parse diagnostics for editor integration |
| `karu_hover_js(word)` | Keyword hover documentation |
| `karu_completions_js()` | Keyword completions for editors |
| `karu_semantic_tokens_js(source)` | Semantic tokens for syntax highlighting |

## Cedar Interop

Karu includes bidirectional Cedar transpilation. Evaluate Cedar policies directly:

```javascript
const result = karu_eval_cedar_js(
  'permit(principal, action, resource) when { principal.role == "admin" };',
  '{"principal": {"role": "admin"}, "action": "read", "resource": "doc"}'
);
console.log(result); // { decision: "ALLOW", matched_rules: [...] }
```

## Learn More

- [Documentation](https://github.com/wellknown-id/karu)
- [Playground](https://wellknown-id.github.io/karu/)

## License

MIT
