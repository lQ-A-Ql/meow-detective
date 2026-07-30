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

export function useSearchResults(query: string) {
  return useQuery({
    queryKey: ['search', query],
    queryFn: () => searchFiles(query),
    enabled: query.trim().length > 0,
  });
}

export function useInfiniteSearchResults(
  query: string,
  pageSize = 100,
  options: SearchRequestOptions = EMPTY_OPTIONS,
) {
  return useInfiniteQuery({
    queryKey: ['search', 'infinite', query, pageSize, options],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) => {
      const continuationPageSize = Math.min(pageSize * 5, MAX_CONTINUATION_PAGE_SIZE);
      return searchFiles(
        query,
        0,
        pageParam ? continuationPageSize : pageSize,
        pageParam ?? undefined,
        options,
      );
    },
    getNextPageParam: (lastPage) => lastPage.nextCursor,
    enabled: query.trim().length > 0,
    retry: false,
  });
}
