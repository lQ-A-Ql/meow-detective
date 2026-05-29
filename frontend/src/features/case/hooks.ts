import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  closeCase,
  createCase,
  deleteCase,
  deleteDataSource,
  getCaseMetrics,
  getCurrentCase,
  getDataSources,
  getRecentCases,
  getRecentObjects,
  openCase,
  removeCaseFromList,
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

export function useDeleteCase() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (caseRoot: string) => deleteCase(caseRoot),
    onSuccess: () => {
      qc.invalidateQueries();
    },
    onError: (error: Error) => {
      alert(`删除案件失败: ${error.message}`);
    },
  });
}

export function useDeleteDataSource() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (dataSourceId: string) => deleteDataSource(dataSourceId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['case'] });
      qc.invalidateQueries({ queryKey: ['files', 'tree'] });
      qc.invalidateQueries({ queryKey: ['files', 'rows'] });
      qc.invalidateQueries({ queryKey: ['files', 'children'] });
      qc.invalidateQueries({ queryKey: ['files', 'viewer'] });
      qc.invalidateQueries({ queryKey: ['timeline'] });
      qc.invalidateQueries({ queryKey: ['artifacts'] });
      qc.invalidateQueries({ queryKey: ['search'] });
    },
    onError: (error: Error) => {
      alert(`删除数据源失败: ${error.message}`);
    },
  });
}

export function useRemoveCaseFromList() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (caseRoot: string) => removeCaseFromList(caseRoot),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['case', 'recent-cases'] });
    },
  });
}
