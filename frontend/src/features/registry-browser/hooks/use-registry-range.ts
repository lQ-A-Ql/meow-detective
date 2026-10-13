import { useQuery } from '@tanstack/react-query';
import type { RegistryBrowserClient } from '@/lib/api/registry-browser';
import type { RegistryHiveSession, RegistryKeyMetadata, RegistryValueMetadata } from '@/types/registryBrowser';
import { REGISTRY_BROWSER_LAYOUT } from '@/design/registry-browser';

export function useRegistryValueRange(client: RegistryBrowserClient, session: RegistryHiveSession | undefined,
  key: RegistryKeyMetadata | undefined, value: RegistryValueMetadata | undefined, offset: number) {
  return useQuery({ queryKey: ['registry-browser', 'range', session?.source ?? null, session?.sessionId ?? null, key?.keyId ?? null, value?.valueId ?? null, offset],
    queryFn: () => client.readValueRange({ sessionId: session!.sessionId, keyId: key!.keyId, valueId: value!.valueId,
      offset, length: REGISTRY_BROWSER_LAYOUT.valueRangeBytes }),
    enabled: Boolean(session && key && value), retry: false, refetchOnWindowFocus: false });
}
