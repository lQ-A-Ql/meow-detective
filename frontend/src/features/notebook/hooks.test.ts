import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  useCurrentCase: vi.fn(),
  listNotebookEntries: vi.fn(),
  getNotebookThread: vi.fn(),
  createNotebookEntry: vi.fn(),
  updateNotebookEntry: vi.fn(),
  addEvidenceCitation: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.useCurrentCase,
}));

vi.mock('@/lib/api/notebook', () => ({
  listNotebookEntries: mocks.listNotebookEntries,
  getNotebookThread: mocks.getNotebookThread,
  createNotebookEntry: mocks.createNotebookEntry,
  updateNotebookEntry: mocks.updateNotebookEntry,
  addEvidenceCitation: mocks.addEvidenceCitation,
}));

import {
  useCreateNotebookEntry,
  useNotebookEntries,
  useNotebookEntry,
  useUpdateNotebookEntry,
} from './hooks';

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

describe('notebook hooks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.useCurrentCase.mockReturnValue({
      isSuccess: true,
      data: { id: 'case-1', examiner: 'analyst-1' },
    });
    mocks.listNotebookEntries.mockResolvedValue([
      { entryId: 'e1', title: 'Note 1', status: 'draft' },
    ]);
    mocks.getNotebookThread.mockResolvedValue({
      entryId: 'e1',
      title: 'Note 1',
      body: 'Content',
      citations: [],
    });
    mocks.createNotebookEntry.mockResolvedValue({
      entryId: 'e2',
      title: 'New Note',
    });
    mocks.updateNotebookEntry.mockResolvedValue({
      entryId: 'e1',
      title: 'Updated',
    });
    mocks.addEvidenceCitation.mockResolvedValue({ citationId: 'c1' });
  });

  it('fetches notebook entries', async () => {
    const { result } = renderHook(() => useNotebookEntries(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.listNotebookEntries).toHaveBeenCalledTimes(1);
    expect(result.current.data).toHaveLength(1);
  });

  it('fetches notebook thread when entryId is provided', async () => {
    const { result } = renderHook(() => useNotebookEntry('e1'), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.getNotebookThread).toHaveBeenCalledWith('e1');
  });

  it('does not fetch notebook thread when entryId is undefined', () => {
    const { result } = renderHook(() => useNotebookEntry(undefined), {
      wrapper: createWrapper(),
    });

    expect(result.current.fetchStatus).toBe('idle');
    expect(mocks.getNotebookThread).not.toHaveBeenCalled();
  });

  it('creates a notebook entry with auto-populated author from current case', async () => {
    const { result } = renderHook(() => useCreateNotebookEntry(), {
      wrapper: createWrapper(),
    });

    await result.current.mutateAsync({
      title: 'New Note',
      body: 'Some content',
    } as never);

    expect(mocks.createNotebookEntry).toHaveBeenCalledWith(
      expect.objectContaining({
        title: 'New Note',
        author: 'analyst-1',
        status: 'draft',
      }),
    );
  });

  it('updates a notebook entry and invalidates related queries', async () => {
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');

    const wrapper = function Wrapper({ children }: { children: React.ReactNode }) {
      return createElement(QueryClientProvider, { client: qc }, children);
    };

    const { result } = renderHook(() => useUpdateNotebookEntry(), { wrapper });

    await result.current.mutateAsync({ entryId: 'e1', title: 'Updated' } as never);

    expect(mocks.updateNotebookEntry).toHaveBeenCalledWith({ entryId: 'e1', title: 'Updated' });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['notebook', 'entry', 'e1'] });
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['notebook', 'entries'] });
  });
});
