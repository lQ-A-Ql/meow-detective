import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useMemo, useState } from 'react';
import {
  extractFileToPath,
  getFileChildren,
  getFileChildrenPage,
  getFileJumpContext,
  getFileRows,
  getFileRowsPage,
  getFileTree,
  getTextPreview,
  getImagePreview,
  getMediaUrl,
  readMediaRange,
  importDataSource,
  openFileHandle,
  readFileRange,
} from '@/lib/api/files';
import {
  FileEntryRow,
  FileHexViewerState,
  HexByteWindowLines,
  HexLoadedRange,
  ImportDataSourceRequest,
  MediaPreview,
  ViewerRangeResponse,
} from '@/types/models';
import type { FileSortKey, FileSortDirection } from '@/lib/file-sort';
import { expectJobsSnapshotActivity } from '@/features/jobs/hooks';
import { invalidateImportProjectionQueries } from '@/features/cache-invalidation';
import { parseOffsetInput } from '@/lib/hex-offset-parser';
import { mergeLoadedRanges } from '@/lib/hex-range-merger';
import { getPreviewSettings } from '@/lib/settings';
import { saveDialog } from '@/lib/platform/dialog';
import {
  adoptPreviewHandle,
  ensurePreviewRequestActive,
  releasePreviewHandle,
  usePreviewHandleOwner,
} from './previewHandleOwner';

function getHexWindowSize(fileSize: number, maxViewerRangeLength: number, hexChunkBytes: number) {
  return fileSize <= maxViewerRangeLength ? fileSize : hexChunkBytes;
}

function getRawBytes(range?: ViewerRangeResponse): number[] {
  return range?.rawBytes ?? [];
}

function createHexByteWindowLines(
  lines: string[],
  rawBytes: number[],
  baseOffset: number,
  fileSize: number,
): HexByteWindowLines {
  const displayGateLines = lines.length > 0 || rawBytes.length === 0 ? lines : [''];
  const byteWindow: HexByteWindowLines = Object.assign([], displayGateLines);
  byteWindow.rawBytes = rawBytes;
  byteWindow.baseOffset = baseOffset;
  byteWindow.fileSize = fileSize;
  return byteWindow;
}

function invalidateImportQueries(qc: ReturnType<typeof useQueryClient>) {
  invalidateImportProjectionQueries(qc);
}

export function useFileTree(showHidden = false) {
  return useQuery({
    queryKey: ['files', 'tree', showHidden],
    queryFn: () => getFileTree(showHidden),
    staleTime: 10_000, // reduced from Infinity to pick up post-import tree updates
  });
}

export function useFileRows(parentId?: string, showHidden = false) {
  return useQuery({
    queryKey: ['files', 'rows', parentId ?? null, showHidden],
    queryFn: () => getFileRows(parentId, showHidden),
    enabled: parentId !== undefined,
  });
}

export function useFileRowsPage(
  parentId?: string,
  offset = 0,
  limit = 500,
  showHidden = false,
  sortKey: FileSortKey = 'name',
  sortDirection: FileSortDirection = 'asc',
) {
  return useQuery({
    queryKey: ['files', 'rows-page', parentId ?? null, offset, limit, showHidden, sortKey, sortDirection],
    queryFn: () => getFileRowsPage(parentId, offset, limit, showHidden, sortKey, sortDirection),
    enabled: parentId !== undefined,
  });
}

export function useFileJumpContext(
  fileId?: string,
  showHidden = false,
  pageLimit = 500,
  sortKey: FileSortKey = 'name',
  sortDirection: FileSortDirection = 'asc',
) {
  return useQuery({
    queryKey: ['files', 'jump-context', fileId ?? null, showHidden, pageLimit, sortKey, sortDirection],
    queryFn: () =>
      getFileJumpContext(fileId!, {
        showHidden,
        pageLimit,
        sortKey,
        sortDirection,
      }),
    enabled: Boolean(fileId),
    retry: false,
  });
}

export function useFileChildren(parentId?: string, showHidden = false) {
  return useQuery({
    queryKey: ['files', 'children', parentId, showHidden],
    queryFn: () => getFileChildren(parentId!, showHidden),
    enabled: Boolean(parentId),
    staleTime: 60_000,
  });
}

export function useFileChildrenPage(parentId?: string, offset = 0, limit = 500, showHidden = false) {
  return useQuery({
    queryKey: ['files', 'children-page', parentId, offset, limit, showHidden],
    queryFn: () => getFileChildrenPage(parentId!, offset, limit, showHidden),
    enabled: Boolean(parentId),
    staleTime: 60_000,
  });
}

export function useImportDataSource() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (request: ImportDataSourceRequest) => importDataSource(request),
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

