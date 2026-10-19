import { registryHardwareClient, type RegistryHardwareClient } from '@/lib/api/registry-hardware';
import type { RegistryBrowserContext } from '@/features/registry-browser/types';
import { useRegistryHardwareModel } from '../use-registry-hardware-model';
import { RegistryHardwareView } from '../components/RegistryHardwareView';

export function RegistryHardwareContainer({ context, client = registryHardwareClient }: { context: RegistryBrowserContext; client?: RegistryHardwareClient }) {
  const model = useRegistryHardwareModel(client, context);
  return <RegistryHardwareView model={model} />;
}
