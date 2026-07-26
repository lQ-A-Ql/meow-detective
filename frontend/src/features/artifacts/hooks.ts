import { useInfiniteQuery, useQuery } from '@tanstack/react-query';
import {
  getArtifactById,
  getArtifactFamilies,
  getArtifactRows,
  getArtifactRowsPage,
  getArtifactFamilyCounts,
} from '@/lib/api/artifacts';

export function useArtifactFamilies() {
  return useQuery({ queryKey: ['artifacts', 'families'], queryFn: getArtifactFamilies });
}

export function useArtifactRows(family: string) {
  return useQuery({
    queryKey: ['artifacts', 'rows', family],
    queryFn: () => getArtifactRows(family),
  });
}

export function useArtifactFamilyCounts() {
  return useQuery({ queryKey: ['artifacts', 'counts'], queryFn: getArtifactFamilyCounts });
}

export function useInfiniteArtifactRows(family: string, pageSize = 200) {
  return useInfiniteQuery({
    queryKey: ['artifacts', 'rows', 'infinite', family, pageSize],
    initialPageParam: undefined as string | undefined,
    queryFn: ({ pageParam }) => getArtifactRowsPage(family, pageParam, pageSize),
    getNextPageParam: (lastPage) => lastPage.nextCursor,
  });
}

export function useArtifactById(artifactId?: string) {
  return useQuery({
    queryKey: ['artifacts', 'by-id', artifactId ?? null],
    queryFn: () => getArtifactById(artifactId!),
    enabled: Boolean(artifactId),
    retry: false,
  });
}