export function useFileHandle(fileId?: string, enabled = true) {
  const owner = usePreviewHandleOwner(`${fileId ?? ''}:${enabled}`);
  const query = useQuery({
    queryKey: ['files', 'handle', fileId ?? null],
    enabled: Boolean(fileId) && enabled,
    retry: false,
    gcTime: 0,
    queryFn: async ({ signal }) => {
      const handle = await openFileHandle(fileId!);
      await adoptPreviewHandle(owner, handle.handleId, signal);
      return handle;
    },
  });
  return query;
}

export function useFileViewer(fileId?: string, enabled = true) {
  const preview = useMemo(() => getPreviewSettings(), []);
  const owner = usePreviewHandleOwner(`${fileId ?? ''}:${enabled}:viewer`);
  const [loadedRanges, setLoadedRanges] = useState<HexLoadedRange[]>([]);
  const [loadedChunks, setLoadedChunks] = useState<Record<number, ViewerRangeResponse>>({});
  const [activeOffset, setActiveOffset] = useState(0);
  const [jumpOffsetInput, setJumpOffsetInput] = useState('0x0');
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [viewerError, setViewerError] = useState<string>();

  useEffect(() => {
    setLoadedRanges([]);
    setLoadedChunks({});
    setActiveOffset(0);
    setJumpOffsetInput('0x0');
    setIsLoadingMore(false);
    setViewerError(undefined);
  }, [fileId]);

  const baseQuery = useQuery({
    queryKey: ['files', 'viewer', fileId ?? null],
    enabled: Boolean(fileId) && enabled,
    retry: false,
    gcTime: 0,
    queryFn: async ({ signal }) => {
      const handle = await openFileHandle(fileId!);
      await adoptPreviewHandle(owner, handle.handleId, signal);
      try {
        const initialLength = getHexWindowSize(
          handle.size,
          preview.maxViewerRangeLength,
          preview.hexChunkBytes,
        );
        const range = await readFileRange({
          handleId: handle.handleId,
          offset: 0,
          length: Math.max(1, initialLength),
        });
        await ensurePreviewRequestActive(owner, handle.handleId, signal);
        return { handle, range };
      } catch (error) {
        await releasePreviewHandle(owner, handle.handleId);
        throw error;
      }
    },
  });

  useEffect(() => {
    if (!baseQuery.data) {
      return;
    }
    const { handle, range } = baseQuery.data;
    const initialLength = getHexWindowSize(
      handle.size,
      preview.maxViewerRangeLength,
      preview.hexChunkBytes,
    );
    setLoadedRanges([{ start: 0, end: initialLength }]);
    setLoadedChunks({ 0: range });
    setViewerError(undefined);
  }, [baseQuery.data]);

  const loadRange = async (offset: number) => {
    if (!baseQuery.data || isLoadingMore) {
      return;
    }

    const { handle } = baseQuery.data;
    // Early return for empty files
    if (handle.size === 0) {
      return;
    }

    const clampedOffset = Math.min(Math.max(0, offset), Math.max(0, handle.size - 1));
    const mode = handle.size <= preview.maxViewerRangeLength ? 'full' : 'chunked';
    const alignedOffset =
      mode === 'full'
        ? 0
        : Math.max(
            0,
            Math.floor(clampedOffset / preview.hexChunkBytes) * preview.hexChunkBytes,
          );
    if (loadedChunks[alignedOffset]) {
      setActiveOffset(clampedOffset);
      return;
    }

    setIsLoadingMore(true);
    setViewerError(undefined);
    try {
      const length = Math.min(
        getHexWindowSize(handle.size, preview.maxViewerRangeLength, preview.hexChunkBytes),
        Math.max(1, handle.size - alignedOffset),
      );
      const range = await readFileRange({
        handleId: handle.handleId,
        offset: alignedOffset,
        length,
      });
      setLoadedChunks((current) => ({ ...current, [alignedOffset]: range }));
      setLoadedRanges((current) =>
        mergeLoadedRanges(current, { start: alignedOffset, end: alignedOffset + length }),
      );
      setActiveOffset(clampedOffset);
    } catch (error) {
      setViewerError(error instanceof Error ? error.message : 'Hex range load failed');
    } finally {
      setIsLoadingMore(false);
    }
  };

  const data = useMemo<FileHexViewerState | undefined>(() => {
    if (!baseQuery.data) {
      return undefined;
    }

    const { handle } = baseQuery.data;
    const mode = handle.size <= preview.maxViewerRangeLength ? 'full' : 'chunked';
    const baseOffset =
      mode === 'full'
        ? 0
        : Math.max(0, Math.floor(activeOffset / preview.hexChunkBytes) * preview.hexChunkBytes);
    const activeChunk = loadedChunks[baseOffset] ?? baseQuery.data.range;
    const rawBytes = getRawBytes(activeChunk);
    const lines = createHexByteWindowLines(activeChunk?.lines ?? [], rawBytes, baseOffset, handle.size);
    const isFullyLoaded =
      mode === 'full' || loadedRanges.some((range) => range.start === 0 && range.end >= handle.size);
    const firstRange = loadedRanges[0];
    const lastRange = loadedRanges[loadedRanges.length - 1];

    return {
      handle,
      mode,
      chunkSize: preview.hexChunkBytes,
      fileSize: handle.size,
      lines,
      rawBytes,
      baseOffset,
      loadedRanges,
      activeOffset,
      jumpOffsetInput,
      isFullyLoaded,
      isLoadingMore,
      hasMoreBefore: Boolean(firstRange && firstRange.start > 0),
      hasMoreAfter: Boolean(lastRange && lastRange.end < handle.size),
      error: viewerError,
    };
  }, [activeOffset, baseQuery.data, isLoadingMore, jumpOffsetInput, loadedChunks, loadedRanges, viewerError]);

  return {
    ...baseQuery,
    data,
    setJumpOffsetInput,
    loadRange,
    loadNextRange: () => {
      if (!data?.fileSize) {
        return Promise.resolve();
      }
      return loadRange(Math.min(data.fileSize - 1, activeOffset + preview.hexChunkBytes));
    },
    loadPreviousRange: () => loadRange(Math.max(0, activeOffset - preview.hexChunkBytes)),
    jumpToOffset: async (input?: string) => {
      const nextInput = input ?? jumpOffsetInput;
      const parsed = parseOffsetInput(nextInput);
      if (parsed === null || !data) {
        setViewerError('Invalid offset');
        return false;
      }
      if (data.fileSize === 0) {
        return true;
      }
      const clamped = Math.min(parsed, Math.max(0, data.fileSize - 1));
      setJumpOffsetInput(nextInput);
      await loadRange(clamped);
      // Note: activeOffset is set by loadRange, no need to set it here
      setViewerError(undefined);
      return true;
    },
  };
}

