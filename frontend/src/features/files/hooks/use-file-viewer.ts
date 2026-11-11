import { useQuery } from '@tanstack/react-query';
import { useEffect, useMemo, useRef, useState } from 'react';
import { openFileHandle, readFileRange } from '@/lib/api/files';
import type { FileHexViewerState, HexLoadedRange, ViewerRangeResponse } from '@/types/models';
import { mergeLoadedRanges } from '@/lib/hex-range-merger';
import { getPreviewSettings } from '@/lib/settings';
import { parseOffsetInput } from '@/lib/hex-offset-parser';
import { errorMessage } from '@/lib/errors';
import {
  adoptPreviewHandle,
  ensurePreviewRequestActive,
  releasePreviewHandle,
  usePreviewHandleOwner,
} from '../previewHandleOwner';
import { PREVIEW_STALE_TIME } from './preview-constants';
import {
  MAX_RETAINED_HEX_CHUNKS,
  clampRequestedRangeLength,
  createHexByteWindowLines,
  getHexWindowSize,
  getRawBytes,
  isStalePreviewHandleError,
  readRangeByteLength,
  resolveWindowBaseOffset,
} from './viewer-range';

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
  }, [
    activeOffset,
    baseQuery.data,
    isLoadingMore,
    jumpOffsetInput,
    loadedChunks,
    loadedRanges,
    preview.hexChunkBytes,
    preview.maxViewerRangeLength,
    viewerError,
  ]);

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
