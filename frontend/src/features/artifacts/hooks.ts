import { useQuery } from '@tanstack/react-query';
import { getArtifactById, getArtifactFamilies, getArtifactRows, getArtifactFamilyCounts } from '@/lib/api/artifacts';

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

export function useArtifactById(artifactId?: string) {
  return useQuery({
    queryKey: ['artifacts', 'by-id', artifactId ?? null],
    queryFn: () => getArtifactById(artifactId!),
    enabled: Boolean(artifactId),
    retry: false,
  });
}
