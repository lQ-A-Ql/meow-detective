import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  createCase,
  closeCase,
  getCaseMetrics,
  getCurrentCase,
  getDataSources,
  getRecentCases,
  getRecentObjects,
  openCase,
  renameDataSource,
} from '@/lib/api/case';

export function useCurrentCase() {
  return useQuery({ queryKey: ['case', 'current'], queryFn: getCurrentCase, retry: false });
}

export function useCaseMetrics() {
  return useQuery({ queryKey: ['case', 'metrics'], queryFn: getCaseMetrics });
}

export function useRecentObjects() {
  return useQuery({ queryKey: ['case', 'recent-objects'], queryFn: getRecentObjects });
}

export function useRecentCases() {
  return useQuery({ queryKey: ['case', 'recent-cases'], queryFn: getRecentCases });
}

export function useDataSources() {
  return useQuery({ queryKey: ['case', 'data-sources'], queryFn: getDataSources });
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
      qc.invalidateQueries({ queryKey: ['files'] });
      qc.invalidateQueries({ queryKey: ['jobs'] });
    },
  });
}

export function useRenameDataSource() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (params: { dataSourceId: string; name: string }) =>
      renameDataSource(params.dataSourceId, params.name),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['case', 'data-sources'] });
      qc.invalidateQueries({ queryKey: ['files'] });
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
