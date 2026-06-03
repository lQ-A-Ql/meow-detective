import { useQuery } from '@tanstack/react-query';
import { getArtifactFamilies, getArtifactRows, getArtifactFamilyCounts } from '@/lib/api/artifacts';

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
