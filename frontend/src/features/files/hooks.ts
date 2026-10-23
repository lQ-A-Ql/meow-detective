import { keepPreviousData, useMutation, useQueries, useQuery, useQueryClient } from '@tanstack/react-query';
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
import { useCurrentCase } from '@/features/case/hooks';
import { invalidateImportProjectionQueries } from '@/features/cache-invalidation';
import { parseOffsetInput } from '@/lib/hex-offset-parser';
import { fileJumpQuery } from './hooks/file-jump-query';
import { mergeLoadedRanges } from '@/lib/hex-range-merger';
import { getPreviewSettings } from '@/lib/settings';
import { clampViewerRangeLength } from '@/lib/preview-limits';
import { errorMessage, isApiErrorDto } from '@/lib/errors';
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

export function useFileTree(showHidden = false, caseId?: string) {
  return useQuery({
    queryKey: ['files', 'tree', caseId ?? null, showHidden],
    queryFn: () => getFileTree(showHidden),
    staleTime: 10_000, // reduced from Infinity to pick up post-import tree updates
    placeholderData: keepPreviousData,
  });
}

export function useFileRows(parentId?: string, showHidden = false, caseId?: string) {
  return useQuery({
    queryKey: ['files', 'rows', caseId ?? null, parentId ?? null, showHidden],
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
  caseId?: string,
) {
  return useQuery({
    queryKey: ['files', 'rows-page', caseId ?? null, parentId ?? null, offset, limit, showHidden, sortKey, sortDirection],
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

export function useFileChildren(parentId?: string, showHidden = false, caseId?: string) {
  return useQuery({
    queryKey: ['files', 'children', caseId ?? null, parentId, showHidden],
    queryFn: () => getFileChildren(parentId!, showHidden),
    enabled: Boolean(parentId),
    staleTime: 60_000,
  });
}

export function useFileChildrenPage(
  parentId?: string,
  offset = 0,
  limit = 500,
  showHidden = false,
  caseId?: string,
) {
  return useQuery({
    queryKey: ['files', 'children-page', caseId ?? null, parentId, offset, limit, showHidden],
    queryFn: () => getFileChildrenPage(parentId!, offset, limit, showHidden),
    enabled: Boolean(parentId),
    staleTime: 60_000,
  });
}

export interface FileChildrenBranchRequest {
  parentId: string;
  offset: number;
}

/**
 * Loads the children of every expanded tree branch, not just the active directory.
 *
 * Without this, expanding a branch programmatically (a search jump, nested-evidence
 * open, or a restored expansion after a page switch) left the intermediate levels
 * without children, so the deepest directory never rendered even though it was
 * expanded and selected - the tree "lost" the path. The queries share the
 * `children-page` cache entries with {@link useFileChildrenPage}, so a branch that was
 * already fetched by clicking it is never requested twice.
 */
export function useFileChildrenBranches(
  branches: readonly FileChildrenBranchRequest[],
  limit = 500,
  showHidden = false,
  caseId?: string,
) {
  return useQueries({
    queries: branches.map((branch) => ({
      queryKey: ['files', 'children-page', caseId ?? null, branch.parentId, branch.offset, limit, showHidden],
      queryFn: () => getFileChildrenPage(branch.parentId, branch.offset, limit, showHidden),
      enabled: Boolean(branch.parentId),
      staleTime: 60_000,
    })),
  });
}

export function useImportDataSource() {
  const qc = useQueryClient();
  const currentCase = useCurrentCase();
  const jobsQueryKey = ['jobs', 'snapshot', currentCase.data?.id ?? null];
  return useMutation({
    mutationFn: (request: ImportDataSourceRequest) => importDataSource(request),
    onMutate: () => {
      expectJobsSnapshotActivity(qc.getQueryData(jobsQueryKey));
      qc.invalidateQueries({ queryKey: jobsQueryKey });
    },
    onSuccess: () => {
      expectJobsSnapshotActivity();
      invalidateImportQueries(qc);
      qc.invalidateQueries({ queryKey: jobsQueryKey });
    },
  });
}

export function useFileHandle(fileId?: string, enabled = true, caseId?: string) {
  const owner = usePreviewHandleOwner(`${fileId ?? ''}:handle`);
  const query = useQuery({
    queryKey: ['files', 'handle', caseId ?? null, fileId ?? null],
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

/**
 * Requested window length, clamped to what one IPC range request may ask for. The
 * backend clamps anything longer, and a caller that keeps the requested (not the
 * returned) length would mark unloaded bytes as loaded.
 */
function clampRequestedRangeLength(length: number) {
  return clampViewerRangeLength(length);
}

/**
 * Number of bytes a range response actually carried. Prepared preview sessions return
 * raw bytes; the legacy routed fallback returns formatted lines only, where the
 * requested length is the best available estimate.
 */
function readRangeByteLength(range: ViewerRangeResponse | undefined, requestedLength: number) {
  const rawBytes = range?.rawBytes;
  if (rawBytes) return rawBytes.length;
  return requestedLength;
}

/**
 * Base offset of the loaded window that contains `activeOffset`. Normally the window is
 * aligned to the configured chunk size, but a short read (end of file, or a truncated
 * backend response) shifts the windows, so the closest loaded offset wins.
 */
function resolveWindowBaseOffset(
  activeOffset: number,
  chunkSize: number,
  loadedChunks: Record<number, ViewerRangeResponse>,
) {
  let base = Math.max(0, Math.floor(activeOffset / chunkSize) * chunkSize);
  for (const key of Object.keys(loadedChunks)) {
    const offset = Number(key);
    if (offset <= activeOffset && offset > base) base = offset;
  }
  return base;
}

/**
 * A preview handle is a short-lived, case-scoped credential (30 minute TTL, invalidated by
 * source refreshes and BitLocker unlock). Losing it is a normal condition, not a corrupt
 * file, so the viewer re-opens the handle once instead of latching an error forever.
 */
function isStalePreviewHandleError(error: unknown) {
  if (isApiErrorDto(error)) {
    return error.code === 'PREVIEW_HANDLE_EXPIRED' || error.code === 'PREVIEW_SCOPE_CHANGED';
  }
  const message = errorMessage(error, '').toLowerCase();
  if (!message) return false;
  return (message.includes('handle') || message.includes('session'))
    && (message.includes('expired')
      || message.includes('invalid')
      || message.includes('not found')
      || message.includes('unavailable'));
}

export function useFileViewer(fileId?: string, enabled = true, caseId?: string) {
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
  // One handle re-open per file: a second stale-handle failure is surfaced instead of
  // looping between re-open and read.
  const handleReopenedRef = useRef(false);

  useEffect(() => {
    loadedFileIdRef.current = fileId;
    handleReopenedRef.current = false;
    setLoadedRanges([]);
    setLoadedChunks({});
    setActiveOffset(0);
    setJumpOffsetInput('0x0');
    setIsLoadingMore(false);
    setViewerError(undefined);
    rangeLoadInFlight.current = false;
  }, [fileId]);

  const baseQuery = useQuery({
    queryKey: ['files', 'viewer', caseId ?? null, fileId ?? null],
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
        const requestedLength = clampRequestedRangeLength(Math.max(1, initialLength));
        const range = await readFileRange({
          handleId: handle.handleId,
          offset: 0,
          length: requestedLength,
        });
        await ensurePreviewRequestActive(owner, handle.handleId, signal);
        return { handle, range, requestedLength };
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
    const { range, requestedLength } = baseQuery.data;
    const loadedLength = readRangeByteLength(range, requestedLength);
    // Merge rather than replace: a handle re-open for the same (immutable) evidence
    // produces a new response for offset 0 while later windows stay valid. Replacing
    // would drop them and re-read bytes the investigator already scrolled past.
    setLoadedChunks((current) => ({ ...current, 0: range }));
    setLoadedRanges((current) => mergeLoadedRanges(current, { start: 0, end: loadedLength }));
    setViewerError(undefined);
    handleReopenedRef.current = false;
  }, [baseQuery.data]);

  /**
   * Read a range, re-opening the preview handle once when the backend reports that the
   * cached handle expired. The subsequent refresh of `baseQuery` installs the new handle
   * for every later read.
   */
  const readRange = async (offset: number, length: number): Promise<ViewerRangeResponse> => {
    const handle = baseQuery.data?.handle;
    if (!handle) throw new Error('Preview handle is not ready');
    try {
      return await readFileRange({ handleId: handle.handleId, offset, length });
    } catch (error) {
      if (!isStalePreviewHandleError(error) || handleReopenedRef.current) throw error;
      handleReopenedRef.current = true;
      const refreshed = await baseQuery.refetch();
      const nextHandle = refreshed.data?.handle;
      if (!nextHandle) throw error;
      return readFileRange({ handleId: nextHandle.handleId, offset, length });
    }
  };

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
      const length = clampRequestedRangeLength(Math.min(
        getHexWindowSize(handle.size, preview.maxViewerRangeLength, preview.hexChunkBytes),
        Math.max(1, handle.size - alignedOffset),
      ));
      const range = await readRange(alignedOffset, length);
      if (loadedFileIdRef.current !== requestedFileId) {
        return;
      }
      const bytesRead = readRangeByteLength(range, length);
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
        mergeLoadedRanges(current, { start: alignedOffset, end: alignedOffset + bytesRead }),
      );
      setActiveOffset(clampedOffset);
    } catch (error) {
      if (loadedFileIdRef.current !== requestedFileId) {
        return;
      }
      setViewerError(errorMessage(error, 'Hex range load failed'));
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
    const baseOffset = mode === 'full'
      ? 0
      : resolveWindowBaseOffset(activeOffset, preview.hexChunkBytes, loadedChunks);
    const activeChunk = loadedChunks[baseOffset] ?? baseQuery.data.range;
    const rawBytes = getRawBytes(activeChunk);
    const lines = createHexByteWindowLines(activeChunk?.lines ?? [], rawBytes, baseOffset, handle.size);
    const isFullyLoaded = loadedRanges.some(
      (range) => range.start === 0 && range.end >= handle.size,
    );
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
export function useTextPreview(fileId?: string, enabled = true, caseId?: string) {
  return useQuery({
    queryKey: ['files', 'text', caseId ?? null, fileId],
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
export function useImagePreview(fileId?: string, enabled = true, caseId?: string) {
  return useQuery({
    queryKey: ['files', 'image', caseId ?? null, fileId],
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
export function useImageMetadata(fileId?: string, enabled = true, caseId?: string) {
  return useQuery({
    queryKey: ['files', 'image-metadata', caseId ?? null, fileId],
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
export function useDocumentPreview(fileId?: string, enabled = true, caseId?: string) {
  return useQuery({
    queryKey: ['files', 'document', caseId ?? null, fileId],
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
export function useMediaUrl(fileId?: string, enabled = true, caseId?: string) {
  const preview = useMemo(() => getPreviewSettings(), []);
  const owner = usePreviewHandleOwner(`${fileId ?? ''}:media`);
  const query = useQuery<MediaPreview | null, Error>({
    queryKey: ['files', 'media', caseId ?? null, fileId],
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
