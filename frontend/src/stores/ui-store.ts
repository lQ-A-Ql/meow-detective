import { create } from 'zustand';

export type PageKey = 'home' | 'files' | 'analysis' | 'v2' | 'v3' | 'graph' | 'search' | 'timeline' | 'artifacts' | 'reports';
type ViewerTabKey = 'metadata' | 'text' | 'hex' | 'preview';
type FileSortKey = 'name' | 'size' | 'modifiedAt' | 'ext';
type FileSortDirection = 'asc' | 'desc';

// 验证辅助函数
const VALID_SORT_KEYS: FileSortKey[] = ['name', 'size', 'modifiedAt', 'ext'];
const VALID_SORT_DIRECTIONS: FileSortDirection[] = ['asc', 'desc'];

function isValidSortKey(key: string | null): key is FileSortKey {
  return key !== null && VALID_SORT_KEYS.includes(key as FileSortKey);
}

function isValidSortDirection(dir: string | null): dir is FileSortDirection {
  return dir !== null && VALID_SORT_DIRECTIONS.includes(dir as FileSortDirection);
}

function readStorageValue(key: string): string | null {
  try {
    const storage = globalThis.localStorage;
    return typeof storage?.getItem === 'function' ? storage.getItem(key) : null;
  } catch {
    return null;
  }
}

function writeStorageValue(key: string, value: string): void {
  try {
    const storage = globalThis.localStorage;
    if (typeof storage?.setItem === 'function') {
      storage.setItem(key, value);
    }
  } catch {
    // Ignore storage failures in test, SSR, or restricted browser contexts.
  }
}

function getSavedSortKey(): FileSortKey {
  const saved = readStorageValue('fileSortKey');
  return isValidSortKey(saved) ? saved : 'name';
}

function getSavedSortDirection(): FileSortDirection {
  const saved = readStorageValue('fileSortDirection');
  return isValidSortDirection(saved) ? saved : 'asc';
}

type UiState = {
  currentPage: PageKey;
  drawerOpen: boolean;
  viewerTab: ViewerTabKey;
  rightPanelTab: 'details' | 'trace';
  globalSearchQuery: string;
  setCurrentPage: (page: PageKey) => void;
  setDrawerOpen: (open: boolean) => void;
  toggleDrawer: () => void;
  setViewerTab: (tab: ViewerTabKey) => void;
  setRightPanelTab: (tab: 'details' | 'trace') => void;
  setGlobalSearchQuery: (query: string) => void;

  // 文件列表排序
  fileSortKey: FileSortKey;
  fileSortDirection: FileSortDirection;
  setFileSortKey: (key: FileSortKey) => void;
  toggleFileSortDirection: () => void;
};

export const useUiStore = create<UiState>((set) => ({
  currentPage: 'home',
  drawerOpen: false,
  viewerTab: 'hex',
  rightPanelTab: 'details',
  globalSearchQuery: '',
  setCurrentPage: (page) => set({ currentPage: page }),
  setDrawerOpen: (open) => set({ drawerOpen: open }),
  toggleDrawer: () => set((state) => ({ drawerOpen: !state.drawerOpen })),
  setViewerTab: (tab) => set({ viewerTab: tab }),
  setRightPanelTab: (tab) => set({ rightPanelTab: tab }),
  setGlobalSearchQuery: (query) => set({ globalSearchQuery: query }),

  // 文件列表排序 (使用安全的类型验证)
  fileSortKey: getSavedSortKey(),
  fileSortDirection: getSavedSortDirection(),
  setFileSortKey: (key) => {
    writeStorageValue('fileSortKey', key);
    set({ fileSortKey: key });
  },
  toggleFileSortDirection: () => {
    set((state) => {
      const newDirection = state.fileSortDirection === 'asc' ? 'desc' : 'asc';
      writeStorageValue('fileSortDirection', newDirection);
      return { fileSortDirection: newDirection };
    });
  },
}));
