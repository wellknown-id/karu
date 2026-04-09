// SPDX-License-Identifier: MIT

/**
 * Reusable CodeMirror 6 editor as a Lit web component.
 * Includes LSP-powered linting, hover tooltips, and autocomplete for Karu.
 */
import { LitElement, html, css, type PropertyValues } from 'lit';
import { customElement, property } from 'lit/decorators.js';
import { EditorState, StateField, StateEffect, RangeSet, type Extension } from '@codemirror/state';
import { EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter, drawSelection, rectangularSelection, hoverTooltip, type Tooltip, gutter, GutterMarker } from '@codemirror/view';
import { defaultKeymap, indentWithTab, history, historyKeymap } from '@codemirror/commands';
import { syntaxHighlighting, defaultHighlightStyle, bracketMatching, foldGutter } from '@codemirror/language';
import { closeBrackets, closeBracketsKeymap, autocompletion, type CompletionContext, type CompletionResult } from '@codemirror/autocomplete';
import { linter, type Diagnostic } from '@codemirror/lint';
import { oneDark } from '@codemirror/theme-one-dark';
import { json } from '@codemirror/lang-json';
import { karuLanguage } from './karu-lang';
import { getDiagnostics, getHover, getCompletions, getCodeActions, isReady, type LspTestResult, type LspRuleCoverage } from './karu-engine';

// ── Test result & coverage gutter markers ─────────────────────────

class TestPassMarker extends GutterMarker {
  toDOM() {
    const el = document.createElement('span');
    el.textContent = '✓';
    el.className = 'test-gutter-pass';
    return el;
  }
}

class TestFailMarker extends GutterMarker {
  toDOM() {
    const el = document.createElement('span');
    el.textContent = '✗';
    el.className = 'test-gutter-fail';
    return el;
  }
}

const testPassMarker = new TestPassMarker();
const testFailMarker = new TestFailMarker();

class CoverageFullMarker extends GutterMarker {
  toDOM() {
    const el = document.createElement('span');
    el.textContent = '●';
    el.className = 'test-gutter-cov-full';
    el.title = 'Full test coverage';
    return el;
  }
}

class CoveragePartialMarker extends GutterMarker {
  toDOM() {
    const el = document.createElement('span');
    el.textContent = '◐';
    el.className = 'test-gutter-cov-partial';
    el.title = 'Partial test coverage';
    return el;
  }
}

class CoverageNoneMarker extends GutterMarker {
  toDOM() {
    const el = document.createElement('span');
    el.textContent = '○';
    el.className = 'test-gutter-cov-none';
    el.title = 'No test coverage';
    return el;
  }
}

const covFullMarker = new CoverageFullMarker();
const covPartialMarker = new CoveragePartialMarker();
const covNoneMarker = new CoverageNoneMarker();

/** Effect to update the test result markers in the gutter. */
const setTestMarkers = StateEffect.define<{ line: number; passed: boolean }[]>();
/** Effect to update the coverage markers in the gutter. */
const setCoverageMarkers = StateEffect.define<{ line: number; status: string }[]>();

interface GutterEntry {
  from: number;
  value: GutterMarker;
}

/** State field tracking test result + coverage markers. */
const testMarkerField = StateField.define<RangeSet<GutterMarker>>({
  create() {
    return RangeSet.empty;
  },
  update(markers, tr) {
    // We rebuild the full set when either effect fires
    let testEntries: GutterEntry[] | null = null;
    let covEntries: GutterEntry[] | null = null;

    for (const effect of tr.effects) {
      if (effect.is(setTestMarkers)) {
        testEntries = [];
        for (const { line, passed } of effect.value) {
          const cmLine = line + 1;
          if (cmLine >= 1 && cmLine <= tr.state.doc.lines) {
            const pos = tr.state.doc.line(cmLine).from;
            testEntries.push({ from: pos, value: passed ? testPassMarker : testFailMarker });
          }
        }
      }
      if (effect.is(setCoverageMarkers)) {
        covEntries = [];
        for (const { line, status } of effect.value) {
          const cmLine = line + 1;
          if (cmLine >= 1 && cmLine <= tr.state.doc.lines) {
            const pos = tr.state.doc.line(cmLine).from;
            const marker = status === 'full' ? covFullMarker : status === 'partial' ? covPartialMarker : covNoneMarker;
            covEntries.push({ from: pos, value: marker });
          }
        }
      }
    }

    if (testEntries || covEntries) {
      // Merge: combine whatever we have
      const all = [...(testEntries || []), ...(covEntries || [])];
      all.sort((a, b) => a.from - b.from);
      return RangeSet.of(all.map(e => e.value.range(e.from)));
    }
    return markers;
  },
});

/** Gutter that shows test pass/fail + coverage markers. */
function testResultGutter() {
  return [
    testMarkerField,
    gutter({
      class: 'test-gutter',
      markers: (view) => view.state.field(testMarkerField),
      initialSpacer: () => testPassMarker,
    }),
  ];
}

