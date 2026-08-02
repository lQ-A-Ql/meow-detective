import { useInfiniteQuery, useQuery } from '@tanstack/react-query';
import {
  getArtifactById,
  getArtifactFamilies,
  getArtifactRows,
  getArtifactRowsPage,
  getArtifactFamilyCounts,
} from '@/lib/api/artifacts';
import { useCurrentCase } from '@/features/case/hooks';

function scopeKey(key: readonly unknown[], caseId: string | undefined) {
  return [key[0], caseId ?? null, ...key.slice(1)];
}

export function useArtifactFamilies() {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: scopeKey(['artifacts', 'families'], currentCase.data?.id),
    queryFn: getArtifactFamilies,
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
  });
}

export function useArtifactRows(family: string) {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: scopeKey(['artifacts', 'rows', family], currentCase.data?.id),
    queryFn: () => getArtifactRows(family),
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
  });
}

export function useArtifactFamilyCounts() {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: scopeKey(['artifacts', 'counts'], currentCase.data?.id),
    queryFn: getArtifactFamilyCounts,
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
  });
}

export function useInfiniteArtifactRows(family: string, pageSize = 200) {
  const currentCase = useCurrentCase();
  return useInfiniteQuery({
    queryKey: scopeKey(['artifacts', 'rows', 'infinite', family, pageSize], currentCase.data?.id),
    initialPageParam: undefined as string | undefined,
    queryFn: ({ pageParam }) => getArtifactRowsPage(family, pageParam, pageSize),
    getNextPageParam: (lastPage) => lastPage.nextCursor,
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
  });
}

export function useArtifactById(artifactId?: string) {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: scopeKey(['artifacts', 'by-id', artifactId ?? null], currentCase.data?.id),
    queryFn: () => getArtifactById(artifactId!),
    enabled: currentCase.isSuccess && Boolean(currentCase.data) && Boolean(artifactId),
    retry: false,
  });
}
