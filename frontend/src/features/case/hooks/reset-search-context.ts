import type { QueryClient } from '@tanstack/react-query';
import type { CaseSummary } from '@/types/models';
import { useSelectionStore } from '@/stores/selection-store';

export function resetSearchContext(cache: QueryClient, currentCase: CaseSummary | null) {
  void cache.cancelQueries({ queryKey: ['search'] });
  cache.removeQueries({ queryKey: ['search'] });
  void cache.cancelQueries({ queryKey: ['files', 'jump-context'] });
  cache.removeQueries({ queryKey: ['files', 'jump-context'] });
  cache.setQueryData(['case', 'current'], currentCase);
  useSelectionStore.setState({ selectedDirectoryId: undefined, selectedFileId: undefined,
    selectedSearchHitId: undefined, selectedArtifactId: undefined, selectedTimelineId: undefined });
}
