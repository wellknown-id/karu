// Main benchmark orchestrator
import { SCENARIOS, generateBatchInputs } from './policies.js';
import { initKaru, benchKaru, karuBatch } from './karu-bench.js';
import { initCedar, getCedarVersion, benchCedar } from './cedar-bench.js';

const WARMUP = 10;
const RUNS = 100;
const BATCH_SIZE = 1000;

const results = {};
let log;

function logMsg(msg) {
    log.innerHTML += `${new Date().toISOString().slice(11, 19)} ${msg}\n`;
    log.scrollTop = log.scrollHeight;
    console.log(msg);
}

function updateUI(scenario, karuTime, cedarTime) {
    const row = document.getElementById(`row-${scenario}`);
    if (row) {
        row.querySelector('.karu-time').textContent = karuTime.toFixed(1);
        row.querySelector('.cedar-time').textContent = cedarTime.toFixed(1);
        const diff = ((cedarTime - karuTime) / cedarTime * 100).toFixed(0);
        const diffCell = row.querySelector('.diff');
        diffCell.textContent = `${diff}%`;
        diffCell.className = `diff ${karuTime < cedarTime ? 'faster' : 'slower'}`;
    }
}

async function runBenchmarks() {
    const btn = document.getElementById('runBtn');
    btn.disabled = true;
    btn.textContent = 'Running...';
    log = document.getElementById('status');
    log.innerHTML = '';

    try {
        // Initialize engines
        logMsg('Initializing Karu WASM...');
        const karuInitTime = await initKaru();
        logMsg(`✓ Karu loaded in ${karuInitTime.toFixed(0)}ms (221 KB)`);
        document.getElementById('karuInit').textContent = karuInitTime.toFixed(0);

        logMsg('Initializing Cedar WASM...');
        const cedarInitTime = await initCedar();
        logMsg(`✓ Cedar ${getCedarVersion()} loaded in ${cedarInitTime.toFixed(0)}ms (4.3 MB)`);
        document.getElementById('cedarInit').textContent = cedarInitTime.toFixed(0);

        // Run each scenario
        for (const [key, scenario] of Object.entries(SCENARIOS)) {
            logMsg(`\nBenchmarking: ${scenario.name}...`);

            // Karu
            const karuTime = benchKaru(scenario.karu, scenario.input, RUNS, WARMUP);
            logMsg(`  Karu: ${karuTime.toFixed(1)} μs`);

            // Cedar
            const cedarTime = benchCedar(scenario.cedar, scenario.cedarRequest, scenario.input, RUNS, WARMUP);
            logMsg(`  Cedar: ${cedarTime.toFixed(1)} μs`);

            results[key] = { karu: karuTime, cedar: cedarTime };
            updateUI(key, karuTime, cedarTime);
        }

        // Batch benchmark
        logMsg(`\nBenchmarking: Batch (${BATCH_SIZE} evals)...`);
        const batchInputs = generateBatchInputs(BATCH_SIZE, SCENARIOS.simple);

        const batchStart = performance.now();
        karuBatch(SCENARIOS.simple.karu, batchInputs);
        const karuBatchTime = ((performance.now() - batchStart) * 1000) / BATCH_SIZE;
        logMsg(`  Karu batch: ${karuBatchTime.toFixed(2)} μs/op`);
        document.getElementById('karuBatch').textContent = karuBatchTime.toFixed(2);

        // Cedar batch (sequential)
        const cedarBatchStart = performance.now();
        for (const input of batchInputs) {
            benchCedar(SCENARIOS.simple.cedar, SCENARIOS.simple.cedarRequest, input, 1, 0);
        }
        const cedarBatchTime = ((performance.now() - cedarBatchStart) * 1000) / BATCH_SIZE;
        logMsg(`  Cedar batch: ${cedarBatchTime.toFixed(2)} μs/op`);
        document.getElementById('cedarBatch').textContent = cedarBatchTime.toFixed(2);

        // Summary
        logMsg('\n=== SUMMARY ===');
        for (const [key, r] of Object.entries(results)) {
            const ratio = (r.cedar / r.karu).toFixed(1);
            logMsg(`${SCENARIOS[key].name}: Karu ${ratio}x faster`);
        }
        logMsg('\n✓ All benchmarks complete!');

    } catch (e) {
        logMsg(`✗ Error: ${e.message}`);
        console.error(e);
    }

    btn.disabled = false;
    btn.textContent = 'Run Benchmarks';
}

// Expose to window
window.runBenchmarks = runBenchmarks;
