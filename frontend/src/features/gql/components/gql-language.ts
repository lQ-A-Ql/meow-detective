import type { KeyboardEvent } from 'react';

// ── GQL language constants ──

export const NODE_TYPES = ['file', 'artifact', 'timelineEvent', 'entity', 'lead', 'notebookEntry'] as const;
export const EDGE_TYPES = ['contains', 'references', 'correlatesWith', 'derivesFrom', 'precedes', 'cites', 'annotates'] as const;
export const KEYWORDS = ['MATCH', 'WHERE', 'RETURN', 'LIMIT', 'AND', 'OR', 'LIKE', 'CONTAINS', 'NOT', 'count'] as const;
export const EDGE_PROPERTIES = ['confidence', 'provenance', 'edgeType', 'sourceId', 'targetId'] as const;
export const NODE_PROPERTIES = ['label', 'summary', 'tags', 'nodeType'] as const;

// ── Syntax highlighting ──

export interface HighlightToken {
  text: string;
  type: 'keyword' | 'type' | 'variable' | 'operator' | 'string' | 'number' | 'comment' | 'punctuation' | 'plain';
}

export function tokenize(code: string): HighlightToken[] {
  const tokens: HighlightToken[] = [];
  let i = 0;

  while (i < code.length) {
    // Whitespace
    if (code[i] === ' ' || code[i] === '\t' || code[i] === '\n') {
      let ws = '';
      while (i < code.length && (code[i] === ' ' || code[i] === '\t' || code[i] === '\n')) {
        ws += code[i];
        i++;
      }
      tokens.push({ text: ws, type: 'plain' });
      continue;
    }

    // Strings
    if (code[i] === "'" || code[i] === '"') {
      const quote = code[i];
      let s = quote;
      i++;
      while (i < code.length && code[i] !== quote) {
        s += code[i];
        i++;
      }
      if (i < code.length) {
        s += code[i];
        i++;
      }
      tokens.push({ text: s, type: 'string' });
      continue;
    }

    // Numbers
    if (code[i] >= '0' && code[i] <= '9') {
      let num = '';
      while (i < code.length && ((code[i] >= '0' && code[i] <= '9') || code[i] === '.')) {
        num += code[i];
        i++;
      }
      tokens.push({ text: num, type: 'number' });
      continue;
    }

    // Punctuation: ()[]-,:.*
    if ('()[],:.*'.includes(code[i])) {
      let punct = code[i];
      i++;
      // Arrow ->
      if (punct === '-' && i < code.length && code[i] === '>') {
        punct += '>';
        i++;
      }
      if (punct === '<' && i < code.length && code[i] === '-') {
        punct += '-';
        i++;
      }
      tokens.push({ text: punct, type: 'punctuation' });
      continue;
    }

    // Comparison operators
    if ('=!><'.includes(code[i])) {
      let op = code[i];
      i++;
      if (i < code.length && code[i] === '=') {
        op += '=';
        i++;
      }
      tokens.push({ text: op, type: 'operator' });
      continue;
    }

    // Identifiers / keywords
    if (/[a-zA-Z_]/.test(code[i])) {
      let word = '';
      while (i < code.length && /[a-zA-Z0-9_]/.test(code[i])) {
        word += code[i];
        i++;
      }
      const upper = word.toUpperCase();
      if ((KEYWORDS as readonly string[]).includes(upper)) {
        tokens.push({ text: word, type: 'keyword' });
      } else if ((NODE_TYPES as readonly string[]).includes(word)) {
        tokens.push({ text: word, type: 'type' });
      } else if ((EDGE_TYPES as readonly string[]).includes(word)) {
        tokens.push({ text: word, type: 'type' });
      } else {
        tokens.push({ text: word, type: 'variable' });
      }
      continue;
    }

    // Any other character
    tokens.push({ text: code[i], type: 'plain' });
    i++;
  }

  return tokens;
}

// ── Autocomplete ──

export interface AutocompleteSuggestion {
  label: string;
  insertText: string;
  description: string;
  kind: 'keyword' | 'type' | 'property' | 'predicate';
}

