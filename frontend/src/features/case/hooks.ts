import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { createCase, closeCase, getCaseMetrics, getCurrentCase, getRecentObjects, openCase } from '@/lib/api/case';

export function useCurrentCase() {
  return useQuery({ queryKey: ['case', 'current'], queryFn: getCurrentCase });
}

export function useCaseMetrics() {
  return useQuery({ queryKey: ['case', 'metrics'], queryFn: getCaseMetrics });
}

export function useRecentObjects() {
  return useQuery({ queryKey: ['case', 'recent-objects'], queryFn: getRecentObjects });
}

export function useCreateCase() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (params: { caseRoot: string; name: string; examiner?: string }) =>
      createCase(params.caseRoot, params.name, params.examiner),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['case'] });
    },
  });
}

export function useOpenCase() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (caseRoot: string) => openCase(caseRoot),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['case'] });
    },
  });
}

export function useCloseCase() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => closeCase(),
    onSuccess: () => {
      qc.invalidateQueries();
    },
  });
}
