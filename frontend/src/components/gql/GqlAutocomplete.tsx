import { useTranslation } from 'react-i18next';
import type { AutocompleteSuggestion } from './gql-language';

interface GqlAutocompleteProps {
  suggestions: AutocompleteSuggestion[];
  selectedSuggestion: number;
  applySuggestion: (suggestion: AutocompleteSuggestion) => void;
  setSelectedSuggestion: (index: number) => void;
}

export function GqlAutocomplete({
  suggestions,
  selectedSuggestion,
  applySuggestion,
  setSelectedSuggestion,
}: GqlAutocompleteProps) {
  const { t } = useTranslation();

  return (
    <div
      className="absolute z-50 bg-forensics-surface border border-forensics-border rounded-md shadow-lg
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
            ${i === selectedSuggestion ? 'bg-[#0366d6] text-white' : 'hover:bg-forensics-highlight'}`}
        >
          <span
            className={`text-[10px] px-1 py-0.5 rounded ${
              i === selectedSuggestion
                ? 'bg-white/20'
                : s.kind === 'keyword'
                  ? 'bg-forensics-gql-keyword/10 text-forensics-gql-keyword'
                  : s.kind === 'type'
                    ? 'bg-forensics-gql-type/10 text-forensics-gql-type'
                    : 'bg-forensics-gql-variable/10 text-forensics-gql-variable'
            }`}
          >
            {t(`gql.autocomplete.kind.${s.kind}`)}
          </span>
          <span className="flex-1 truncate">{s.label}</span>
          <span className="text-[10px] opacity-60 truncate max-w-[120px]">
            {s.description}
          </span>
        </button>
      ))}
    </div>
  );
}
