import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect } from 'react';
import {
  extractFile,
  getFileChildren,
  getFileRows,
  getFileTree,
  getTextPreview,
  getImagePreview,
  getMediaUrl,
  readMediaRange,
  importDataSource,
  openFileHandle,
  readFileRange,
} from '@/lib/api/files';
import { FileEntryRow } from '@/types/models';
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
    staleTime: Infinity,
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
    staleTime: 60_000,
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

/**
 * Hook to get text preview for a file.
 * Returns text content with encoding detection.
 */
export function useTextPreview(fileId?: string) {
  return useQuery({
    queryKey: ['files', 'text', fileId],
    enabled: Boolean(fileId),
    retry: false,
    queryFn: async () => {
      if (!fileId) return null;
      return await getTextPreview(fileId, 1024 * 1024); // 1MB limit
    },
  });
}

/**
 * Hook to get image preview for a file.
 * Returns an inline data URL for bounded image previews.
 */
export function useImagePreview(fileId?: string) {
  return useQuery({
    queryKey: ['files', 'image', fileId],
    enabled: Boolean(fileId),
    retry: false,
    queryFn: async () => {
      if (!fileId) return null;
      return await getImagePreview(fileId);
    },
  });
}

/**
 * Hook to get media URL for video/audio playback.
 * Returns an inline data URL for bounded media previews.
 */
export function useMediaUrl(fileId?: string) {
  const query = useQuery({
    queryKey: ['files', 'media', fileId],
    enabled: Boolean(fileId),
    retry: false,
    queryFn: async () => {
      if (!fileId) return null;
      const media = await getMediaUrl(fileId);
      if (media.url || !media.handleId || !media.canReadRanges) {
        return media;
      }

      const range = await readMediaRange({
        handleId: media.handleId,
        offset: 0,
        length: Math.min(media.size, 1024 * 1024),
      });
      const byteCharacters = atob(range.bytesBase64);
      const byteNumbers = new Array(byteCharacters.length);
      for (let index = 0; index < byteCharacters.length; index += 1) {
        byteNumbers[index] = byteCharacters.charCodeAt(index);
      }
      const blob = new Blob([new Uint8Array(byteNumbers)], { type: media.mimeType });
      return {
        ...media,
        url: URL.createObjectURL(blob),
      };
    },
  });

  const blobUrl = query.data?.url?.startsWith('blob:') ? query.data.url : undefined;
  useEffect(() => {
    return () => {
      if (blobUrl) {
        URL.revokeObjectURL(blobUrl);
      }
    };
  }, [blobUrl]);

  return query;
}

export function useExtractFile() {
  return useMutation({
    mutationFn: (file: FileEntryRow) => extractFile(file),
  });
}
