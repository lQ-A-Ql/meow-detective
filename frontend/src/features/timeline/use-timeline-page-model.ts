import { useCallback } from 'react';
import { useNavigate } from 'react-router';
import { useSelectionStore } from '@/stores/selection-store';
import type { TimelineEvent } from '@/types/models';

export function useTimelineSelectionModel() {
  const navigate = useNavigate();
  const selectedTimelineId = useSelectionStore((state) => state.selectedTimelineId);
  const setSelectedTimelineId = useSelectionStore((state) => state.setSelectedTimelineId);
  const setSelectedFileId = useSelectionStore((state) => state.setSelectedFileId);
  const setSelectedArtifactId = useSelectionStore((state) => state.setSelectedArtifactId);

  const eventLookupId =
    selectedTimelineId && !selectedTimelineId.startsWith('artifact:')
      ? selectedTimelineId
      : undefined;

  const jumpToSource = useCallback(
    (event?: TimelineEvent) => {
      if (!event) return;

      const sourceId = event.sourceObjectId;
      if (sourceId.startsWith('artifact:')) {
        setSelectedArtifactId(sourceId.replace(/^artifact:/, ''));
        navigate('/artifacts');
        return;
      }

      setSelectedFileId(sourceId);
      navigate('/files');
    },
    [navigate, setSelectedArtifactId, setSelectedFileId],
  );

  return {
    eventLookupId,
    jumpToSource,
    selectedTimelineId,
    setSelectedTimelineId,
  };
}
