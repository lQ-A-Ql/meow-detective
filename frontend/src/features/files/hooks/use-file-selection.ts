import { useSelectionStore } from '@/stores/selection-store';

export function useFileSelection() {
  const selectedDirectoryId = useSelectionStore((s) => s.selectedDirectoryId);
  const setSelectedDirectoryId = useSelectionStore((s) => s.setSelectedDirectoryId);
  const selectedFileId = useSelectionStore((s) => s.selectedFileId);
  const setSelectedFileId = useSelectionStore((s) => s.setSelectedFileId);
  const setSelectedTimelineId = useSelectionStore((s) => s.setSelectedTimelineId);

  return {
    selectedDirectoryId,
    setSelectedDirectoryId,
    selectedFileId,
    setSelectedFileId,
    setSelectedTimelineId,
  };
}
