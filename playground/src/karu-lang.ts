/**
 * CodeMirror 6 language support for Karu policy syntax.
 * Uses StreamLanguage for a lightweight, tokenizer-based approach.
 */
import { StreamLanguage, type StreamParser } from '@codemirror/language';
import { tags as t } from '@lezer/highlight';

const keywords = new Set([
  'allow', 'deny', 'if', 'and', 'or', 'not', 'in', 'has', 'like',
  'forall', 'exists', 'is', 'test', 'expect', 'use', 'schema',
  'mod', 'import', 'assert', 'abstract', 'appliesTo', 'actor',
  'resource', 'action', 'context',
]);

const builtinTypes = new Set([
  'String', 'Boolean', 'Long', 'Set',
]);

const constants = new Set(['true', 'false']);

interface KaruState {
  inString: boolean;
  inComment: boolean;
}

const karuParser: StreamParser<KaruState> = {
  name: 'karu',

  startState(): KaruState {
    return { inString: false, inComment: false };
  },

  token(stream, _state): string | null {
    // Skip whitespace
    if (stream.eatSpace()) return null;

    // Comments
    if (stream.match('#')) {
      stream.skipToEnd();
      return 'lineComment';
    }

    // Strings
    if (stream.match('"')) {
      while (!stream.eol()) {
        const ch = stream.next();
        if (ch === '\\') { stream.next(); continue; }
        if (ch === '"') break;
      }
      return 'string';
    }

    // Numbers
    if (stream.match(/^-?\d+(\.\d+)?([eE][+-]?\d+)?/)) {
      return 'number';
    }

    // Operators
    if (stream.match(/^(==|!=|<=|>=|<|>)/)) return 'compareOperator';
    if (stream.match(/^[{}()\[\]:;,._|]/)) return 'punctuation';

    // Identifiers and keywords
    if (stream.match(/^[a-zA-Z_][a-zA-Z0-9_]*/)) {
      const word = stream.current();
      if (keywords.has(word)) {
        if (word === 'allow') return 'keyword';
        if (word === 'deny') return 'keyword';
        if (word === 'test') return 'keyword';
        return 'keyword';
      }
      if (builtinTypes.has(word)) return 'typeName';
      if (constants.has(word)) return 'bool';
      return 'variableName';
    }

    // Skip unknown
    stream.next();
    return null;
  },

  tokenTable: {
    keyword: t.keyword,
    string: t.string,
    number: t.number,
    lineComment: t.lineComment,
    variableName: t.variableName,
    typeName: t.typeName,
    bool: t.bool,
    compareOperator: t.compareOperator,
    punctuation: t.punctuation,
  },
};

export const karuLanguage = StreamLanguage.define(karuParser);
