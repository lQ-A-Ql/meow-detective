import { beforeEach, describe, expect, it } from 'vitest';
import { useUiStore } from '@/stores/ui-store';

describe('ui-store', () => {
  beforeEach(() => {
    useUiStore.setState({
      drawerOpen: false,
      viewerTab: 'hex',
      rightPanelTab: 'details',
      fileSortKey: 'name',
      fileSortDirection: 'asc',
    });
  });

  it('initial state has correct defaults', () => {
    const state = useUiStore.getState();

    expect(state.drawerOpen).toBe(false);
    expect(state.viewerTab).toBe('hex');
  });

  it('toggleDrawer toggles drawer state', () => {
    expect(useUiStore.getState().drawerOpen).toBe(false);

    useUiStore.getState().toggleDrawer();
    expect(useUiStore.getState().drawerOpen).toBe(true);

    useUiStore.getState().toggleDrawer();
    expect(useUiStore.getState().drawerOpen).toBe(false);
  });
});
