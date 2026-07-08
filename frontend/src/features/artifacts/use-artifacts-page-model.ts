import { useCallback } from 'react';
import { useNavigate } from 'react-router';
import { useSelectionStore } from '@/stores/selection-store';
import type { ArtifactRow } from '@/types/models';

export function useArtifactsSelectionModel() {
  const navigate = useNavigate();
  const selectedArtifactFamily = useSelectionStore(
    (state) => state.selectedArtifactFamily,
  );
  const setSelectedArtifactFamily = useSelectionStore(
    (state) => state.setSelectedArtifactFamily,
  );
  const selectedArtifactId = useSelectionStore((state) => state.selectedArtifactId);
  const setSelectedArtifactId = useSelectionStore(
    (state) => state.setSelectedArtifactId,
  );
  const setSelectedFileId = useSelectionStore((state) => state.setSelectedFileId);
  const setSelectedTimelineId = useSelectionStore(
    (state) => state.setSelectedTimelineId,
  );

  const openArtifactSource = useCallback(
    (artifact?: ArtifactRow) => {
      const sourceObjectId = artifact?.sourceObjectId;
      if (!sourceObjectId) return;

      setSelectedFileId(sourceObjectId);
      navigate('/files');
    },
    [navigate, setSelectedFileId],
  );

  const openArtifactTimeline = useCallback(
    (artifact?: ArtifactRow) => {
      if (!artifact) return;

      setSelectedTimelineId(`artifact:${artifact.id}`);
      navigate('/timeline');
    },
    [navigate, setSelectedTimelineId],
  );

  return {
    openArtifactSource,
    openArtifactTimeline,
    selectedArtifactFamily,
    selectedArtifactId,
    setSelectedArtifactFamily,
    setSelectedArtifactId,
  };
}
