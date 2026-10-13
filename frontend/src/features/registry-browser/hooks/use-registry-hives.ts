import { useInfiniteQuery } from '@tanstack/react-query';
import type { RegistryBrowserClient } from '@/lib/api/registry-browser';
import type { RegistryBrowserContext } from '../types';
import { REGISTRY_BROWSER_LAYOUT } from '@/design/registry-browser';

export function useRegistryHives(client: RegistryBrowserClient, context: RegistryBrowserContext) {
  const query = useInfiniteQuery({ queryKey: ['registry-browser', 'hives', context.caseId, context.dataSourceId, context.sourceRevision],
    queryFn: ({ pageParam }) => client.listHives({ dataSourceId: context.dataSourceId, limit: REGISTRY_BROWSER_LAYOUT.pageSize, cursor: pageParam }),
    initialPageParam: undefined as string | undefined, getNextPageParam: (page) => page.nextCursor,
    retry: false, gcTime: 0, refetchOnWindowFocus: false });
  const first = query.data?.pages[0];
  return { ...query, data: first ? { ...first, hives: query.data!.pages.flatMap((page) => page.hives) } : undefined };
}
