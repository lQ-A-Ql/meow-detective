import { act, renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useContentIndexModel } from '@/features/search/use-content-index-model';

const mocks = vi.hoisted(() => ({ coverage: vi.fn(), start: vi.fn(), loading: false, task: undefined as { taskId: string; state: string } | undefined }));
vi.mock('@/features/jobs/hooks/use-evidence-task', () => ({ useEvidenceTask: () => ({
  task: mocks.task, loading: mocks.loading, start: mocks.start,
}) }));
vi.mock('@/lib/api/evidence-tasks', () => ({ getContentIndexCoverage: mocks.coverage }));

function setup() {
  const cache = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return { cache, wrapper: ({ children }: { children: React.ReactNode }) => <QueryClientProvider client={cache}>{children}</QueryClientProvider> };
}
describe('content indexing controls', () => {
  beforeEach(() => { vi.clearAllMocks(); mocks.loading = false; mocks.task = undefined; mocks.coverage.mockResolvedValue({ stale: false, summary: { expected: 2, indexed: 2, complete: true } }); });
  it('requires a case and source and prevents duplicate build starts while running', () => {
    const { wrapper } = setup();
    const { result, rerender } = renderHook(({ source }) => useContentIndexModel('case-a', source), {
      initialProps: { source: undefined as string | undefined }, wrapper,
    });
    act(() => result.current.build());
    expect(mocks.start).not.toHaveBeenCalled();
    rerender({ source: 'source-a' });
    act(() => result.current.build());
    expect(mocks.start).toHaveBeenCalledWith({ kind: 'contentIndex', request: { dataSourceId: 'source-a' } });
    mocks.loading = true;
    rerender({ source: 'source-a' });
    act(() => result.current.build());
    expect(mocks.start).toHaveBeenCalledTimes(1);
  });
  it('resets only the completed case search cursors and refreshes the matching source coverage', async () => {
    const { cache, wrapper } = setup();
    const reset = vi.spyOn(cache, 'resetQueries');
    const { rerender } = renderHook(() => useContentIndexModel('case-a', 'source-a'), { wrapper });
    await waitFor(() => expect(mocks.coverage).toHaveBeenCalledWith('source-a'));
    mocks.task = { taskId: 'task-a', state: 'completed' };
    rerender();
    await waitFor(() => expect(reset).toHaveBeenCalledWith({ queryKey: ['search', 'case-a'] }));
    expect(reset).not.toHaveBeenCalledWith({ queryKey: ['search'] });
  });
});
