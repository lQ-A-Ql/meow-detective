import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { getFileChildren, getFileRows, getFileTree, importDataSource, openFileHandle, readFileRange } from '@/lib/api/files';
import { expectJobsSnapshotActivity } from '@/features/jobs/hooks';

const importRefreshKeys = [['case'], ['files'], ['timeline'], ['artifacts'], ['search']] as const;

function invalidateImportQueries(qc: ReturnType<typeof useQueryClient>) {
  importRefreshKeys.forEach((queryKey) => {
    qc.invalidateQueries({ queryKey });
  });
}

export function useFileTree() {
  return useQuery({
    queryKey: ['files', 'tree'],
    queryFn: getFileTree,
    refetchInterval: 1500,
    staleTime: 0,
  });
}

export function useFileRows(parentId?: string) {
  return useQuery({
    queryKey: ['files', 'rows', parentId ?? null],
    queryFn: () => getFileRows(parentId),
    enabled: parentId !== undefined,
  });
}

export function useFileChildren(parentId?: string) {
  return useQuery({
    queryKey: ['files', 'children', parentId],
    queryFn: () => getFileChildren(parentId!),
    enabled: Boolean(parentId),
  });
}

export function useImportDataSource() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (sourcePath: string) => importDataSource(sourcePath),
    onMutate: () => {
      expectJobsSnapshotActivity(qc.getQueryData(['jobs', 'snapshot']));
      qc.invalidateQueries({ queryKey: ['jobs', 'snapshot'] });
    },
    onSuccess: () => {
      expectJobsSnapshotActivity();
      invalidateImportQueries(qc);
      qc.invalidateQueries({ queryKey: ['jobs', 'snapshot'] });
    },
  });
}

export function useFileViewer(fileId?: string) {
  return useQuery({
    queryKey: ['files', 'viewer', fileId],
    enabled: Boolean(fileId),
    retry: false,
    queryFn: async () => {
      const handle = await openFileHandle(fileId!);
      const range = await readFileRange({ handleId: handle.handleId, offset: 0, length: 96 });
      return { handle, range };
    },
  });
}
