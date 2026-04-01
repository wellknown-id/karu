// SPDX-License-Identifier: MIT

/**
 * Main Karu Playground application shell.
 * Split-pane layout: policy editor | JSON input + results.
 */
import { LitElement, html, css, nothing } from 'lit';
import { customElement, state } from 'lit/decorators.js';
import './karu-editor';
import { initEngine, isReady, getLoadError, evaluate, evaluateCedar, runInlineTests, type EvalResult, type LspInlineTestResults } from './karu-engine';
import { examples } from './examples';

const REPO_URL = 'https://github.com/wellknown-id/karu';

@customElement('karu-playground')
export class KaruPlayground extends LitElement {
  static styles = css`
    :host {
      display: flex;
      flex-direction: column;
      height: 100vh;
      height: 100dvh;
      background: var(--bg-base);
      color: var(--text-primary);
      font-family: 'Inter', system-ui, sans-serif;
    }

    /* ── Header ─────────────────────────────────── */
    header {
      display: flex;
      align-items: center;
      gap: 12px;
      padding: 8px 16px;
      background: var(--bg-surface);
      border-bottom: 1px solid var(--border);
      flex-shrink: 0;
      z-index: 10;
    }
    .logo-link {
      display: flex;
      align-items: center;
      gap: 8px;
      text-decoration: none;
      transition: opacity 0.15s;
    }
    .logo-link:hover { opacity: 0.85; }
    .logo-link img {
      width: 24px;
      height: 24px;
    }
    .logo {
      font-weight: 700;
      font-size: 18px;
      letter-spacing: -0.02em;
      background: linear-gradient(135deg, var(--accent), var(--accent-light));
      -webkit-background-clip: text;
      -webkit-text-fill-color: transparent;
      background-clip: text;
    }
    .logo span {
      font-weight: 400;
      opacity: 0.5;
      -webkit-text-fill-color: var(--text-secondary);
    }
    .spacer { flex: 1; }

    select {
      background: var(--bg-raised);
      color: var(--text-primary);
      border: 1px solid var(--border);
      border-radius: 6px;
      padding: 6px 10px;
      font-size: 13px;
      font-family: inherit;
      cursor: pointer;
      outline: none;
      transition: border-color 0.15s;
    }
    select:hover { border-color: var(--accent); }
    select:focus { border-color: var(--accent); box-shadow: 0 0 0 2px rgba(124, 58, 237, 0.2); }

    button {
      background: var(--accent);
      color: white;
      border: none;
      border-radius: 6px;
      padding: 7px 18px;
      font-size: 13px;
      font-weight: 600;
      font-family: inherit;
      cursor: pointer;
      transition: all 0.15s;
      letter-spacing: 0.01em;
    }
    button:hover { background: var(--accent-light); transform: translateY(-1px); }
    button:active { transform: translateY(0); }
    button.secondary {
      background: var(--bg-raised);
      color: var(--text-primary);
      border: 1px solid var(--border);
    }
    button.secondary:hover { border-color: var(--accent); }

    /* ── Split panes ────────────────────────────── */
    .panes {
      display: flex;
      flex: 1;
      min-height: 0;
    }
    @media (orientation: portrait), (max-width: 700px) {
      .panes { flex-direction: column; }
      .divider { width: 100% !important; height: 6px !important; cursor: row-resize !important; }
      .divider::after { width: 40px !important; height: 3px !important; }
    }

    .pane {
      display: flex;
      flex-direction: column;
      min-width: 0;
      min-height: 0;
      flex: 1;
    }
    .pane-header {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 6px 12px;
      background: var(--bg-surface);
      border-bottom: 1px solid var(--border);
      font-size: 11px;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.08em;
      color: var(--text-tertiary);
      flex-shrink: 0;
    }
    .pane-body {
      flex: 1;
      min-height: 0;
      overflow: hidden;
    }

    .divider {
      width: 6px;
      background: var(--bg-surface);
      border-left: 1px solid var(--border);
      border-right: 1px solid var(--border);
      cursor: col-resize;
      flex-shrink: 0;
      display: flex;
      align-items: center;
      justify-content: center;
      transition: background 0.15s;
    }
    .divider:hover { background: var(--bg-raised); }
    .divider::after {
      content: '';
      width: 3px;
      height: 40px;
      border-radius: 3px;
      background: rgba(255,255,255,0.08);
    }

    /* ── Right pane ─────────────────────────────── */
    .right-pane {
      display: flex;
      flex-direction: column;
    }
    .input-area {
      flex: 1;
      min-height: 0;
      display: flex;
      flex-direction: column;
    }
    .input-area .pane-body {
      flex: 1;
      min-height: 0;
      overflow: hidden;
    }

    /* ── Result strip ──────────────────────────── */
    .result-strip {
      flex-shrink: 0;
      display: flex;
      align-items: center;
      gap: 12px;
      padding: 10px 16px;
      background: var(--bg-surface);
      border-top: 1px solid var(--border);
      min-height: 44px;
      transition: background 0.2s;
    }
    .result-strip.has-result { background: var(--bg-raised); }
    .result-badge {
      font-size: 14px;
      font-weight: 800;
      letter-spacing: 0.06em;
      padding: 4px 14px;
      border-radius: 6px;
      animation: result-pop 0.3s cubic-bezier(0.34, 1.56, 0.64, 1);
    }
    @keyframes result-pop {
      0% { transform: scale(0.8); opacity: 0; }
      100% { transform: scale(1); opacity: 1; }
    }
    .result-badge.allow {
      background: rgba(52, 211, 153, 0.15);
      color: #34d399;
    }
    .result-badge.deny {
      background: rgba(248, 113, 113, 0.15);
      color: #f87171;
    }
    .matched-rules {
      font-size: 12px;
      color: var(--text-secondary);
    }
    .matched-rules strong { color: var(--text-primary); }
    .result-empty {
      color: var(--text-tertiary);
      font-size: 12px;
    }
    .result-error {
      color: #f87171;
      font-family: 'JetBrains Mono', monospace;
      font-size: 12px;
      white-space: pre-wrap;
      word-break: break-word;
      overflow: auto;
      max-height: 80px;
    }

    /* ── Test results panel ────────────────────── */
    .test-panel {
      flex-shrink: 0;
      background: var(--bg-surface);
      border-top: 1px solid var(--border);
      max-height: 180px;
      overflow-y: auto;
    }
    .test-panel-header {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 6px 12px;
      font-size: 11px;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.08em;
      color: var(--text-tertiary);
      border-bottom: 1px solid var(--border);
    }
    .test-summary {
      font-size: 11px;
      font-weight: 400;
      text-transform: none;
      letter-spacing: 0;
    }
    .test-summary.all-pass { color: #34d399; }
    .test-summary.has-fail { color: #f87171; }
    .test-item {
      display: flex;
      align-items: center;
      gap: 8px;
      padding: 4px 12px;
      font-size: 12px;
      font-family: 'JetBrains Mono', monospace;
    }
    .test-icon {
      font-size: 14px;
      flex-shrink: 0;
    }
    .test-icon.pass { color: #34d399; }
    .test-icon.fail { color: #f87171; }
    .test-name {
      color: var(--text-primary);
      font-weight: 500;
    }
    .test-message {
      color: var(--text-tertiary);
      font-size: 11px;
    }
    .lang-badge {
      font-size: 10px;
      font-weight: 600;
      letter-spacing: 0.06em;
      padding: 2px 8px;
      border-radius: 4px;
      background: rgba(255, 255, 255, 0.06);
      color: var(--text-tertiary);
      text-transform: uppercase;
    }

    /* ── Engine status ──────────────────────────── */
    .status {
      font-size: 11px;
      display: flex;
      align-items: center;
      gap: 6px;
    }
    .status-dot {
      width: 6px;
      height: 6px;
      border-radius: 50%;
      background: var(--text-tertiary);
    }
    .status-dot.ready { background: #34d399; }
    .status-dot.error { background: #f87171; }

    /* ── Footer ─────────────────────────────────── */
    footer {
      flex-shrink: 0;
      padding: 6px 16px;
      background: var(--bg-surface);
      border-top: 1px solid var(--border);
      font-size: 10px;
      color: var(--text-tertiary);
      text-align: center;
      letter-spacing: 0.02em;
    }
    footer a {
      color: var(--text-secondary);
      text-decoration: none;
    }
    footer a:hover { color: var(--accent-light); text-decoration: underline; }
  `;

