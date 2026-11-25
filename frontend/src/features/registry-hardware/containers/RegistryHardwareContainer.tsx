import type { RegistryHardwareDevice } from '@/types/analysisRegistry';
import { useRegistryHardwareModel } from '../use-registry-hardware-model';
import { RegistryHardwareView } from '../components/RegistryHardwareView';

export function RegistryHardwareContainer({
  devices,
  warnings,
}: {
  devices: RegistryHardwareDevice[];
  warnings: string[];
}) {
  const model = useRegistryHardwareModel(devices, warnings);
  return <RegistryHardwareView model={model} />;
}
