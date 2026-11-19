import { renderHook } from '@testing-library/react';
import { MemoryRouter } from 'react-router';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { CapabilityMatrix } from '@/types/models';

const mocks = vi.hoisted(() => ({
  matrix: vi.fn<() => CapabilityMatrix | undefined>(),
  search: vi.fn(),
}));

vi.mock('@/features/capability/hooks', () => ({
  useCapabilityMatrix: () => ({ data: mocks.matrix() }),
}));

vi.mock('@/features/case/hooks', () => ({
  useDataSources: () => ({
    data: [
      { id: 'source-pending', name: 'pending', importState: 'ready', platform: 'windows' },
      { id: 'source-ready', name: 'ready', importState: 'importing', platform: 'windows' },
      { id: 'source-legacy', name: 'legacy', importState: 'ready', platform: 'windows' },
      { id: 'source-legacy-importing', name: 'legacy-importing', importState: 'importing', platform: 'windows' },
    ],
  }),
  useCurrentCase: () => ({ data: { id: 'case-a' } }),
}));

vi.mock('@/features/search/use-content-index-model', () => ({
  useContentIndexModel: () => ({ available: false }),
}));

vi.mock('@/features/search/hooks', () => ({
  MAX_LOADED_SEARCH_RESULTS: 5_000,
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
  useSearchSelection: () => ({ selectedSearchHitId: undefined, setSelectedSearchHitId: vi.fn() }),
}));

vi.mock('@/features/search/use-search-preview-model', () => ({
  useSearchPreviewModel: () => ({ open: false, openHit: vi.fn(), onOpenChange: vi.fn() }),
}));

vi.mock('@/features/files/hooks/use-file-navigation', () => ({
  useFileNavigation: () => ({ jump: vi.fn(), pending: false, errorText: undefined }),
}));

import { useSearchWorkspaceModel } from '@/features/search/use-search-workspace-model';

function wrapper() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return function SearchRouter({ children }: { children: React.ReactNode }) {
    return (
      <QueryClientProvider client={client}>
        <MemoryRouter initialEntries={['/search']}>{children}</MemoryRouter>
      </QueryClientProvider>
    );
  };
}

const row = (dataSourceId: string, state: CapabilityMatrix['sources'][number]['state']) => ({
  dataSourceId,
  capability: 'catalog' as const,
  state,
  revision: 1,
  derived: false,
  updatedAt: '2026-11-19T00:00:00Z',
});

describe('useSearchWorkspaceModel capability gating', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('excludes catalog-pending sources and admits matrix-ready sources; rows without a row fall back to legacy', () => {
    mocks.matrix.mockReturnValue({
      sources: [row('source-pending', 'pending'), row('source-ready', 'ready')],
    });
    const { result } = renderHook(() => useSearchWorkspaceModel(), { wrapper: wrapper() });
    expect(result.current.indexSources.map((s) => s.id)).toEqual(['source-ready', 'source-legacy']);
  });

  it('falls back to the legacy importState filter when the matrix is absent', () => {
    mocks.matrix.mockReturnValue(undefined);
    const { result } = renderHook(() => useSearchWorkspaceModel(), { wrapper: wrapper() });
    expect(result.current.indexSources.map((s) => s.id)).toEqual(['source-pending', 'source-legacy']);
  });
});
