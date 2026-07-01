import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import { createElement } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { Search } from './Search';

const mocks = vi.hoisted(() => ({
  searchResults: vi.fn(),
  selectionState: {
    selectedSearchHitId: undefined as string | undefined,
    setSelectedSearchHitId: vi.fn(),
    setSelectedFileId: vi.fn(),
  },
  searchParams: new URLSearchParams(),
}));

vi.mock('@/features/search/hooks', () => ({
  useSearchResults: mocks.searchResults,
}));

vi.mock('@/stores/selection-store', () => ({
  useSelectionStore: vi.fn((selector) => selector(mocks.selectionState)),
}));

vi.mock('react-router', () => ({
  useNavigate: () => vi.fn(),
  useSearchParams: () => [mocks.searchParams],
}));

vi.mock('@/lib/saved-queries', () => ({
  readSavedSearchQueries: () => [],
  writeSavedSearchQueries: vi.fn(),
  upsertSavedSearchQuery: vi.fn(),
  removeSavedSearchQuery: vi.fn(),
}));

function queryState(overrides: Record<string, unknown> = {}) {
  return {
    data: undefined,
    error: null,
    isLoading: false,
    isSuccess: true,
    refetch: vi.fn(),
    ...overrides,
  };
}

function renderPage() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    createElement(
      QueryClientProvider,
      { client: queryClient },
      createElement(Search),
    ),
  );
}

const emptySearchResult = { total: 0, tookMs: 0, items: [] };
const populatedSearchResult = {
  total: 2,
  tookMs: 15,
  items: [
    {
      fileId: 'file-1',
      path: '/evidence/docs/report.doc',
      score: 0.92,
      snippets: [{ text: 'Found sensitive data in report' }],
    },
    {
      fileId: 'file-2',
      path: '/evidence/docs/budget.xls',
      score: 0.85,
      snippets: [{ text: 'Financial records for Q4' }],
    },
  ],
};

describe('Search page', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    Object.assign(mocks.selectionState, {
      selectedSearchHitId: undefined,
      setSelectedSearchHitId: vi.fn(),
      setSelectedFileId: vi.fn(),
    });
    mocks.searchParams = new URLSearchParams();
  });

  it('renders empty state when no search results', () => {
    mocks.searchResults.mockReturnValue(queryState({ data: emptySearchResult }));

    renderPage();

    expect(screen.getByText('无搜索命中')).toBeDefined();
    expect(screen.getByText('请调整检索语句、范围或过滤条件。')).toBeDefined();
    expect(screen.getByText(/共 0 项命中/)).toBeDefined();
  });

  it('shows query input with default query', () => {
    mocks.searchResults.mockReturnValue(queryState({ data: emptySearchResult }));

    renderPage();

    const input = screen.getByRole('textbox') as HTMLInputElement;
    expect(input.value).toBe('content:password AND path:doc');
  });

  it('renders search results table when data is available', () => {
    mocks.searchResults.mockReturnValue(queryState({ data: populatedSearchResult }));

    renderPage();

    expect(screen.getAllByText('/evidence/docs/report.doc').length).toBeGreaterThan(0);
    expect(screen.getAllByText('/evidence/docs/budget.xls').length).toBeGreaterThan(0);
    expect(screen.getAllByText('0.92').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('0.85').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('Found sensitive data in report').length).toBeGreaterThan(0);
    expect(screen.getAllByText('Financial records for Q4').length).toBeGreaterThan(0);
    expect(screen.getByText(/共 2 项命中/)).toBeDefined();
    expect(screen.getByText(/高置信 2 项/)).toBeDefined();
  });

  it('uses q from initial URL as active query', () => {
    mocks.searchParams = new URLSearchParams('q=abc');
    mocks.searchResults.mockReturnValue(queryState({ data: emptySearchResult }));

    renderPage();

    expect((screen.getByRole('textbox') as HTMLInputElement).value).toBe('abc');
    expect(mocks.searchResults).toHaveBeenLastCalledWith('abc');
  });

  it('syncs active query when URL q changes on same page', async () => {
    mocks.searchParams = new URLSearchParams('q=abc');
    mocks.searchResults.mockReturnValue(queryState({ data: emptySearchResult }));
    const view = renderPage();

    mocks.searchParams = new URLSearchParams('q=def');
    view.rerender(
      createElement(
        QueryClientProvider,
        { client: new QueryClient({ defaultOptions: { queries: { retry: false } } }) },
        createElement(Search),
      ),
    );

    await waitFor(() =>
      expect((screen.getByRole('textbox') as HTMLInputElement).value).toBe('def'),
    );
    expect(mocks.searchResults).toHaveBeenLastCalledWith('def');
  });

  it('falls back to default query when URL q is empty', async () => {
    mocks.searchParams = new URLSearchParams('q=abc');
    mocks.searchResults.mockReturnValue(queryState({ data: emptySearchResult }));
    const view = renderPage();

    mocks.searchParams = new URLSearchParams('q=');
    view.rerender(
      createElement(
        QueryClientProvider,
        { client: new QueryClient({ defaultOptions: { queries: { retry: false } } }) },
        createElement(Search),
      ),
    );

    await waitFor(() =>
      expect((screen.getByRole('textbox') as HTMLInputElement).value).toBe(
        'content:password AND path:doc',
      ),
    );
    expect(mocks.searchResults).toHaveBeenLastCalledWith('content:password AND path:doc');
  });
});
