/**
 * Reusable CodeMirror 6 editor as a Lit web component.
 */
import { LitElement, html, css, type PropertyValues } from 'lit';
import { customElement, property } from 'lit/decorators.js';
import { EditorState, type Extension } from '@codemirror/state';
import { EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter, drawSelection, rectangularSelection } from '@codemirror/view';
import { defaultKeymap, indentWithTab, history, historyKeymap } from '@codemirror/commands';
import { syntaxHighlighting, defaultHighlightStyle, bracketMatching, foldGutter } from '@codemirror/language';
import { closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete';
import { oneDark } from '@codemirror/theme-one-dark';
import { json } from '@codemirror/lang-json';
import { karuLanguage } from './karu-lang';

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
  `;

  @property({ type: String }) language: 'karu' | 'json' = 'karu';
  @property({ type: String }) value = '';
  @property({ type: Boolean }) readonly = false;

  private view?: EditorView;
  private _container?: HTMLDivElement;

  private getLanguageExtension(): Extension {
    return this.language === 'json' ? json() : karuLanguage;
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
      this.getLanguageExtension(),
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
