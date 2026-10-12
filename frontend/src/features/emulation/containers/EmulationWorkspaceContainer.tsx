import { EmulationWorkspaceController } from './EmulationWorkspaceController';
import { useEmulationWorkspaceModel } from '@/features/emulation/use-emulation-workspace-model';

export function EmulationWorkspaceContainer() {
  const model = useEmulationWorkspaceModel();
  return <EmulationWorkspaceController model={model} />;
}
