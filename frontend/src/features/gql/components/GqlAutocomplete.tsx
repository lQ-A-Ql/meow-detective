import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
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
      className="absolute z-50 bg-forensics-surface border border-forensics-border rounded-none shadow-none
                 max-h-[200px] overflow-y-auto"
      style={{
        left: '16px',
        bottom: '40px',
        minWidth: '200px',
      }}
    >
      {suggestions.map((s, i) => (
        <Button
          type="button"
          key={s.label}
          onClick={() => applySuggestion(s)}
          onMouseEnter={() => setSelectedSuggestion(i)}
          variant="autocompleteOption"
          size="autocompleteItem"
          className={i === selectedSuggestion ? 'bg-forensics-primary-blue text-white' : undefined}
          data-active={i === selectedSuggestion ? 'true' : undefined}
        >
          <span
            className={`text-[10px] px-1 py-0.5 rounded-none ${
              i === selectedSuggestion
                ? 'bg-forensics-surface/20'
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
        </Button>
      ))}
    </div>
  );
}
