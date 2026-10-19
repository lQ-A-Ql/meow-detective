import { keepPreviousData, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useMemo, useRef, useState } from 'react';
import {
  getFileChildren,
  getFileChildrenPage,
  getFileRows,
  getFileRowsPage,
  getFileTree,
  getTextPreview,
  getImagePreview,
  getImageMetadata,
  getDocumentPreview,
  getMediaUrl,
  readMediaRange,
  importDataSource,
  openFileHandle,
  readFileRange,
} from '@/lib/api/files';
export { useFileDigest } from './hooks/use-file-digest';
export { useNtfsTechnicalFile } from './hooks/use-ntfs-technical-file';
import {
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
import { fileJumpQuery } from './hooks/file-jump-query';
import { mergeLoadedRanges } from '@/lib/hex-range-merger';
import { getPreviewSettings } from '@/lib/settings';
import {
  adoptPreviewHandle,
  ensurePreviewRequestActive,
  releasePreviewHandle,
  usePreviewHandleOwner,
} from './previewHandleOwner';

// Evidence sources are read-only for the lifetime of an active case. Keep an
// observed preview result stable while its tab is hidden so tab changes do not
// reopen handles or schedule duplicate Tauri reads.
const PREVIEW_STALE_TIME = Infinity;

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
    placeholderData: keepPreviousData,
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
  caseId?: string,
) {
  return useQuery(fileJumpQuery(caseId, fileId, showHidden, pageLimit, sortKey, sortDirection));
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
  const owner = usePreviewHandleOwner(`${fileId ?? ''}:handle`);
  const query = useQuery({
    queryKey: ['files', 'handle', fileId ?? null],
    enabled: Boolean(fileId) && enabled,
    retry: false,
    staleTime: PREVIEW_STALE_TIME,
    gcTime: 0,
    queryFn: async ({ signal }) => {
      const handle = await openFileHandle(fileId!);
      await adoptPreviewHandle(owner, handle.handleId, signal);
      return handle;
    },
  });
  return query;
}

/**
 * Chunks retained per open hex viewer. Each entry keeps 64 KiB of raw bytes plus its
 * formatted lines, and the hook's state survives preview-tab switches, so the map needs an
 * explicit bound (chunks furthest from the viewed offset are evicted first).
 */
const MAX_RETAINED_HEX_CHUNKS = 8;

export function useFileViewer(fileId?: string, enabled = true) {
  const preview = useMemo(() => getPreviewSettings(), []);
  const owner = usePreviewHandleOwner(`${fileId ?? ''}:viewer`);
  const [loadedRanges, setLoadedRanges] = useState<HexLoadedRange[]>([]);
  const [loadedChunks, setLoadedChunks] = useState<Record<number, ViewerRangeResponse>>({});
  const [activeOffset, setActiveOffset] = useState(0);
  const [jumpOffsetInput, setJumpOffsetInput] = useState('0x0');
  const [isLoadingMore, setIsLoadingMore] = useState(false);
  const [viewerError, setViewerError] = useState<string>();
  const rangeLoadInFlight = useRef(false);
  // Identifies the file that the in-flight chunk read belongs to. A read that resolves
  // after the investigator switched files must never write into the new file's state.
  const loadedFileIdRef = useRef(fileId);

  useEffect(() => {
    loadedFileIdRef.current = fileId;
    setLoadedRanges([]);
    setLoadedChunks({});
    setActiveOffset(0);
    setJumpOffsetInput('0x0');
    setIsLoadingMore(false);
    setViewerError(undefined);
    rangeLoadInFlight.current = false;
  }, [fileId]);

  const baseQuery = useQuery({
    queryKey: ['files', 'viewer', fileId ?? null],
    enabled: Boolean(fileId) && enabled,
    retry: false,
    staleTime: PREVIEW_STALE_TIME,
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
    if (!baseQuery.data || rangeLoadInFlight.current) {
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

    const requestedFileId = fileId;
    rangeLoadInFlight.current = true;
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
      if (loadedFileIdRef.current !== requestedFileId) {
        return;
      }
      setLoadedChunks((current) => {
        const next = { ...current, [alignedOffset]: range };
        const offsets = Object.keys(next).map(Number);
        if (offsets.length <= MAX_RETAINED_HEX_CHUNKS) {
          return next;
        }
        const retained = new Set(
          offsets
            .sort((a, b) => Math.abs(a - alignedOffset) - Math.abs(b - alignedOffset))
            .slice(0, MAX_RETAINED_HEX_CHUNKS),
        );
        return Object.fromEntries(
          offsets.filter((offset) => retained.has(offset)).map((offset) => [offset, next[offset]]),
        ) as Record<number, ViewerRangeResponse>;
      });
      setLoadedRanges((current) =>
        mergeLoadedRanges(current, { start: alignedOffset, end: alignedOffset + length }),
      );
      setActiveOffset(clampedOffset);
    } catch (error) {
      if (loadedFileIdRef.current !== requestedFileId) {
        return;
      }
      setViewerError(error instanceof Error ? error.message : 'Hex range load failed');
    } finally {
      if (loadedFileIdRef.current === requestedFileId) {
        rangeLoadInFlight.current = false;
        setIsLoadingMore(false);
      }
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
    staleTime: PREVIEW_STALE_TIME,
    gcTime: 60_000,
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
    staleTime: PREVIEW_STALE_TIME,
    gcTime: 60_000,
    queryFn: async () => {
      if (!fileId) return null;
      return await getImagePreview(fileId);
    },
  });
}

/** Load bounded EXIF/GPS metadata through the source-bound backend reader. */
export function useImageMetadata(fileId?: string, enabled = true) {
  return useQuery({
    queryKey: ['files', 'image-metadata', fileId],
    enabled: Boolean(fileId) && enabled,
    retry: false,
    staleTime: PREVIEW_STALE_TIME,
    gcTime: 60_000,
    queryFn: async () => {
      if (!fileId) return null;
      return await getImageMetadata(fileId);
    },
  });
}

/**
 * Hook for structured document preview (PDF, Office Open XML, SQLite).
 */
export function useDocumentPreview(fileId?: string, enabled = true) {
  return useQuery({
    queryKey: ['files', 'document', fileId],
    enabled: Boolean(fileId) && enabled,
    retry: false,
    staleTime: PREVIEW_STALE_TIME,
    gcTime: 60_000,
    queryFn: async () => {
      if (!fileId) return null;
      return await getDocumentPreview(fileId);
    },
  });
}

/**
 * Hook to get media URL for video/audio playback.
 * Returns an inline data URL for bounded media previews.
 */
export function useMediaUrl(fileId?: string, enabled = true) {
  const preview = useMemo(() => getPreviewSettings(), []);
  const owner = usePreviewHandleOwner(`${fileId ?? ''}:media`);
  const query = useQuery<MediaPreview | null, Error>({
    queryKey: ['files', 'media', fileId],
    enabled: Boolean(fileId) && enabled,
    retry: false,
    staleTime: PREVIEW_STALE_TIME,
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
        const objectUrl = URL.createObjectURL(blob);
        if (signal.aborted) {
          // React Query discards an aborted result, so the revoke effect keyed on the query
          // data would never run and the object URL would leak for the whole session.
          URL.revokeObjectURL(objectUrl);
          throw new DOMException('Media preview was aborted', 'AbortError');
        }
        return {
          ...media,
          mode: media.mode ?? 'rangeFallback',
          previewMode: 'rangeFallback' as const,
          previewBytes: range.bytesRead,
          url: objectUrl,
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
