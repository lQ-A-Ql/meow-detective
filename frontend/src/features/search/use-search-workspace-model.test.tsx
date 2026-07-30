import { act, renderHook } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  openHit: vi.fn(),
  search: vi.fn(),
  setSelected: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useDataSources: () => ({ data: [] }),
}));

vi.mock('@/features/search/hooks', () => ({
  useInfiniteSearchResults: (...args: unknown[]) => {
    mocks.search(...args);
    return {
      data: undefined,
      dataUpdatedAt: 0,
      fetchNextPage: vi.fn(),
      hasNextPage: false,
      isError: false,
      isFetchNextPageError: false,
      isFetchingNextPage: false,
      refetch: vi.fn(),
    };
  },
}));

vi.mock('@/features/search/use-search-page-model', () => ({
  useOpenSearchHitInFiles: () => mocks.openHit,
  useSearchSelection: () => ({
    selectedSearchHitId: undefined,
    setSelectedSearchHitId: mocks.setSelected,
  }),
}));

import { useSearchWorkspaceModel } from './use-search-workspace-model';

function wrapper(initialEntry = '/search') {
  return function SearchRouter({ children }: { children: React.ReactNode }) {
    return <MemoryRouter initialEntries={[initialEntry]}>{children}</MemoryRouter>;
  };
}

describe('useSearchWorkspaceModel', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('hydrates the initial filename query from the URL', () => {
    const { result } = renderHook(() => useSearchWorkspaceModel(), {
      wrapper: wrapper('/search?q=report.7z'),
    });

    expect(result.current.queryInput).toBe('report.7z');
    expect(result.current.activeQuery).toBe('report.7z');
    expect(mocks.search).toHaveBeenLastCalledWith(
      'report.7z',
      100,
      expect.objectContaining({ sortKey: 'name', sortDirection: 'asc' }),
    );
  });

  it('debounces and trims filename input for 180 milliseconds', () => {
    const { result } = renderHook(() => useSearchWorkspaceModel(), {
      wrapper: wrapper('/search?q=alpha'),
    });

    act(() => result.current.setQueryInput('  beta  '));
    expect(result.current.activeQuery).toBe('alpha');
    act(() => vi.advanceTimersByTime(179));
    expect(result.current.activeQuery).toBe('alpha');
    act(() => vi.advanceTimersByTime(1));
    expect(result.current.activeQuery).toBe('beta');
    expect(mocks.search).toHaveBeenLastCalledWith(
      'beta',
      100,
      expect.objectContaining({ sortKey: 'name', sortDirection: 'asc' }),
    );
  });

  it('applies file filters immediately without changing the active query', () => {
    const { result } = renderHook(() => useSearchWorkspaceModel(), {
      wrapper: wrapper('/search?q=evidence'),
    });

    act(() => result.current.setOption('entryType', 'directory'));
    expect(result.current.activeQuery).toBe('evidence');
    expect(result.current.options.entryType).toBe('directory');
    expect(mocks.search).toHaveBeenLastCalledWith(
      'evidence',
      100,
      expect.objectContaining({ entryType: 'directory' }),
    );
  });

  it('preserves extension separators while applying normalized filters', () => {
    const { result } = renderHook(() => useSearchWorkspaceModel(), {
      wrapper: wrapper('/search?q=evidence'),
    });

    act(() => result.current.setExtensionInput('.txt; log,'));

    expect(result.current.extensionInput).toBe('.txt; log,');
    expect(result.current.options.extensions).toEqual(['txt', 'log']);
  });
});
