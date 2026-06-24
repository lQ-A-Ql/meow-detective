import { useState, useRef, useCallback, useMemo } from 'react';
import { Play, RefreshCw } from 'lucide-react';

// ── GQL language constants ──

const NODE_TYPES = ['file', 'artifact', 'timelineEvent', 'entity', 'lead', 'notebookEntry'] as const;
const EDGE_TYPES = ['contains', 'references', 'correlatesWith', 'derivesFrom', 'precedes', 'cites', 'annotates'] as const;
const KEYWORDS = ['MATCH', 'WHERE', 'RETURN', 'LIMIT', 'AND', 'OR', 'LIKE', 'CONTAINS', 'NOT', 'count'] as const;
const EDGE_PROPERTIES = ['confidence', 'provenance', 'edgeType', 'sourceId', 'targetId'] as const;
const NODE_PROPERTIES = ['label', 'summary', 'tags', 'nodeType'] as const;

// ── Syntax highlighting ──

interface HighlightToken {
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

interface AutocompleteSuggestion {
  label: string;
  insertText: string;
  description: string;
  kind: 'keyword' | 'type' | 'property' | 'predicate';
}

function getAutocompleteSuggestions(
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

const HIGHLIGHT_COLORS: Record<HighlightToken['type'], string> = {
  keyword: 'text-[#d73a49]',
  type: 'text-[#6f42c1]',
  variable: 'text-[#005cc5]',
  operator: 'text-[#d73a49]',
  string: 'text-[#032f62]',
  number: 'text-[#005cc5]',
  comment: 'text-[#6a737d]',
  punctuation: 'text-[#24292e]',
  plain: 'text-[#24292e]',
};

// ── Props ──

export interface GqlQueryInputProps {
  onExecute?: (query: string) => void;
  loading?: boolean;
  error?: string | null;
  initialQuery?: string;
  placeholder?: string;
  onQueryChange?: (query: string) => void;
}

// ── Component ──

export function GqlQueryInput({
  onExecute,
  loading = false,
  error,
  initialQuery = '',
  placeholder = 'MATCH (n:File)-[e:References]->(m:Artifact)\nWHERE e.confidence > 0.7\nRETURN n, e, m\nLIMIT 50',
  onQueryChange,
}: GqlQueryInputProps) {
  const [code, setCode] = useState(initialQuery);
  const [cursorPos, setCursorPos] = useState(0);
  const [showAutocomplete, setShowAutocomplete] = useState(false);
  const [selectedSuggestion, setSelectedSuggestion] = useState(0);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Tokenize for highlighting
  const tokens = useMemo(() => tokenize(code), [code]);

  // Autocomplete suggestions
  const suggestions = useMemo(
    () => getAutocompleteSuggestions(code, cursorPos),
    [code, cursorPos],
  );

  const handleChange = useCallback(
    (value: string) => {
      setCode(value);
      onQueryChange?.(value);
    },
    [onQueryChange],
  );

  const applySuggestion = useCallback(
    (suggestion: AutocompleteSuggestion) => {
      const beforeCursor = code.slice(0, cursorPos);
      const afterCursor = code.slice(cursorPos);
      const wordMatch = beforeCursor.match(/([a-zA-Z_]*)$/);
      if (!wordMatch) return;

      const wordStart = cursorPos - wordMatch[1].length;
      const newCode =
        code.slice(0, wordStart) + suggestion.insertText + afterCursor;
      setCode(newCode);
      setShowAutocomplete(false);
      onQueryChange?.(newCode);
    },
    [code, cursorPos, onQueryChange],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === 'Enter' && e.ctrlKey) {
        e.preventDefault();
        onExecute?.(code);
        return;
      }

      if (showAutocomplete && suggestions.length > 0) {
        if (e.key === 'ArrowDown') {
          e.preventDefault();
          setSelectedSuggestion((prev) =>
            prev < suggestions.length - 1 ? prev + 1 : 0,
          );
        } else if (e.key === 'ArrowUp') {
          e.preventDefault();
          setSelectedSuggestion((prev) =>
            prev > 0 ? prev - 1 : suggestions.length - 1,
          );
        } else if (e.key === 'Enter' || e.key === 'Tab') {
          e.preventDefault();
          applySuggestion(suggestions[selectedSuggestion]);
        } else if (e.key === 'Escape') {
          setShowAutocomplete(false);
        }
      }
    },
    [showAutocomplete, suggestions, selectedSuggestion, code, cursorPos],
  );

