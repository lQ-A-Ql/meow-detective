import { useInfiniteQuery, useQuery } from '@tanstack/react-query';
import { searchFiles } from '@/lib/api/search';

export function useSearchResults(query: string) {
  return useQuery({
    queryKey: ['search', query],
    queryFn: () => searchFiles(query),
  });
}

export function useInfiniteSearchResults(query: string, pageSize = 100) {
  return useInfiniteQuery({
    queryKey: ['search', 'infinite', query, pageSize],
    initialPageParam: null as string | null,
    queryFn: ({ pageParam }) => searchFiles(query, 0, pageSize, pageParam ?? undefined),
    getNextPageParam: (lastPage) => lastPage.nextCursor,
    retry: false,
  });
}
