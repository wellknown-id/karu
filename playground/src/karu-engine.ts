// SPDX-License-Identifier: MIT

/**
 * WASM bridge for the Karu policy engine.
 * Loads karu_bg.wasm and exposes evaluate/check/simulate + LSP features.
 */

interface KaruWasm {
  karu_eval_js(policy: string, input: string): { result?: string; error?: string };
  karu_check_js(policy: string): { ok?: boolean; rules?: number; error?: string };
  karu_simulate_js(policy: string, input: string): {
    decision?: string;
    matched_rules?: string[];
    error?: string;
  };
  karu_eval_cedar_js?(cedar_policy: string, input: string): {
    decision?: string;
    matched_rules?: string[];
    error?: string;
  };
  // LSP-core exports
  karu_diagnostics_js(source: string): string;
  karu_hover_js(word: string): { docs: string } | null;
  karu_completions_js(): string;
  karu_semantic_tokens_js(source: string): string;
  karu_run_tests_js(source: string): string | null;
  karu_code_actions_js(source: string): string;
}

let wasmModule: KaruWasm | null = null;
let loadError: string | null = null;
let loadPromise: Promise<void> | null = null;

async function loadWasm(): Promise<void> {
  try {
    // Load from the public/wasm/ directory, respecting Vite base path
    const base = import.meta.env.BASE_URL;
    const wasmUrl = new URL(`${base}wasm/karu_bg.wasm`, window.location.href);
    const initUrl = new URL(`${base}wasm/karu.js`, window.location.href);

    const initModule = await import(/* @vite-ignore */ initUrl.href);
    await initModule.default(wasmUrl.href);

    wasmModule = initModule as unknown as KaruWasm;
  } catch (e) {
    loadError = `WASM not available. Build with:\nwasm-pack build crates/karu --target web --no-default-features --features wasm,cedar --out-dir ../../playground/public/wasm`;
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

export function evaluateCedar(cedarPolicy: string, input: string): EvalResult {
  if (!wasmModule) {
    return { decision: 'DENY', matchedRules: [], error: loadError || 'Engine not loaded' };
  }
  if (!wasmModule.karu_eval_cedar_js) {
    return { decision: 'DENY', matchedRules: [], error: 'Cedar support not available in this WASM build' };
  }
  try {
    const sim = wasmModule.karu_eval_cedar_js(cedarPolicy, input);
    if (sim.error) {
      return { decision: 'DENY', matchedRules: [], error: sim.error };
    }
    return {
      decision: (sim.decision as 'ALLOW' | 'DENY') || 'DENY',
      matchedRules: sim.matched_rules || [],
    };
  } catch (e) {
    return { decision: 'DENY', matchedRules: [], error: String(e) };
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

// ============================================================================
// LSP-core functions
// ============================================================================

export interface LspDiagnostic {
  line: number;    // 0-indexed
  col: number;     // 0-indexed
  end_col: number; // 0-indexed
  severity: 'error' | 'warning' | 'info' | 'hint';
  message: string;
  code?: string;
}

export interface LspCompletion {
  label: string;
  detail: string;
  insert_text?: string;
  kind: string;
}

export interface LspSemanticToken {
  line: number;
  start: number;
  length: number;
  token_type: string;
}

export interface LspTestResult {
  name: string;
  line: number;
  passed: boolean;
  message: string;
}

export interface LspRuleCoverage {
  name: string;
  line: number;
  has_positive: boolean;
  has_negative: boolean;
  status: string;
}

export interface LspInlineTestResults {
  tests: LspTestResult[];
  coverage: LspRuleCoverage[];
}

/** Get parse diagnostics for a Karu source file. */
export function getDiagnostics(source: string): LspDiagnostic[] {
  if (!wasmModule) return [];
  try {
    const json = wasmModule.karu_diagnostics_js(source);
    return JSON.parse(json) as LspDiagnostic[];
  } catch {
    return [];
  }
}

/** Get hover documentation for a keyword. */
export function getHover(word: string): string | null {
  if (!wasmModule) return null;
  try {
    const result = wasmModule.karu_hover_js(word);
    return result?.docs ?? null;
  } catch {
    return null;
  }
}

/** Get keyword completions. */
export function getCompletions(): LspCompletion[] {
  if (!wasmModule) return [];
  try {
    const json = wasmModule.karu_completions_js();
    return JSON.parse(json) as LspCompletion[];
  } catch {
    return [];
  }
}

/** Get semantic tokens for syntax highlighting. */
export function getSemanticTokens(source: string): LspSemanticToken[] {
  if (!wasmModule) return [];
  try {
    const json = wasmModule.karu_semantic_tokens_js(source);
    return JSON.parse(json) as LspSemanticToken[];
  } catch {
    return [];
  }
}

/** Run inline tests in a Karu source file. */
export function runInlineTests(source: string): LspInlineTestResults | null {
  if (!wasmModule) return null;
  try {
    const json = wasmModule.karu_run_tests_js(source);
    if (!json) return null;
    return JSON.parse(json) as LspInlineTestResults;
  } catch {
    return null;
  }
}

export interface LspTextEdit {
  line: number;
  col: number;
  end_col: number;
  new_text: string;
}

export interface LspCodeAction {
  title: string;
  kind: string;
  diagnostic_code?: string;
  edits: LspTextEdit[];
}

/** Get code actions (quick-fixes) for a Karu source file. */
export function getCodeActions(source: string): LspCodeAction[] {
  if (!wasmModule) return [];
  try {
    const json = wasmModule.karu_code_actions_js(source);
    return JSON.parse(json) as LspCodeAction[];
  } catch {
    return [];
  }
}

