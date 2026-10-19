import { useInfiniteQuery, useQuery } from '@tanstack/react-query';
import { searchFiles } from '@/lib/api/search';
import type { SearchRequestOptions } from '@/types/models';

const EMPTY_OPTIONS: SearchRequestOptions = {
  matchPath: false,
  entryType: 'any',
  extensions: [],
  dataSourceIds: [],
  sortKey: 'name',
  sortDirection: 'asc',
};

const MAX_CONTINUATION_PAGE_SIZE = 500;
export const MAX_LOADED_SEARCH_RESULTS = 5_000;

export function useSearchResults(caseId: string | undefined, query: string) {
  return useQuery({
    queryKey: ['search', caseId, query],
    queryFn: async ({ signal }) => {
      const result = await searchFiles(query);
      signal.throwIfAborted();
      return result;
    },
    enabled: Boolean(caseId) && query.trim().length > 0,
  });
}

export function useInfiniteSearchResults(
  caseId: string | undefined,
  query: string,
  pageSize = 100,
  options: SearchRequestOptions = EMPTY_OPTIONS,
) {
  return useInfiniteQuery({
    queryKey: ['search', caseId, 'infinite', query, pageSize, options],
    initialPageParam: { cursor: undefined as string | undefined, limit: Math.max(1, Math.min(pageSize, MAX_CONTINUATION_PAGE_SIZE)) },
    queryFn: async ({ pageParam, signal }) => {
      const result = await searchFiles(
        query,
        0,
        pageParam.limit,
        pageParam.cursor,
        options,
      );
      signal.throwIfAborted();
      return result;
    },
    getNextPageParam: (lastPage, pages) => {
      const loaded = pages.reduce((count, page) => count + page.items.length, 0);
      if (!lastPage.nextCursor || lastPage.items.length === 0 || loaded >= MAX_LOADED_SEARCH_RESULTS) return undefined;
      return { cursor: lastPage.nextCursor, limit: Math.min(pageSize * 5, MAX_CONTINUATION_PAGE_SIZE, MAX_LOADED_SEARCH_RESULTS - loaded) };
    },
    enabled: Boolean(caseId),
    retry: false,
    staleTime: 10_000,
    gcTime: 60_000,
  });
}
