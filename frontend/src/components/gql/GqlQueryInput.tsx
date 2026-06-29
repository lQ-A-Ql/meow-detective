import { useState, useRef, useCallback, useMemo } from 'react';
import { getAutocompleteSuggestions, tokenize, type AutocompleteSuggestion } from './gql-language';
import { GqlQueryHeader } from './GqlQueryHeader';
import { GqlQueryEditor } from './GqlQueryEditor';
import { GqlQueryError } from './GqlQueryError';

// Re-export tokenizer so consumers can import it from this module.
export { tokenize } from './gql-language';

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
    [showAutocomplete, suggestions, selectedSuggestion, code, cursorPos, applySuggestion, onExecute],
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
      <GqlQueryHeader loading={loading} executeQuery={executeQuery} code={code} />
      <GqlQueryEditor
        ref={textareaRef}
        value={code}
        placeholder={placeholder}
        tokens={tokens}
        suggestions={suggestions}
        showAutocomplete={showAutocomplete}
        selectedSuggestion={selectedSuggestion}
        onChange={handleChange}
        onKeyDown={handleKeyDown}
        onSelect={handleSelect}
        onInput={handleInput}
        onClick={handleSelect}
        applySuggestion={applySuggestion}
        setSelectedSuggestion={setSelectedSuggestion}
      />
      {error && <GqlQueryError error={error} />}
    </>
  );
}
