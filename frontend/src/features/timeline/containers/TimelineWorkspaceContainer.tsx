import { TimelineWorkspace } from '@/features/timeline/components/TimelineWorkspace';
import { useTimelineWorkspaceModel } from '@/features/timeline/use-timeline-workspace-model';

export function TimelineWorkspaceContainer() {
  const model = useTimelineWorkspaceModel();
  return <TimelineWorkspace model={model} />;
}
