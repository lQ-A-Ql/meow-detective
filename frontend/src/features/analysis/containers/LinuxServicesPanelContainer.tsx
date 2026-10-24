import { useLinuxServicesPanelModel, type LinuxServicesPanelProps } from '../use-linux-services-panel-model';
import { LinuxServicesPanelView } from '../components/panels/LinuxServicesPanelView';

export function LinuxServicesPanelContainer(props: LinuxServicesPanelProps) {
  return <LinuxServicesPanelView model={useLinuxServicesPanelModel(props)} />;
}