  @state() private policy = examples[0].policy;
  @state() private input = examples[0].input;
  @state() private result: EvalResult | null = null;
  @state() private engineReady = false;
  @state() private engineError: string | null = null;
  @state() private selectedExample = 0;
  @state() private testResults: LspInlineTestResults | null = null;
  private _testDebounce: ReturnType<typeof setTimeout> | null = null;

  private get currentLanguage(): 'karu' | 'cedar' {
    return examples[this.selectedExample]?.language || 'karu';
  }

  async connectedCallback() {
    super.connectedCallback();
    await initEngine();
    this.engineReady = isReady();
    this.engineError = getLoadError();
    // Auto-run tests for the initial example
    this.autoRunTests();
  }

  /** Debounced: auto-run inline tests if the policy contains any. */
  private autoRunTests() {
    if (this._testDebounce) clearTimeout(this._testDebounce);
    this._testDebounce = setTimeout(() => {
      if (this.currentLanguage === 'karu') {
        this.testResults = runInlineTests(this.policy);
      }
    }, 500);
  }

  private handleRun() {
    if (this.currentLanguage === 'cedar') {
      this.result = evaluateCedar(this.policy, this.input);
    } else {
      this.result = evaluate(this.policy, this.input);
    }
    // Also run inline tests immediately (no debounce on explicit Run)
    this.testResults = runInlineTests(this.policy);
  }

