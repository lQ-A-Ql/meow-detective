import { forwardRef } from 'react';
import type { KeyboardEvent, SyntheticEvent, FormEvent } from 'react';
import type { HighlightToken, AutocompleteSuggestion } from './gql-language';
import { HIGHLIGHT_COLORS } from './gql-language';
import { GqlAutocomplete } from './GqlAutocomplete';
import { Textarea } from '@/app/components/ui/textarea';

interface GqlQueryEditorProps {
  value: string;
  placeholder: string;
  tokens: HighlightToken[];
  suggestions: AutocompleteSuggestion[];
  showAutocomplete: boolean;
  selectedSuggestion: number;
  onChange: (value: string) => void;
  onKeyDown: (e: KeyboardEvent<HTMLTextAreaElement>) => void;
  onSelect: (e: SyntheticEvent<HTMLTextAreaElement>) => void;
  onInput: (e: FormEvent<HTMLTextAreaElement>) => void;
  onClick: (e: SyntheticEvent<HTMLTextAreaElement>) => void;
  applySuggestion: (suggestion: AutocompleteSuggestion) => void;
  setSelectedSuggestion: (index: number) => void;
}

export const GqlQueryEditor = forwardRef<HTMLTextAreaElement, GqlQueryEditorProps>(
  function GqlQueryEditor(
    {
      value,
      placeholder,
      tokens,
      suggestions,
      showAutocomplete,
      selectedSuggestion,
      onChange,
      onKeyDown,
      onSelect,
      onInput,
      onClick,
      applySuggestion,
      setSelectedSuggestion,
    },
    ref,
  ) {
    return (
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
        <Textarea
          ref={ref}
          value={value}
          onChange={(e) => onChange(e.currentTarget.value)}
          onKeyDown={onKeyDown}
          onSelect={onSelect}
          onInput={onInput}
          onClick={onClick}
          placeholder={placeholder}
          unstyled
          className="absolute inset-0 h-full w-full resize-none border-none bg-transparent px-3 py-2 font-mono text-[13px]
                     leading-relaxed text-transparent caret-forensics-gql-base outline-none"
          style={{ WebkitTextFillColor: 'transparent' }}
          spellCheck={false}
        />

        {/* Autocomplete dropdown */}
        {showAutocomplete && suggestions.length > 0 && (
          <GqlAutocomplete
            suggestions={suggestions}
            selectedSuggestion={selectedSuggestion}
            applySuggestion={applySuggestion}
            setSelectedSuggestion={setSelectedSuggestion}
          />
        )}
      </div>
    );
  },
);
