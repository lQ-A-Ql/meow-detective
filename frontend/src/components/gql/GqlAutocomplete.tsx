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
  return (
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
  );
}
