import type { DataSourcePartition } from '@/types/models';
import type { BitLockerVolumeModel } from '../hooks/use-bitlocker-volume';
import { useBitLockerPanelState } from '../hooks/use-bitlocker-panel-state';
import { BitLockerVolumePanel } from '../components/BitLockerVolumePanel';

export function BitLockerVolumePanelController(props: {
  partition: DataSourcePartition;
  model: BitLockerVolumeModel;
}) {
  const panel = useBitLockerPanelState(props.model);
  return <BitLockerVolumePanel {...props} panel={panel} />;
}
