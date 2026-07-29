import { ArtifactsWorkspace } from '@/features/artifacts/components/ArtifactsWorkspace';
import { useArtifactsWorkspaceModel } from '@/features/artifacts/use-artifacts-workspace-model';

export function ArtifactsWorkspaceContainer() {
  const model = useArtifactsWorkspaceModel();
  return <ArtifactsWorkspace model={model} />;
}
