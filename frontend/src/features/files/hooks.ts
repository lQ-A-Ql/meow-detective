import { useQuery } from '@tanstack/react-query';
import { getFileRows, getFileTree, openFileHandle, readFileRange } from '@/lib/api/files';

export function useFileTree() {
  return useQuery({ queryKey: ['files', 'tree'], queryFn: getFileTree });
}

export function useFileRows() {
  return useQuery({ queryKey: ['files', 'rows'], queryFn: getFileRows });
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
