import { useQuery } from '@tanstack/react-query';
import { searchFiles } from '@/lib/api/search';

export function useSearchResults(query: string) {
  return useQuery({
    queryKey: ['search', query],
    queryFn: () => searchFiles(query),
  });
}