  private handlePolicyChange(e: CustomEvent<{ value: string }>) {
    this.policy = e.detail.value;
    this.autoRunTests();
  }

  private handleInputChange(e: CustomEvent<{ value: string }>) {
    this.input = e.detail.value;
  }

  private handleExampleChange(e: Event) {
    const idx = parseInt((e.target as HTMLSelectElement).value);
    this.selectedExample = idx;
    this.policy = examples[idx].policy;
    this.input = examples[idx].input;
    this.result = null;
    this.testResults = null;
  }

  private handleKeyDown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      this.handleRun();
    }
  }

  private renderResultStrip() {
    if (!this.result) {
      return html`<span class="result-empty">Press <strong>Run</strong> or <strong>Ctrl+Enter</strong></span>`;
    }
    if (this.result.error) {
      return html`<span class="result-error">${this.result.error}</span>`;
    }
    const cls = this.result.decision.toLowerCase();
    return html`
      <span class="result-badge ${cls}" key=${Date.now()}>${this.result.decision}</span>
      ${this.result.matchedRules.length > 0 ? html`
        <span class="matched-rules">
          <strong>Matched:</strong> ${this.result.matchedRules.join(', ')}
        </span>
      ` : nothing}
    `;
  }

  private renderTestPanel() {
    if (!this.testResults) return nothing;
    const { tests } = this.testResults;
    const allPass = tests.every(t => t.passed);
    const passCount = tests.filter(t => t.passed).length;
    const failCount = tests.length - passCount;

    return html`
      <div class="test-panel">
        <div class="test-panel-header">
          Tests
          <span class="test-summary ${allPass ? 'all-pass' : 'has-fail'}">
            ${allPass ? `✓ All ${tests.length} passed` : `${passCount} passed, ${failCount} failed`}
          </span>
        </div>
        ${tests.map(t => html`
          <div class="test-item">
            <span class="test-icon ${t.passed ? 'pass' : 'fail'}">${t.passed ? '✓' : '✗'}</span>
            <span class="test-name">${t.name}</span>
            ${t.message ? html`<span class="test-message">${t.message}</span>` : nothing}
          </div>
        `)}
      </div>
    `;
  }

  render() {
    const lang = this.currentLanguage;
    return html`
      <header @keydown=${this.handleKeyDown}>
        <a class="logo-link" href=${REPO_URL} target="_blank" rel="noopener" title="View on GitHub">
          <img src="${import.meta.env.BASE_URL}karu-icon.svg" alt="Karu" />
          <div class="logo">karu <span>playground</span></div>
        </a>
        <div class="spacer"></div>
        <select @change=${this.handleExampleChange} .value=${String(this.selectedExample)}>
          ${examples.map((ex, i) => html`<option value=${i}>${ex.name}</option>`)}
        </select>
        <button @click=${this.handleRun} title="Ctrl+Enter">Run</button>
        <div class="status">
          <div class="status-dot ${this.engineReady ? 'ready' : this.engineError ? 'error' : ''}"></div>
          ${this.engineReady ? 'Ready' : this.engineError ? 'Offline' : 'Loading…'}
        </div>
      </header>

      <div class="panes" @keydown=${this.handleKeyDown}>
        <!-- Left: Policy Editor -->
        <div class="pane">
          <div class="pane-header">
            Policy
            <span class="lang-badge">${lang}</span>
          </div>
          <div class="pane-body">
            <karu-editor
              language="karu"
              .value=${this.policy}
              .testResults=${this.testResults?.tests ?? null}
              .coverage=${this.testResults?.coverage ?? null}
              @change=${this.handlePolicyChange}
            ></karu-editor>
          </div>
        </div>

        <div class="divider"></div>

        <!-- Right: Input JSON + Result Strip -->
        <div class="pane right-pane">
          <div class="input-area">
            <div class="pane-header">Input JSON</div>
            <div class="pane-body">
              <karu-editor
                language="json"
                .value=${this.input}
                @change=${this.handleInputChange}
              ></karu-editor>
            </div>
          </div>
          <div class="result-strip ${this.result ? 'has-result' : ''}">
            ${this.renderResultStrip()}
          </div>
          ${this.renderTestPanel()}
        </div>
      </div>

      <footer>
        © ${new Date().getFullYear()} <a href="https://wellknown.id" target="_blank" rel="noopener">wellknown.id</a> ltd.
        wellknown.id is a registered trademark. All rights reserved.
        · <a href=${REPO_URL} target="_blank" rel="noopener">GitHub</a>
      </footer>
    `;
  }
}
