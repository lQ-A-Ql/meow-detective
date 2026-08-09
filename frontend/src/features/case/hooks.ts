import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
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
import { getFileTree } from '@/lib/api/files';

export function useCurrentCase() {
  return useQuery({ queryKey: ['case', 'current'], queryFn: getCurrentCase, retry: false });
}

export function useCaseMetrics() {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: ['case', 'metrics', currentCase.data?.id ?? null],
    queryFn: getCaseMetrics,
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
  });
}

export function useRecentObjects() {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: ['case', 'recent-objects', currentCase.data?.id ?? null],
    queryFn: getRecentObjects,
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
  });
}

export function useRecentCases() {
  return useQuery({ queryKey: ['case', 'recent-cases'], queryFn: getRecentCases });
}

export function useDataSources() {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: ['case', 'data-sources', currentCase.data?.id ?? null],
    queryFn: getDataSources,
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
  });
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
      // Warm the file-tree query right away: the first fetch against a cold
      // database takes seconds, and prefetching here lets the user reach the
      // file browser with the tree already cached (same key as useFileTree).
      void qc.prefetchQuery({
        queryKey: ['files', 'tree', false],
        queryFn: () => getFileTree(false),
        staleTime: 10_000,
      });
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
      qc.invalidateQueries({ queryKey: ['analysis', 'case-overview'] });
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
      toast.error(`删除案件失败: ${error.message}`);
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
      qc.invalidateQueries({ queryKey: ['analysis'] });
    },
    onError: (error: Error) => {
      toast.error(`删除数据源失败: ${error.message}`);
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
