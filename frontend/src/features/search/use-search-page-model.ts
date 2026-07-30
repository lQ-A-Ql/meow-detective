import { useSelectionStore } from '@/stores/selection-store';

export function useSearchSelection() {
  const selectedSearchHitId = useSelectionStore((state) => state.selectedSearchHitId);
  const setSelectedSearchHitId = useSelectionStore((state) => state.setSelectedSearchHitId);

  return {
    selectedSearchHitId,
    setSelectedSearchHitId,
  };
}
