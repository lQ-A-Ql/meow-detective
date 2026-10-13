import { useQuery } from '@tanstack/react-query';
import type { RegistryBrowserClient } from '@/lib/api/registry-browser';
import type { RegistryBrowserContext } from '../types';
import { REGISTRY_BROWSER_LAYOUT } from '@/design/registry-browser';

export function useRegistryHives(client: RegistryBrowserClient, context: RegistryBrowserContext) {
  return useQuery({ queryKey: ['registry-browser', 'hives', context.caseId, context.dataSourceId, context.sourceRevision],
    queryFn: () => client.listHives({ dataSourceId: context.dataSourceId, limit: REGISTRY_BROWSER_LAYOUT.pageSize }),
    retry: false, refetchOnWindowFocus: false });
}
