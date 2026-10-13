import { useInfiniteQuery } from '@tanstack/react-query';
import type { RegistryBrowserClient } from '@/lib/api/registry-browser';
import type { RegistryHiveSession, RegistryKeyMetadata } from '@/types/registryBrowser';
import { REGISTRY_BROWSER_LAYOUT } from '@/design/registry-browser';

export function useRegistryValues(client: RegistryBrowserClient, session?: RegistryHiveSession, key?: RegistryKeyMetadata) {
  return useInfiniteQuery({ queryKey: ['registry-browser', 'values', session?.source ?? null, session?.sessionId ?? null, key?.keyId ?? null],
    initialPageParam: undefined as string | undefined,
    queryFn: ({ pageParam }) => client.listValues({ sessionId: session!.sessionId, keyId: key!.keyId,
      cursor: pageParam, limit: REGISTRY_BROWSER_LAYOUT.pageSize }),
    getNextPageParam: (page) => page.nextCursor,
    enabled: Boolean(session && key), retry: false, refetchOnWindowFocus: false });
}
