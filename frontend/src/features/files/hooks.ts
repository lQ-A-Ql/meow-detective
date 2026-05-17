import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { getFileChildren, getFileRows, getFileTree, importDataSource, openFileHandle, readFileRange } from '@/lib/api/files';

export function useFileTree() {
  return useQuery({ queryKey: ['files', 'tree'], queryFn: getFileTree });
}

export function useFileRows() {
  return useQuery({ queryKey: ['files', 'rows'], queryFn: getFileRows });
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
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['files'] });
      qc.invalidateQueries({ queryKey: ['case'] });
      qc.invalidateQueries({ queryKey: ['timeline'] });
    },
  });
}

export function useFileViewer(fileId?: string) {
  return useQuery({
    queryKey: ['files', 'viewer', fileId],
    enabled: Boolean(fileId),
    queryFn: async () => {
      const handle = await openFileHandle(fileId!);
      const range = await readFileRange({ handleId: handle.handleId, offset: 0, length: 96 });
      return { handle, range };
    },
  });
}
