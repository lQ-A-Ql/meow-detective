import { create } from 'zustand';
import type { HexSearchMode, HexSearchPhase, HexSearchStatus } from '@/types/models';

export interface HexSearchFileState {
  mode: HexSearchMode;
  pattern: string;
  taskId?: string;
  phase?: HexSearchPhase;
  scannedBytes: number;
  totalBytes: number;
  matches: number[];
  truncated: boolean;
  activeIndex: number;
  error?: string;
}

interface HexSearchStore {
  byFile: Record<string, HexSearchFileState>;
  beginSearch: (fileId: string, mode: HexSearchMode, pattern: string) => void;
  applyStatus: (fileId: string, status: HexSearchStatus) => void;
  failSearch: (fileId: string, error: string) => void;
  markCancelling: (fileId: string) => void;
  setActiveIndex: (fileId: string, index: number) => void;
  reset: (fileId: string) => void;
}

const emptyState = (mode: HexSearchMode, pattern: string): HexSearchFileState => ({
  mode,
  pattern,
  phase: 'queued',
  scannedBytes: 0,
  totalBytes: 0,
  matches: [],
  truncated: false,
  activeIndex: -1,
});

export const useHexSearchStore = create<HexSearchStore>((set) => ({
  byFile: {},
  beginSearch(fileId, mode, pattern) {
    set((state) => ({ byFile: { ...state.byFile, [fileId]: emptyState(mode, pattern) } }));
  },
  applyStatus(fileId, status) {
    set((state) => {
      const previous = state.byFile[fileId];
      const sameTask = previous?.taskId === status.taskId;
      const matches = sameTask && previous.matches.length === status.matches.length ? previous.matches : status.matches;
      const activeIndex =
        matches.length === 0
          ? -1
          : sameTask && previous.activeIndex >= 0
            ? Math.min(previous.activeIndex, matches.length - 1)
            : 0;
      const next: HexSearchFileState = {
        mode: previous?.mode ?? 'hex',
        pattern: previous?.pattern ?? '',
        taskId: status.taskId,
        phase: status.phase,
        scannedBytes: status.scannedBytes,
        totalBytes: status.totalBytes,
        matches,
        truncated: status.truncated,
        activeIndex,
        error: status.error,
      };
      return { byFile: { ...state.byFile, [fileId]: next } };
    });
  },
  failSearch(fileId, error) {
    set((state) => {
      const previous = state.byFile[fileId];
      if (!previous) return state;
      return { byFile: { ...state.byFile, [fileId]: { ...previous, phase: 'failed', error } } };
    });
  },
  markCancelling(fileId) {
    set((state) => {
      const previous = state.byFile[fileId];
      if (!previous) return state;
      return { byFile: { ...state.byFile, [fileId]: { ...previous, phase: 'cancelling' } } };
    });
  },
  setActiveIndex(fileId, index) {
    set((state) => {
      const previous = state.byFile[fileId];
      if (!previous || previous.matches.length === 0) return state;
      const activeIndex = ((index % previous.matches.length) + previous.matches.length) % previous.matches.length;
      return { byFile: { ...state.byFile, [fileId]: { ...previous, activeIndex } } };
    });
  },
  reset(fileId) {
    set((state) => {
      if (!state.byFile[fileId]) return state;
      const byFile = { ...state.byFile };
      delete byFile[fileId];
      return { byFile };
    });
  },
}));
