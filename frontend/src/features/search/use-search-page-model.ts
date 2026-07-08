import { useNavigate } from 'react-router';
import { useSelectionStore } from '@/stores/selection-store';

export function useSearchSelection() {
  const selectedSearchHitId = useSelectionStore((state) => state.selectedSearchHitId);
  const setSelectedSearchHitId = useSelectionStore((state) => state.setSelectedSearchHitId);

  return {
    selectedSearchHitId,
    setSelectedSearchHitId,
  };
}

export function useOpenSearchHitInFiles() {
  const navigate = useNavigate();
  const setSelectedFileId = useSelectionStore((state) => state.setSelectedFileId);

  return (fileId: string) => {
    setSelectedFileId(fileId);
    navigate('/files');
  };
}
