import type { EmulationWorkspaceModel } from '../use-emulation-workspace-model';
import { useLinuxBypassOutcomePanelModel } from '../linux-bypass-outcome-model';
import { LinuxBypassOutcomePanelView } from '../components/LinuxBypassOutcomePanelView';

export function LinuxBypassOutcomePanelContainer({ model }: { model: EmulationWorkspaceModel }) {
  const panel = useLinuxBypassOutcomePanelModel(model);
  return <LinuxBypassOutcomePanelView {...panel} />;
}