/** CodeMirror linter that calls the WASM engine for real-time diagnostics. */
function karuLinter() {
  return linter((view) => {
    if (!isReady()) return [];
    const source = view.state.doc.toString();
    const diags = getDiagnostics(source);
    const actions = getCodeActions(source);

    return diags.map((d): Diagnostic => {
      const line = view.state.doc.line(d.line + 1); // CM lines are 1-indexed
      const from = Math.min(line.from + d.col, line.to);
      const to = Math.min(line.from + d.end_col, line.to);

      // Find matching code actions for this diagnostic
      const diagActions: Array<{ name: string; apply: (view: EditorView, from: number, to: number) => void }> = [];
      if (d.code) {
        for (const action of actions) {
          if (action.diagnostic_code === d.code) {
            diagActions.push({
              name: action.title,
              apply: (v) => {
                // Apply all edits from this action
                const changes = action.edits.map(edit => {
                  const editLine = v.state.doc.line(edit.line + 1);
                  const editFrom = editLine.from + edit.col;
                  const editTo = editLine.from + edit.end_col;
                  return { from: editFrom, to: editTo, insert: edit.new_text };
                });
                v.dispatch({ changes });
              },
            });
          }
        }
      }

      return {
        from,
        to: Math.max(to, from + 1),
        severity: d.severity === 'warning' ? 'warning' : d.severity === 'info' ? 'info' : 'error',
        message: d.message,
        source: d.code ? `karu(${d.code})` : 'karu',
        actions: diagActions.length > 0 ? diagActions : undefined,
      };
    });
  }, { delay: 300 });
}

/** CodeMirror hover tooltip that shows keyword documentation from the WASM engine. */
function karuHoverTooltip() {
  return hoverTooltip((view, pos): Tooltip | null => {
    if (!isReady()) return null;

    // Extract word at position
    const line = view.state.doc.lineAt(pos);
    const text = line.text;
    const col = pos - line.from;

    // Find word boundaries
    let start = col;
    while (start > 0 && /[a-zA-Z_]/.test(text[start - 1])) start--;
    let end = col;
    while (end < text.length && /[a-zA-Z_]/.test(text[end])) end++;
    if (start === end) return null;

    const word = text.slice(start, end);
    const docs = getHover(word);
    if (!docs) return null;

    return {
      pos: line.from + start,
      end: line.from + end,
      above: true,
      create() {
        const dom = document.createElement('div');
        dom.className = 'karu-hover-tooltip';
        dom.innerHTML = docs
          .replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
          .replace(/`(.*?)`/g, '<code>$1</code>');
        dom.style.cssText = `
          padding: 6px 10px;
          font-size: 12px;
          line-height: 1.5;
          max-width: 400px;
          color: #e0e0e0;
        `;
        return { dom };
      },
    };
  });
}

/** CodeMirror autocomplete source powered by the WASM engine. */
function karuCompletionSource(context: CompletionContext): CompletionResult | null {
  if (!isReady()) return null;
  const word = context.matchBefore(/[a-zA-Z_]+/);
  if (!word || (word.from === word.to && !context.explicit)) return null;

  const completions = getCompletions();

  return {
    from: word.from,
    options: completions.map(c => ({
      label: c.label,
      detail: c.detail,
      type: c.kind === 'keyword' ? 'keyword' :
            c.kind === 'operator' ? 'keyword' :
            c.kind === 'variable' ? 'variable' :
            c.kind === 'snippet' ? 'text' : 'text',
      apply: c.insert_text || undefined,
    })),
  };
}

