import { EmulationWorkspace } from '@/features/emulation/components/EmulationWorkspace';
import { useEmulationWorkspaceModel } from '@/features/emulation/use-emulation-workspace-model';

export function EmulationWorkspaceContainer() {
  const model = useEmulationWorkspaceModel();
  return <EmulationWorkspace model={model} />;
}
