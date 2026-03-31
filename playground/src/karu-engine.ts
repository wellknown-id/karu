/**
 * WASM bridge for the Karu policy engine.
 * Loads karu_bg.wasm and exposes evaluate/check/simulate.
 */

interface KaruWasm {
  karu_eval_js(policy: string, input: string): { result?: string; error?: string };
  karu_check_js(policy: string): { ok?: boolean; rules?: number; error?: string };
  karu_simulate_js(policy: string, input: string): {
    decision?: string;
    matched_rules?: string[];
    error?: string;
  };
}

let wasmModule: KaruWasm | null = null;
let loadError: string | null = null;
let loadPromise: Promise<void> | null = null;

async function loadWasm(): Promise<void> {
  try {
    // Try to load from the public/wasm/ directory
    const wasmUrl = new URL('/wasm/karu_bg.wasm', window.location.href);
    const initUrl = new URL('/wasm/karu.js', window.location.href);

    const initModule = await import(/* @vite-ignore */ initUrl.href);
    await initModule.default(wasmUrl.href);

    wasmModule = initModule as unknown as KaruWasm;
  } catch (e) {
    loadError = `WASM not available. Build with:\nwasm-pack build crates/karu --target web --no-default-features --features wasm --out-dir ../../playground/public/wasm`;
    console.warn('Karu WASM load failed:', e);
  }
}

export async function initEngine(): Promise<void> {
  if (!loadPromise) {
    loadPromise = loadWasm();
  }
  return loadPromise;
}

export function isReady(): boolean {
  return wasmModule !== null;
}

export function getLoadError(): string | null {
  return loadError;
}

export interface EvalResult {
  decision: 'ALLOW' | 'DENY';
  matchedRules: string[];
  error?: string;
}

export function evaluate(policy: string, input: string): EvalResult {
  if (!wasmModule) {
    return { decision: 'DENY', matchedRules: [], error: loadError || 'Engine not loaded' };
  }

  // First try simulate for richer output
  try {
    const sim = wasmModule.karu_simulate_js(policy, input);
    if (sim.error) {
      return { decision: 'DENY', matchedRules: [], error: sim.error };
    }
    return {
      decision: (sim.decision as 'ALLOW' | 'DENY') || 'DENY',
      matchedRules: sim.matched_rules || [],
    };
  } catch {
    // Fallback to simple eval
    try {
      const res = wasmModule.karu_eval_js(policy, input);
      if (res.error) {
        return { decision: 'DENY', matchedRules: [], error: res.error };
      }
      return {
        decision: (res.result as 'ALLOW' | 'DENY') || 'DENY',
        matchedRules: [],
      };
    } catch (e) {
      return { decision: 'DENY', matchedRules: [], error: String(e) };
    }
  }
}

export function check(policy: string): { ok: boolean; rules: number; error?: string } {
  if (!wasmModule) {
    return { ok: false, rules: 0, error: loadError || 'Engine not loaded' };
  }
  try {
    const res = wasmModule.karu_check_js(policy);
    if (res.error) return { ok: false, rules: 0, error: res.error };
    return { ok: res.ok ?? true, rules: res.rules ?? 0 };
  } catch (e) {
    return { ok: false, rules: 0, error: String(e) };
  }
}
