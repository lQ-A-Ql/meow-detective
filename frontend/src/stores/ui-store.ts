import { create } from 'zustand';

type PageKey = 'home' | 'files' | 'search' | 'timeline' | 'artifacts' | 'reports';
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

function getSavedSortKey(): FileSortKey {
  const saved = localStorage.getItem('fileSortKey');
  return isValidSortKey(saved) ? saved : 'name';
}

function getSavedSortDirection(): FileSortDirection {
  const saved = localStorage.getItem('fileSortDirection');
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
    localStorage.setItem('fileSortKey', key);
    set({ fileSortKey: key });
  },
  toggleFileSortDirection: () => {
    set((state) => {
      const newDirection = state.fileSortDirection === 'asc' ? 'desc' : 'asc';
      localStorage.setItem('fileSortDirection', newDirection);
      return { fileSortDirection: newDirection };
    });
  },
}));
