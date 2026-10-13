import type { RegistryBrowserClient } from '@/lib/api/registry-browser';
import type { RegistryBrowserContext } from '../types';
import { useRegistryBrowserModel } from '../use-registry-browser-model';
import { RegistryBrowserView } from '../components/RegistryBrowserView';

export interface RegistryBrowserProps {
  client: RegistryBrowserClient; context: RegistryBrowserContext; initialKeyPath?: string;
}
export function RegistryBrowserController({ client, context, initialKeyPath }: RegistryBrowserProps) {
  const model = useRegistryBrowserModel(client, context, initialKeyPath);
  return <RegistryBrowserView model={model} />;
}
