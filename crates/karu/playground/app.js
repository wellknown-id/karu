// Karu Playground - Browser Application

import init, { karu_eval_js, karu_transpile_js, karu_check_js } from './pkg/karu.js';

// Example policies
const EXAMPLES = {
    basic: {
        policy: `# Basic role-based access
allow access if
    principal.role == "admin";

allow readonly if
    action == "read";`,
        input: {
            principal: { role: "admin", name: "alice" },
            action: "write",
            resource: "secrets"
        }
    },
    abac: {
        policy: `# Attribute-based access control
allow access if
    principal.department == "Engineering" and
    principal.level >= 5 and
    resource.classification == "internal";`,
        input: {
            principal: { name: "bob", department: "Engineering", level: 6 },
            action: "read",
            resource: { name: "design-doc", classification: "internal" }
        }
    },
    deny: {
        policy: `# Deny overrides allow
allow general;

deny blocked if
    principal.status == "suspended";`,
        input: {
            principal: { name: "eve", status: "suspended" },
            action: "login",
            resource: "system"
        }
    },
    pattern: {
        policy: `# Pattern matching in arrays
allow capability if
    { action: "write", resource: "/data/*" } in principal.permissions;`,
        input: {
            principal: {
                name: "charlie",
                permissions: [
                    { action: "read", resource: "*" },
                    { action: "write", resource: "/data/*" }
                ]
            },
            action: "write",
            resource: "/data/file.txt"
        }
    }
};

// DOM Elements
const policyEditor = document.getElementById('policy');
const inputEditor = document.getElementById('input');
const resultDiv = document.getElementById('result');
const policyStatus = document.getElementById('policy-status');
const examplesSelect = document.getElementById('examples');
const transpileBtn = document.getElementById('transpile-btn');
const modal = document.getElementById('modal');
const modalClose = document.getElementById('modal-close');
const cedarOutput = document.getElementById('cedar-output');

let wasmReady = false;
let debounceTimer = null;

// Initialize WASM
async function initWasm() {
    try {
        await init();
        wasmReady = true;
        resultDiv.innerHTML = '<span class="loading">Ready - edit policy or JSON to evaluate</span>';
        evaluate();
    } catch (e) {
        resultDiv.className = 'result error';
        resultDiv.textContent = `Failed to load WASM: ${e.message}`;
    }
}

// Evaluate policy
function evaluate() {
    if (!wasmReady) return;

    const policy = policyEditor.value;
    const input = inputEditor.value;

    // Check policy syntax first
    const checkResult = karu_check_js(policy);
    if (checkResult.error) {
        policyStatus.textContent = 'Error';
        policyStatus.className = 'status error';
        resultDiv.className = 'result error';
        resultDiv.textContent = checkResult.error;
        return;
    }

    policyStatus.textContent = `${checkResult.rules} rule${checkResult.rules !== 1 ? 's' : ''}`;
    policyStatus.className = 'status ok';

    // Evaluate
    const evalResult = karu_eval_js(policy, input);
    if (evalResult.error) {
        resultDiv.className = 'result error';
        resultDiv.textContent = evalResult.error;
        return;
    }

    resultDiv.className = `result ${evalResult.result.toLowerCase()}`;
    resultDiv.textContent = evalResult.result;
}

// Debounced evaluate
function debouncedEvaluate() {
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(evaluate, 300);
}

// Load example
function loadExample(name) {
    const example = EXAMPLES[name];
    if (!example) return;

    policyEditor.value = example.policy;
    inputEditor.value = JSON.stringify(example.input, null, 4);
    saveToStorage();
    evaluate();
}

// Transpile to Cedar
function transpile() {
    if (!wasmReady) return;

    const result = karu_transpile_js(policyEditor.value);
    if (result.error) {
        cedarOutput.textContent = `Error: ${result.error}`;
    } else {
        cedarOutput.textContent = result.cedar;
    }
    modal.classList.remove('hidden');
}

// Storage
function saveToStorage() {
    try {
        localStorage.setItem('karu-policy', policyEditor.value);
        localStorage.setItem('karu-input', inputEditor.value);
    } catch (e) {
        // Storage might be disabled
    }
}

function loadFromStorage() {
    try {
        const policy = localStorage.getItem('karu-policy');
        const input = localStorage.getItem('karu-input');
        if (policy) policyEditor.value = policy;
        if (input) inputEditor.value = input;
    } catch (e) {
        // Storage might be disabled
    }
}

// Event listeners
policyEditor.addEventListener('input', () => {
    saveToStorage();
    debouncedEvaluate();
});

inputEditor.addEventListener('input', () => {
    saveToStorage();
    debouncedEvaluate();
});

examplesSelect.addEventListener('change', (e) => {
    if (e.target.value) {
        loadExample(e.target.value);
        e.target.value = '';
    }
});

transpileBtn.addEventListener('click', transpile);

modalClose.addEventListener('click', () => {
    modal.classList.add('hidden');
});

modal.addEventListener('click', (e) => {
    if (e.target === modal) {
        modal.classList.add('hidden');
    }
});

document.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
        modal.classList.add('hidden');
    }
});

// Initialize
loadFromStorage();
initWasm();