export function getAutocompleteSuggestions(
  code: string,
  cursorPos: number,
): AutocompleteSuggestion[] {
  // Find the current word being typed
  const beforeCursor = code.slice(0, cursorPos);
  const wordMatch = beforeCursor.match(/([a-zA-Z_]*)$/);
  if (!wordMatch) return [];

  const prefix = wordMatch[1].toLowerCase();
  if (prefix.length === 0) return [];

  const suggestions: AutocompleteSuggestion[] = [];

  // Determine context
  const context = beforeCursor.toUpperCase();

  // After MATCH, suggest node/edge patterns
  if (context.includes('MATCH')) {
    // Node types (after colon)
    if (beforeCursor.endsWith(':') || beforeCursor.endsWith(':' + prefix)) {
      for (const nt of NODE_TYPES) {
        if (nt.toLowerCase().startsWith(prefix)) {
          suggestions.push({
            label: nt,
            insertText: nt,
            description: `Node type: ${nt}`,
            kind: 'type',
          });
        }
      }
    }
    // Edge types
    if (/\[[a-zA-Z_]*:?[a-zA-Z_]*$/.test(beforeCursor)) {
      if (beforeCursor.endsWith(':') || beforeCursor.endsWith(':' + prefix)) {
        for (const et of EDGE_TYPES) {
          if (et.toLowerCase().startsWith(prefix)) {
            suggestions.push({
              label: et,
              insertText: et,
              description: `Edge type: ${et}`,
              kind: 'type',
            });
          }
        }
      }
    }
    // Keywords
    for (const kw of KEYWORDS) {
      if (kw.toLowerCase().startsWith(prefix)) {
        suggestions.push({
          label: kw,
          insertText: kw,
          description: `Keyword: ${kw}`,
          kind: 'keyword',
        });
      }
    }
  }

  // In WHERE clause, suggest properties
  if (context.includes('WHERE')) {
    // After a variable name and dot
    const afterWhere = beforeCursor.slice(beforeCursor.lastIndexOf('WHERE') + 5);
    const dotMatch = afterWhere.match(/([a-zA-Z_])\.([a-zA-Z_]*)$/);
    if (dotMatch) {
      const varName = dotMatch[1];
      const propPrefix = dotMatch[2].toLowerCase();
      const properties = varName === 'e' ? EDGE_PROPERTIES : NODE_PROPERTIES;
      for (const prop of properties) {
        if (prop.toLowerCase().startsWith(propPrefix)) {
          suggestions.push({
            label: prop,
            insertText: prop,
            description: `Property: ${varName}.${prop}`,
            kind: 'property',
          });
        }
      }
    }
    // Also suggest operators: LIKE, CONTAINS
    const opKeywords = ['LIKE', 'CONTAINS', 'AND', 'OR'];
    for (const kw of opKeywords) {
      if (kw.toLowerCase().startsWith(prefix)) {
        suggestions.push({
          label: kw,
          insertText: kw,
          description: `Operator: ${kw}`,
          kind: 'keyword',
        });
      }
    }
  }

  // In RETURN clause, suggest aggregate functions
  if (context.includes('RETURN') && !context.includes('WHERE')) {
    const aggFuncs = ['count(*)', 'min(', 'max(', 'avg(', 'sum('];
    for (const fn of aggFuncs) {
      if (fn.toLowerCase().startsWith(prefix)) {
        suggestions.push({
          label: fn,
          insertText: fn,
          description: `Aggregate: ${fn}`,
          kind: 'keyword',
        });
      }
    }
  }

  // General keywords
  for (const kw of KEYWORDS) {
    if (kw.toLowerCase().startsWith(prefix)) {
      const exists = suggestions.some((s) => s.label === kw);
      if (!exists) {
        suggestions.push({
          label: kw,
          insertText: kw,
          description: `Keyword: ${kw}`,
          kind: 'keyword',
        });
      }
    }
  }

  return suggestions.slice(0, 10);
}

// ── Highlight colors ──

export const HIGHLIGHT_COLORS: Record<HighlightToken['type'], string> = {
  keyword: 'text-forensics-gql-keyword',
  type: 'text-forensics-gql-type',
  variable: 'text-forensics-gql-variable',
  operator: 'text-forensics-gql-keyword',
  string: 'text-forensics-gql-string',
  number: 'text-forensics-gql-variable',
  comment: 'text-forensics-gql-muted',
  punctuation: 'text-forensics-gql-base',
  plain: 'text-forensics-gql-base',
};

// Re-export KeyboardEvent type for subcomponents that reference native events.
export type { KeyboardEvent };
