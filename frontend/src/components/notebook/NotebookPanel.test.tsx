import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { NotebookPanel } from './NotebookPanel';

const mocks = vi.hoisted(() => ({
  currentCase: vi.fn(),
  notebookEntries: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.currentCase,
}));

vi.mock('@/features/notebook/hooks', () => ({
  useNotebookEntries: mocks.notebookEntries,
  useNotebookEntry: vi.fn().mockReturnValue({ data: undefined, isLoading: false, isError: false }),
  useCreateNotebookEntry: vi.fn().mockReturnValue({ mutate: vi.fn(), isPending: false, isError: false }),
  useUpdateNotebookEntry: vi.fn().mockReturnValue({ mutate: vi.fn(), isPending: false }),
  useAddEvidenceCitation: vi.fn().mockReturnValue({ mutate: vi.fn() }),
}));

vi.mock('@/features/graph/hooks', () => ({
  useGraphSnapshot: vi.fn().mockReturnValue({ data: undefined }),
}));

function renderPanel() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    createElement(
      QueryClientProvider,
      { client: queryClient },
      createElement(NotebookPanel),
    ),
  );
}

describe('NotebookPanel', () => {
  it('shows loading state when case is loading', () => {
    mocks.currentCase.mockReturnValue({
      data: undefined,
      isLoading: true,
      isError: false,
    });
    mocks.notebookEntries.mockReturnValue({
      data: [],
      isLoading: false,
      isError: false,
      refetch: vi.fn(),
    });

    renderPanel();
    expect(screen.getByText('正在加载案件...')).toBeDefined();
  });

  it('shows empty state when no case is active', () => {
    mocks.currentCase.mockReturnValue({
      data: null,
      isLoading: false,
      isError: false,
    });
    mocks.notebookEntries.mockReturnValue({
      data: [],
      isLoading: false,
      isError: false,
      refetch: vi.fn(),
    });

    renderPanel();
    expect(screen.getByText('请先打开或创建一个案件')).toBeDefined();
  });

  it('shows empty notebook state when entries are empty', () => {
    mocks.currentCase.mockReturnValue({
      data: { id: 'case-1', name: 'Test Case' },
      isLoading: false,
      isError: false,
    });
    mocks.notebookEntries.mockReturnValue({
      data: [],
      isLoading: false,
      isError: false,
      refetch: vi.fn(),
    });

    renderPanel();
    expect(screen.getByText('笔记面板')).toBeDefined();
    expect(screen.getByText('暂无笔记')).toBeDefined();
  });

  it('renders entry list when entries exist', () => {
    mocks.currentCase.mockReturnValue({
      data: { id: 'case-1', name: 'Test Case' },
      isLoading: false,
      isError: false,
    });
    mocks.notebookEntries.mockReturnValue({
      data: [
        {
          id: 'entry-1',
          parentId: null,
          title: 'First observation',
          entryType: 'observation',
          status: 'draft',
          tags: [],
          replyCount: 0,
          createdAt: '2026-06-01T10:00:00Z',
          updatedAt: '2026-06-01T10:00:00Z',
        },
        {
          id: 'entry-2',
          parentId: null,
          title: 'Key finding',
          entryType: 'finding',
          status: 'reviewed',
          tags: ['important'],
          replyCount: 0,
          createdAt: '2026-06-02T10:00:00Z',
          updatedAt: '2026-06-02T10:00:00Z',
        },
      ],
      isLoading: false,
      isError: false,
      refetch: vi.fn(),
    });

    renderPanel();
    expect(screen.getByText('First observation')).toBeDefined();
    expect(screen.getByText('Key finding')).toBeDefined();
    expect(screen.getByText('总计: 2')).toBeDefined();
  });
});
