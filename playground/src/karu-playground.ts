/**
 * Main Karu Playground application shell.
 * Split-pane layout: policy editor | JSON input + results.
 */
import { LitElement, html, css, nothing } from 'lit';
import { customElement, state } from 'lit/decorators.js';
import './karu-editor';
import { initEngine, isReady, getLoadError, evaluate, type EvalResult } from './karu-engine';
import { examples } from './examples';

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

    /* ── Right pane tabs ────────────────────────── */
    .right-pane {
      display: flex;
      flex-direction: column;
    }
    .tab-bar {
      display: flex;
      gap: 0;
      background: var(--bg-surface);
      border-bottom: 1px solid var(--border);
      flex-shrink: 0;
    }
    .tab {
      padding: 8px 16px;
      font-size: 11px;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.08em;
      color: var(--text-tertiary);
      cursor: pointer;
      border-bottom: 2px solid transparent;
      transition: all 0.15s;
      user-select: none;
    }
    .tab:hover { color: var(--text-secondary); }
    .tab.active {
      color: var(--accent-light);
      border-bottom-color: var(--accent);
    }
    .tab-content {
      flex: 1;
      min-height: 0;
      overflow: hidden;
    }

    /* ── Result panel ───────────────────────────── */
    .result-panel {
      height: 100%;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 16px;
      padding: 24px;
    }
    .result-badge {
      font-size: 36px;
      font-weight: 800;
      letter-spacing: 0.04em;
      padding: 16px 48px;
      border-radius: 12px;
      animation: result-pop 0.3s cubic-bezier(0.34, 1.56, 0.64, 1);
    }
    @keyframes result-pop {
      0% { transform: scale(0.8); opacity: 0; }
      100% { transform: scale(1); opacity: 1; }
    }
    .result-badge.allow {
      background: rgba(52, 211, 153, 0.12);
      color: #34d399;
      box-shadow: 0 0 40px rgba(52, 211, 153, 0.15);
    }
    .result-badge.deny {
      background: rgba(248, 113, 113, 0.12);
      color: #f87171;
      box-shadow: 0 0 40px rgba(248, 113, 113, 0.15);
    }
    .matched-rules {
      font-size: 13px;
      color: var(--text-secondary);
    }
    .matched-rules strong { color: var(--text-primary); }
    .result-empty {
      color: var(--text-tertiary);
      font-size: 14px;
      text-align: center;
    }
    .result-empty kbd {
      display: inline-block;
      background: var(--bg-raised);
      border: 1px solid var(--border);
      border-radius: 4px;
      padding: 2px 6px;
      font-family: inherit;
      font-size: 12px;
    }

    /* ── Error display ──────────────────────────── */
    .error-panel {
      padding: 16px;
      background: rgba(248, 113, 113, 0.06);
      border-left: 3px solid #f87171;
      color: #f87171;
      font-family: 'JetBrains Mono', monospace;
      font-size: 13px;
      white-space: pre-wrap;
      word-break: break-word;
      overflow: auto;
      height: 100%;
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
  `;

  @state() private policy = examples[0].policy;
  @state() private input = examples[0].input;
  @state() private result: EvalResult | null = null;
  @state() private engineReady = false;
  @state() private engineError: string | null = null;
  @state() private rightTab: 'input' | 'result' = 'input';
  @state() private selectedExample = 0;

  async connectedCallback() {
    super.connectedCallback();
    await initEngine();
    this.engineReady = isReady();
    this.engineError = getLoadError();
  }

  private handleRun() {
    this.result = evaluate(this.policy, this.input);
    this.rightTab = 'result';
  }

  private handlePolicyChange(e: CustomEvent<{ value: string }>) {
    this.policy = e.detail.value;
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
    this.rightTab = 'input';
  }

  private handleKeyDown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
      e.preventDefault();
      this.handleRun();
    }
  }

  private renderResult() {
    if (!this.result) {
      return html`
        <div class="result-empty">
          <p>Press <kbd>Run</kbd> or <kbd>Ctrl+Enter</kbd> to evaluate</p>
        </div>
      `;
    }
    if (this.result.error) {
      return html`<div class="error-panel">${this.result.error}</div>`;
    }
    const cls = this.result.decision.toLowerCase();
    return html`
      <div class="result-panel">
        <div class="result-badge ${cls}" key=${Date.now()}>
          ${this.result.decision}
        </div>
        ${this.result.matchedRules.length > 0 ? html`
          <div class="matched-rules">
            <strong>Matched rules:</strong> ${this.result.matchedRules.join(', ')}
          </div>
        ` : nothing}
      </div>
    `;
  }

  render() {
    return html`
      <header @keydown=${this.handleKeyDown}>
        <div class="logo">karu <span>playground</span></div>
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
          <div class="pane-header">Policy</div>
          <div class="pane-body">
            <karu-editor
              language="karu"
              .value=${this.policy}
              @change=${this.handlePolicyChange}
            ></karu-editor>
          </div>
        </div>

        <div class="divider"></div>

        <!-- Right: Input + Result -->
        <div class="pane right-pane">
          <div class="tab-bar">
            <div
              class="tab ${this.rightTab === 'input' ? 'active' : ''}"
              @click=${() => this.rightTab = 'input'}
            >Input JSON</div>
            <div
              class="tab ${this.rightTab === 'result' ? 'active' : ''}"
              @click=${() => this.rightTab = 'result'}
            >Result ${this.result && !this.result.error ? html`<span style="color: ${this.result.decision === 'ALLOW' ? '#34d399' : '#f87171'}">●</span>` : nothing}</div>
          </div>
          <div class="tab-content">
            ${this.rightTab === 'input'
              ? html`
                <karu-editor
                  language="json"
                  .value=${this.input}
                  @change=${this.handleInputChange}
                ></karu-editor>`
              : this.renderResult()
            }
          </div>
        </div>
      </div>
    `;
  }
}