@customElement('karu-editor')
export class KaruEditor extends LitElement {
  static styles = css`
    :host {
      display: block;
      width: 100%;
      height: 100%;
      overflow: hidden;
    }
    .cm-container {
      height: 100%;
    }
    .cm-container .cm-editor {
      height: 100%;
      font-family: 'JetBrains Mono', 'Fira Code', 'Consolas', monospace;
      font-size: 13px;
    }
    .cm-container .cm-editor .cm-scroller {
      overflow: auto;
    }
    .cm-container .cm-editor.cm-focused {
      outline: none;
    }
    /* Diagnostic underlines */
    .cm-container .cm-editor .cm-lintRange-error {
      background-image: none;
      text-decoration: wavy underline #f87171;
      text-underline-offset: 3px;
    }
    .cm-container .cm-editor .cm-lintRange-warning {
      background-image: none;
      text-decoration: wavy underline #fbbf24;
      text-underline-offset: 3px;
    }
    /* Tooltip styling */
    .cm-container .cm-editor .cm-tooltip {
      background: var(--bg-raised, #2a2a3e);
      border: 1px solid var(--border, rgba(255,255,255,0.1));
      border-radius: 6px;
      box-shadow: 0 4px 12px rgba(0,0,0,0.4);
    }
    .cm-container .cm-editor .cm-tooltip code {
      background: rgba(255,255,255,0.08);
      padding: 1px 4px;
      border-radius: 3px;
      font-family: 'JetBrains Mono', monospace;
      font-size: 11px;
    }
    /* Lint gutter */
    .cm-container .cm-editor .cm-lint-marker-error {
      content: '●';
      color: #f87171;
    }
    .cm-container .cm-editor .cm-lint-marker-warning {
      content: '●';
      color: #fbbf24;
    }
    /* Code action buttons in lint tooltips */
    .cm-container .cm-editor .cm-diagnosticAction {
      text-decoration: underline;
      cursor: pointer;
      color: #60a5fa;
      font-weight: 500;
    }
    .cm-container .cm-editor .cm-diagnosticAction:hover {
      color: #93c5fd;
    }
    /* Test result gutter */
    .cm-container .cm-editor .test-gutter {
      width: 16px;
      text-align: center;
    }
    .cm-container .cm-editor .test-gutter-pass {
      color: #34d399;
      font-size: 13px;
      font-weight: bold;
      line-height: 1;
    }
    .cm-container .cm-editor .test-gutter-fail {
      color: #f87171;
      font-size: 13px;
      font-weight: bold;
      line-height: 1;
    }
    .cm-container .cm-editor .test-gutter-cov-full {
      color: #34d399;
      font-size: 10px;
      line-height: 1;
    }
    .cm-container .cm-editor .test-gutter-cov-partial {
      color: #fbbf24;
      font-size: 10px;
      line-height: 1;
    }
    .cm-container .cm-editor .test-gutter-cov-none {
      color: rgba(255,255,255,0.2);
      font-size: 10px;
      line-height: 1;
    }
  `;

  @property({ type: String }) language: 'karu' | 'json' = 'karu';
  @property({ type: String }) value = '';
  @property({ type: Boolean }) readonly = false;
  @property({ attribute: false }) testResults: LspTestResult[] | null = null;
  @property({ attribute: false }) coverage: LspRuleCoverage[] | null = null;

  private view?: EditorView;
  private _container?: HTMLDivElement;

  private getLanguageExtension(): Extension[] {
    if (this.language === 'json') {
      return [json()];
    }
    return [
      karuLanguage,
      karuLinter(),
      karuHoverTooltip(),
      autocompletion({ override: [karuCompletionSource] }),
      ...testResultGutter(),
    ];
  }

  private createView() {
    if (!this._container) return;

    // Destroy previous
    this.view?.destroy();

    const updateListener = EditorView.updateListener.of((update) => {
      if (update.docChanged) {
        this.value = update.state.doc.toString();
        this.dispatchEvent(new CustomEvent('change', {
          detail: { value: this.value },
          bubbles: true,
          composed: true,
        }));
      }
    });

    const extensions: Extension[] = [
      lineNumbers(),
      highlightActiveLine(),
      highlightActiveLineGutter(),
      drawSelection(),
      rectangularSelection(),
      bracketMatching(),
      closeBrackets(),
      foldGutter(),
      history(),
      keymap.of([
        ...defaultKeymap,
        ...historyKeymap,
        ...closeBracketsKeymap,
        indentWithTab,
      ]),
      oneDark,
      syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
      ...this.getLanguageExtension(),
      updateListener,
      EditorView.theme({
        '&': { backgroundColor: 'transparent' },
        '.cm-gutters': {
          backgroundColor: 'transparent',
          borderRight: '1px solid rgba(255,255,255,0.06)',
        },
      }),
    ];

    if (this.readonly) {
      extensions.push(EditorState.readOnly.of(true));
    }

    this.view = new EditorView({
      state: EditorState.create({
        doc: this.value,
        extensions,
      }),
      parent: this._container,
    });
  }

  protected firstUpdated(_changedProperties: PropertyValues) {
    super.firstUpdated(_changedProperties);
    this._container = this.shadowRoot!.querySelector('.cm-container') as HTMLDivElement;
    this.createView();
  }

  protected updated(changedProperties: PropertyValues) {
    super.updated(changedProperties);
    if (changedProperties.has('value') && this.view) {
      const currentValue = this.view.state.doc.toString();
      if (currentValue !== this.value) {
        this.view.dispatch({
          changes: { from: 0, to: currentValue.length, insert: this.value },
        });
      }
    }
    if ((changedProperties.has('testResults') || changedProperties.has('coverage')) && this.view) {
      this.updateTestMarkers();
    }
  }

  /** Dispatch test result markers into the editor gutter. */
  private updateTestMarkers() {
    if (!this.view) return;
    const effects: StateEffect<unknown>[] = [];

    const markers = (this.testResults || []).map(t => ({
      line: t.line,
      passed: t.passed,
    }));
    effects.push(setTestMarkers.of(markers));

    const covMarkers = (this.coverage || []).map(c => ({
      line: c.line,
      status: c.status,
    }));
    effects.push(setCoverageMarkers.of(covMarkers));

    this.view.dispatch({ effects });
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this.view?.destroy();
  }

  render() {
    return html`<div class="cm-container"></div>`;
  }
}
