/**
 * Reusable CodeMirror 6 editor as a Lit web component.
 * Includes LSP-powered linting, hover tooltips, and autocomplete for Karu.
 */
import { LitElement, html, css, type PropertyValues } from 'lit';
import { customElement, property } from 'lit/decorators.js';
import { EditorState, type Extension } from '@codemirror/state';
import { EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter, drawSelection, rectangularSelection, hoverTooltip, type Tooltip } from '@codemirror/view';
import { defaultKeymap, indentWithTab, history, historyKeymap } from '@codemirror/commands';
import { syntaxHighlighting, defaultHighlightStyle, bracketMatching, foldGutter } from '@codemirror/language';
import { closeBrackets, closeBracketsKeymap, autocompletion, type CompletionContext, type CompletionResult } from '@codemirror/autocomplete';
import { linter, type Diagnostic } from '@codemirror/lint';
import { oneDark } from '@codemirror/theme-one-dark';
import { json } from '@codemirror/lang-json';
import { karuLanguage } from './karu-lang';
import { getDiagnostics, getHover, getCompletions, getCodeActions, isReady } from './karu-engine';

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
  `;

  @property({ type: String }) language: 'karu' | 'json' = 'karu';
  @property({ type: String }) value = '';
  @property({ type: Boolean }) readonly = false;

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
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this.view?.destroy();
  }

  render() {
    return html`<div class="cm-container"></div>`;
  }
}