/**
 * Hook to get text preview for a file.
 * Returns text content with encoding detection.
 */
export function useTextPreview(fileId?: string, enabled = true) {
  return useQuery({
    queryKey: ['files', 'text', fileId],
    enabled: Boolean(fileId) && enabled,
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
export function useImagePreview(fileId?: string, enabled = true) {
  return useQuery({
    queryKey: ['files', 'image', fileId],
    enabled: Boolean(fileId) && enabled,
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
export function useMediaUrl(fileId?: string, enabled = true) {
  const preview = useMemo(() => getPreviewSettings(), []);
  const owner = usePreviewHandleOwner(`${fileId ?? ''}:${enabled}:media`);
  const query = useQuery<MediaPreview | null, Error>({
    queryKey: ['files', 'media', fileId],
    enabled: Boolean(fileId) && enabled,
    retry: false,
    gcTime: 0,
    queryFn: async ({ signal }): Promise<MediaPreview | null> => {
      if (!fileId) return null;
      const media = await getMediaUrl(fileId);
      if (media.handleId) {
        await adoptPreviewHandle(owner, media.handleId, signal);
      }
      try {
        if (media.url && media.mode === 'protocol') {
          return {
            ...media,
            previewMode: 'protocol' as const,
          };
        }
        if (media.url || !media.handleId || !media.canReadRanges) {
          return {
            ...media,
            previewMode: media.mode,
          };
        }

        const range = await readMediaRange({
          handleId: media.handleId,
          offset: 0,
          length: preview.maxViewerRangeLength,
        });
        await ensurePreviewRequestActive(owner, media.handleId, signal);
        if (!range.bytesRead) {
          return {
            ...media,
            previewMode: 'rangeFallback' as const,
            previewBytes: 0,
          };
        }
        const byteCharacters = atob(range.bytesBase64);
        const byteNumbers = new Array(byteCharacters.length);
        for (let index = 0; index < byteCharacters.length; index += 1) {
          byteNumbers[index] = byteCharacters.charCodeAt(index);
        }
        const blob = new Blob([new Uint8Array(byteNumbers)], { type: media.mimeType });
        return {
          ...media,
          mode: media.mode ?? 'rangeFallback',
          previewMode: 'rangeFallback' as const,
          previewBytes: range.bytesRead,
          url: URL.createObjectURL(blob),
        };
      } catch (error) {
        await releasePreviewHandle(owner, media.handleId);
        throw error;
      }
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
    mutationFn: async (file: FileEntryRow) => {
      const destinationPath = await saveDialog({ defaultPath: file.name || file.id });
      if (!destinationPath) {
        return 'Export cancelled';
      }
      return extractFileToPath(file, destinationPath);
    },
  });
}
