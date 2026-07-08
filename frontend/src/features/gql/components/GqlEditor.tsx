import type { GraphQueryResult } from '@/types/models';
import { GqlQueryInput, type GqlQueryInputProps } from './GqlQueryInput';
import { GqlResultView } from './GqlResultView';

// ── Props ──

export interface GqlEditorProps extends GqlQueryInputProps {
  /** Query result to display (null = no result yet). */
  result?: GraphQueryResult | null;
}

// ── Component ──

export function GqlEditor({
  onExecute,
  result,
  loading = false,
  error,
  initialQuery = '',
  placeholder,
  onQueryChange,
}: GqlEditorProps) {
  return (
    <div className="flex flex-col h-full bg-white border border-[#e0e0e0] rounded-lg overflow-hidden">
      <GqlQueryInput
        onExecute={onExecute}
        loading={loading}
        error={error}
        initialQuery={initialQuery}
        placeholder={placeholder}
        onQueryChange={onQueryChange}
      />

      {/* Results panel */}
      {result && <GqlResultView result={result} />}
    </div>
  );
}

export default GqlEditor;
