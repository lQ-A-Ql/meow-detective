import { useCallback, useEffect, useRef } from 'react';
import { cancelHexSearch, getHexSearchStatus, startHexSearch } from '@/lib/api/files';
import { isApiErrorDto } from '@/lib/errors';
import { useHexSearchStore } from '@/stores/hex-search-store';
import type { HexSearchMode, HexSearchPhase } from '@/types/models';

const POLL_INTERVAL_MS = 500;

const NON_TERMINAL_PHASES = new Set<HexSearchPhase>(['queued', 'running', 'cancelling']);

export function isHexSearchActive(phase: HexSearchPhase | undefined) {
  return phase !== undefined && NON_TERMINAL_PHASES.has(phase);
}

export function hexSearchErrorKey(error: string | undefined) {
  switch (error) {
    case 'source_unavailable':
      return 'preview.hex.search.errorSourceUnavailable';
    case 'unsupported_source':
      return 'preview.hex.search.errorUnsupportedSource';
    case 'io_error':
      return 'preview.hex.search.errorIo';
    case 'hex_search_busy':
    case 'conflict':
      return 'preview.hex.search.busy';
    case 'invalid_pattern':
      return 'preview.hex.search.invalidPattern';
    default:
      return 'preview.hex.search.errorFallback';
  }
}

export function hexPatternByteLength(mode: HexSearchMode, pattern: string): number | undefined {
  if (mode === 'hex') {
    const normalized = pattern.replace(/0x/gi, '').replace(/[\s,]/g, '');
    if (!normalized || normalized.length % 2 !== 0 || !/^[0-9a-f]+$/i.test(normalized)) return undefined;
    const bytes = normalized.length / 2;
    return bytes >= 1 && bytes <= 64 ? bytes : undefined;
  }
  const bytes = new TextEncoder().encode(pattern).length;
  return bytes >= 1 && bytes <= 256 ? bytes : undefined;
}

export interface UseHexSearchOptions {
  fileId?: string;
  onJump: (offset: string) => Promise<boolean> | boolean | void;
}

export function useHexSearch({ fileId, onJump }: UseHexSearchOptions) {
  const state = useHexSearchStore((store) => (fileId ? store.byFile[fileId] : undefined));
  const beginSearch = useHexSearchStore((store) => store.beginSearch);
  const applyStatus = useHexSearchStore((store) => store.applyStatus);
  const failSearch = useHexSearchStore((store) => store.failSearch);
  const markCancelling = useHexSearchStore((store) => store.markCancelling);
  const setActiveIndex = useHexSearchStore((store) => store.setActiveIndex);
  const resetStore = useHexSearchStore((store) => store.reset);
  const inFlightRef = useRef(false);
  const onJumpRef = useRef(onJump);
  const lastJumpRef = useRef<string>();
  const active = isHexSearchActive(state?.phase);

  useEffect(() => {
    onJumpRef.current = onJump;
  }, [onJump]);

  useEffect(() => {
    if (!fileId || !active) return;
    let disposed = false;
    const poll = async () => {
      if (inFlightRef.current) return;
      inFlightRef.current = true;
      try {
        const status = await getHexSearchStatus({ fileId });
        if (!disposed && status) applyStatus(fileId, status);
      } catch {
        // Keep the last known status; a transient poll failure is not a terminal state.
      } finally {
        inFlightRef.current = false;
      }
    };
    const timer = window.setInterval(() => void poll(), POLL_INTERVAL_MS);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [active, applyStatus, fileId]);

  useEffect(() => {
    if (!fileId || !state || state.activeIndex < 0) return;
    const start = state.matches[state.activeIndex];
    if (start === undefined) return;
    const jumpKey = `${state.taskId}:${state.activeIndex}`;
    if (lastJumpRef.current === jumpKey) return;
    lastJumpRef.current = jumpKey;
    void onJumpRef.current(`0x${start.toString(16)}`);
  }, [fileId, state]);

  const start = useCallback(
    async (mode: HexSearchMode, pattern: string) => {
      if (!fileId) return;
      beginSearch(fileId, mode, pattern);
      try {
        const status = await startHexSearch({ fileId, mode, pattern });
        applyStatus(fileId, status);
      } catch (error) {
        failSearch(fileId, isApiErrorDto(error) ? error.code : 'hex_search_failed');
      }
    },
    [applyStatus, beginSearch, failSearch, fileId],
  );

  const cancel = useCallback(async () => {
    if (!fileId || !active) return;
    markCancelling(fileId);
    try {
      await cancelHexSearch({ fileId });
    } catch {
      // The poll reconciliation reports the terminal state.
    }
  }, [active, fileId, markCancelling]);

  const reset = useCallback(async () => {
    if (!fileId) return;
    if (active) {
      try {
        await cancelHexSearch({ fileId });
      } catch {
        // Best-effort cancellation before clearing the local state.
      }
    }
    resetStore(fileId);
  }, [active, fileId, resetStore]);

  const next = useCallback(() => {
    if (!fileId || !state || state.matches.length === 0) return;
    setActiveIndex(fileId, state.activeIndex + 1);
  }, [fileId, setActiveIndex, state]);

  const previous = useCallback(() => {
    if (!fileId || !state || state.matches.length === 0) return;
    setActiveIndex(fileId, state.activeIndex - 1);
  }, [fileId, setActiveIndex, state]);

  return { state, start, cancel, reset, next, previous };
}
