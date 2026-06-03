import { beforeEach, describe, expect, it } from 'vitest';
import { useUiStore } from './ui-store';

describe('ui-store', () => {
  beforeEach(() => {
    useUiStore.setState({
      currentPage: 'home',
      drawerOpen: false,
      viewerTab: 'hex',
      rightPanelTab: 'details',
      globalSearchQuery: '',
      fileSortKey: 'name',
      fileSortDirection: 'asc',
    });
  });

  it('initial state has correct defaults', () => {
    const state = useUiStore.getState();

    expect(state.currentPage).toBe('home');
    expect(state.drawerOpen).toBe(false);
    expect(state.globalSearchQuery).toBe('');
  });

  it('setCurrentPage updates state', () => {
    useUiStore.getState().setCurrentPage('search');

    expect(useUiStore.getState().currentPage).toBe('search');
  });

  it('toggleDrawer toggles drawer state', () => {
    expect(useUiStore.getState().drawerOpen).toBe(false);

    useUiStore.getState().toggleDrawer();
    expect(useUiStore.getState().drawerOpen).toBe(true);

    useUiStore.getState().toggleDrawer();
    expect(useUiStore.getState().drawerOpen).toBe(false);
  });

  it('setGlobalSearchQuery updates query', () => {
    useUiStore.getState().setGlobalSearchQuery('test query');

    expect(useUiStore.getState().globalSearchQuery).toBe('test query');
  });
});
