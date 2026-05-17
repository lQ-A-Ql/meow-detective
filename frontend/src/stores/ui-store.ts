import { create } from 'zustand';

type PageKey = 'home' | 'files' | 'search' | 'timeline' | 'artifacts' | 'reports';
type ViewerTabKey = 'metadata' | 'text' | 'hex' | 'preview';

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
};

export const useUiStore = create<UiState>((set) => ({
  currentPage: 'home',
  drawerOpen: false,
  viewerTab: 'hex',
  rightPanelTab: 'details',
  globalSearchQuery: 'credential OR wallet OR exfil',
  setCurrentPage: (page) => set({ currentPage: page }),
  setDrawerOpen: (open) => set({ drawerOpen: open }),
  toggleDrawer: () => set((state) => ({ drawerOpen: !state.drawerOpen })),
  setViewerTab: (tab) => set({ viewerTab: tab }),
  setRightPanelTab: (tab) => set({ rightPanelTab: tab }),
  setGlobalSearchQuery: (query) => set({ globalSearchQuery: query }),
}));
