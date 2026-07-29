import { useNavigate } from 'react-router';
import { CorrelationWorkspace } from '@/features/analysis/components/CorrelationWorkspace';
import { useSelectionStore } from '@/stores/selection-store';
import type { CorrelationSnapshot } from '@/types/models';

interface CorrelationWorkspaceContainerProps {
  snapshot: CorrelationSnapshot;
  onRefresh?: () => void;
  refreshing?: boolean;
}

export function CorrelationWorkspaceContainer({
  snapshot,
  onRefresh,
  refreshing,
}: CorrelationWorkspaceContainerProps) {
  const navigate = useNavigate();
  const setSelectedFileId = useSelectionStore((state) => state.setSelectedFileId);
  const setSelectedArtifactId = useSelectionStore((state) => state.setSelectedArtifactId);
  const setSelectedTimelineId = useSelectionStore((state) => state.setSelectedTimelineId);

  const jumpToTarget = (route: string, targetId: string) => {
    if (route === '/files') {
      setSelectedFileId(targetId);
    } else if (route === '/artifacts') {
      setSelectedArtifactId(targetId);
    } else if (route === '/timeline') {
      setSelectedTimelineId(targetId);
    }
    navigate(route);
  };

  return (
    <CorrelationWorkspace
      snapshot={snapshot}
      onRefresh={onRefresh}
      refreshing={refreshing}
      onJumpToTarget={jumpToTarget}
    />
  );
}