  const handleSelect = useCallback(
    (e: React.SyntheticEvent<HTMLTextAreaElement>) => {
      const textarea = e.currentTarget;
      setCursorPos(textarea.selectionStart);
      if (suggestions.length > 0) {
        setShowAutocomplete(true);
        setSelectedSuggestion(0);
      } else {
        setShowAutocomplete(false);
      }
    },
    [suggestions],
  );

  const handleInput = useCallback(
    (e: React.FormEvent<HTMLTextAreaElement>) => {
      const textarea = e.currentTarget;
      handleChange(textarea.value);
      setCursorPos(textarea.selectionStart);
      if (suggestions.length > 0) {
        setShowAutocomplete(true);
      }
    },
    [handleChange, suggestions],
  );

  const executeQuery = useCallback(() => {
    onExecute?.(code);
  }, [onExecute, code]);

  return (
    <>
      {/* Editor header */}
      <div className="flex items-center justify-between px-3 py-2 border-b border-[#e0e0e0] bg-[#f6f8fa]">
        <span className="text-[11px] font-semibold text-[#586069] uppercase tracking-wider">
          GQL Query
        </span>
        <div className="flex items-center gap-2">
          {loading && (
            <span className="text-[11px] text-[#586069] flex items-center gap-1">
              <RefreshCw size={12} className="animate-spin" />
              Running...
            </span>
          )}
          <button
            onClick={executeQuery}
            disabled={loading || !code.trim()}
            className="flex items-center gap-1 px-3 py-1 rounded text-[11px] font-medium
                       bg-[#2ea44f] text-white hover:bg-[#2c974b] transition-colors
                       disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <Play size={12} />
            Run
          </button>
        </div>
      </div>

      {/* Editor body */}
      <div className="relative flex-1 min-h-[120px]">
        {/* Highlight layer */}
        <div
          className="absolute inset-0 px-3 py-2 font-mono text-[13px] leading-relaxed
                     whitespace-pre-wrap break-words overflow-auto pointer-events-none"
          aria-hidden="true"
        >
          {tokens.map((token, i) => (
            <span key={i} className={HIGHLIGHT_COLORS[token.type]}>
              {token.text}
            </span>
          ))}
        </div>

        {/* Textarea */}
        <textarea
          ref={textareaRef}
          value={code}
          onChange={(e) => handleChange(e.currentTarget.value)}
          onKeyDown={handleKeyDown}
          onSelect={handleSelect}
          onInput={handleInput}
          onClick={handleSelect}
          placeholder={placeholder}
          className="absolute inset-0 w-full h-full resize-none px-3 py-2 font-mono text-[13px]
                     leading-relaxed bg-transparent text-transparent caret-[#24292e]
                     outline-none border-none"
          style={{ WebkitTextFillColor: 'transparent' }}
          spellCheck={false}
        />

        {/* Autocomplete dropdown */}
        {showAutocomplete && suggestions.length > 0 && (
          <div
            className="absolute z-50 bg-white border border-[#e0e0e0] rounded-md shadow-lg
                       max-h-[200px] overflow-y-auto"
            style={{
              left: '16px',
              bottom: '40px',
              minWidth: '200px',
            }}
          >
            {suggestions.map((s, i) => (
              <button
                key={s.label}
                onClick={() => applySuggestion(s)}
                onMouseEnter={() => setSelectedSuggestion(i)}
                className={`w-full text-left px-3 py-1.5 text-[12px] font-mono flex items-center gap-2
                  ${i === selectedSuggestion ? 'bg-[#0366d6] text-white' : 'hover:bg-[#f6f8fa]'}`}
              >
                <span
                  className={`text-[10px] px-1 py-0.5 rounded ${
                    i === selectedSuggestion
                      ? 'bg-white/20'
                      : s.kind === 'keyword'
                        ? 'bg-[#d73a49]/10 text-[#d73a49]'
                        : s.kind === 'type'
                          ? 'bg-[#6f42c1]/10 text-[#6f42c1]'
                          : 'bg-[#005cc5]/10 text-[#005cc5]'
                  }`}
                >
                  {s.kind}
                </span>
                <span className="flex-1 truncate">{s.label}</span>
                <span className="text-[10px] opacity-60 truncate max-w-[120px]">
                  {s.description}
                </span>
              </button>
            ))}
          </div>
        )}
      </div>

      {/* Error */}
      {error && (
        <div className="px-3 py-2 bg-[#fff0f0] border-t border-[#ffcccc] text-[#d73a49] text-[12px] font-mono">
          {error}
        </div>
      )}
    </>
  );
}
