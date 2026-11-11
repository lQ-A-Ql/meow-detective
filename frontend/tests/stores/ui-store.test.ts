import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useUiStore } from '@/stores/ui-store';

async function importFreshUiStore() {
  vi.resetModules();
  return import('@/stores/ui-store');
}

describe('ui-store', () => {
  beforeEach(() => {
    localStorage.clear();
    useUiStore.setState({
      drawerOpen: false,
      viewerTab: 'hex',
      rightPanelTab: 'details',
      fileSortKey: 'name',
      fileSortDirection: 'asc',
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    localStorage.clear();
  });

  it('initial state has correct defaults', () => {
    const state = useUiStore.getState();

    expect(state.drawerOpen).toBe(false);
    expect(state.viewerTab).toBe('hex');
    expect(state.rightPanelTab).toBe('details');
  });

  it('toggleDrawer toggles drawer state', () => {
    expect(useUiStore.getState().drawerOpen).toBe(false);

    useUiStore.getState().toggleDrawer();
    expect(useUiStore.getState().drawerOpen).toBe(true);

    useUiStore.getState().toggleDrawer();
    expect(useUiStore.getState().drawerOpen).toBe(false);
  });

  it('setDrawerOpen sets the drawer explicitly', () => {
    useUiStore.getState().setDrawerOpen(true);
    expect(useUiStore.getState().drawerOpen).toBe(true);

    useUiStore.getState().setDrawerOpen(false);
    expect(useUiStore.getState().drawerOpen).toBe(false);
  });

  it('switches viewer and right-panel tabs', () => {
    useUiStore.getState().setViewerTab('metadata');
    useUiStore.getState().setRightPanelTab('trace');

    expect(useUiStore.getState().viewerTab).toBe('metadata');
    expect(useUiStore.getState().rightPanelTab).toBe('trace');
  });

  it('persists the file sort key and updates state', () => {
    useUiStore.getState().setFileSortKey('modifiedAt');

    expect(useUiStore.getState().fileSortKey).toBe('modifiedAt');
    expect(localStorage.getItem('fileSortKey')).toBe('modifiedAt');
  });

  it('toggles the sort direction and persists both directions', () => {
    useUiStore.getState().toggleFileSortDirection();

    expect(useUiStore.getState().fileSortDirection).toBe('desc');
    expect(localStorage.getItem('fileSortDirection')).toBe('desc');

    useUiStore.getState().toggleFileSortDirection();

    expect(useUiStore.getState().fileSortDirection).toBe('asc');
    expect(localStorage.getItem('fileSortDirection')).toBe('asc');
  });

  it('restores a valid persisted sort configuration at module initialization', async () => {
    localStorage.setItem('fileSortKey', 'size');
    localStorage.setItem('fileSortDirection', 'desc');

    const { useUiStore: freshStore } = await importFreshUiStore();

    expect(freshStore.getState().fileSortKey).toBe('size');
    expect(freshStore.getState().fileSortDirection).toBe('desc');
  });

  it('falls back to defaults for invalid persisted sort values', async () => {
    localStorage.setItem('fileSortKey', 'not-a-sort-key');
    localStorage.setItem('fileSortDirection', 'sideways');

    const { useUiStore: freshStore } = await importFreshUiStore();

    expect(freshStore.getState().fileSortKey).toBe('name');
    expect(freshStore.getState().fileSortDirection).toBe('asc');
  });

  it('validates each persisted value independently', async () => {
    localStorage.setItem('fileSortKey', 'ext');
    localStorage.setItem('fileSortDirection', 'sideways');

    const { useUiStore: freshStore } = await importFreshUiStore();

    expect(freshStore.getState().fileSortKey).toBe('ext');
    expect(freshStore.getState().fileSortDirection).toBe('asc');
  });

  it('falls back to defaults when reading localStorage throws', async () => {
    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new Error('storage blocked');
    });

    const { useUiStore: freshStore } = await importFreshUiStore();

    expect(freshStore.getState().fileSortKey).toBe('name');
    expect(freshStore.getState().fileSortDirection).toBe('asc');
  });

  it('keeps working when writing localStorage throws', () => {
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new Error('quota exceeded');
    });

    expect(() => useUiStore.getState().setFileSortKey('ext')).not.toThrow();
    expect(useUiStore.getState().fileSortKey).toBe('ext');

    expect(() => useUiStore.getState().toggleFileSortDirection()).not.toThrow();
    expect(useUiStore.getState().fileSortDirection).toBe('desc');
  });

  it('uses defaults when localStorage is not available at all', async () => {
    vi.stubGlobal('localStorage', undefined);

    const { useUiStore: freshStore } = await importFreshUiStore();

    expect(freshStore.getState().fileSortKey).toBe('name');
    expect(freshStore.getState().fileSortDirection).toBe('asc');

    expect(() => freshStore.getState().setFileSortKey('size')).not.toThrow();
    expect(freshStore.getState().fileSortKey).toBe('size');

    freshStore.getState().toggleFileSortDirection();
    expect(freshStore.getState().fileSortDirection).toBe('desc');
  });
});
