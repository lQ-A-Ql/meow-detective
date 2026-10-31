import { StrictMode, type ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useEvidenceTask } from '@/features/jobs/hooks/use-evidence-task';
import { startEvidenceTask, getEvidenceTask, cancelEvidenceTask } from '@/lib/api/evidence-tasks';
import type { EvidenceTask, EvidenceTaskRequest } from '@/types/evidenceTask';

vi.mock('@/lib/api/evidence-tasks', () => ({ startEvidenceTask: vi.fn(), getEvidenceTask: vi.fn(), cancelEvidenceTask: vi.fn() }));
const request: EvidenceTaskRequest = { kind: 'digest', request: { scope: 'file', algorithm: 'sha256', fileId: 'ds:source-a:file-a' } };
const queued: EvidenceTask = { taskId: 'task-a', kind: 'digest', state: 'queued', sourceRevision: 'revision-a', bytesProcessed: 0, totalBytes: 100 };
let cache: QueryClient;
function wrapper({ children }: { children: ReactNode }) { return <StrictMode><QueryClientProvider client={cache}>{children}</QueryClientProvider></StrictMode>; }
function deferred() {
  let resolve!: (value: EvidenceTask) => void;
  const promise = new Promise<EvidenceTask>((done) => { resolve = done; });
  return { promise, resolve };
}
beforeEach(() => {
  window.localStorage.clear();
  cache = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  vi.mocked(startEvidenceTask).mockReset().mockResolvedValue(queued);
  vi.mocked(getEvidenceTask).mockReset().mockResolvedValue(queued);
  vi.mocked(cancelEvidenceTask).mockReset().mockResolvedValue({ ...queued, state: 'cancelled' });
});
afterEach(() => { cleanup(); cache.clear(); });
describe('case-bound evidence task lifecycle', () => {
  it('polls persistent byte progress and stops at completion', async () => {
    vi.mocked(getEvidenceTask).mockResolvedValueOnce({ ...queued, state: 'running', bytesProcessed: 64 });
    const hook = renderHook(() => useEvidenceTask('case-a', 'file-a'), { wrapper });
    act(() => hook.result.current.start(request));
    await waitFor(() => expect(hook.result.current.task?.bytesProcessed).toBe(64));
    vi.mocked(getEvidenceTask).mockResolvedValue({ ...queued, state: 'completed', bytesProcessed: 100 });
    await waitFor(() => expect(hook.result.current.loading).toBe(false));
    expect(hook.result.current.task?.state).toBe('completed');
    expect(startEvidenceTask).toHaveBeenCalledWith(request);
  });
  it('restores a running task after navigation and a completed result after reload', async () => {
    const hook = renderHook(() => useEvidenceTask('case-a', 'file-a'), { wrapper });
    act(() => hook.result.current.start(request));
    await waitFor(() => expect(hook.result.current.task?.taskId).toBe('task-a'));
    hook.unmount();

    vi.mocked(getEvidenceTask).mockResolvedValue({ ...queued, state: 'running', bytesProcessed: 64 });
    const navigated = renderHook(() => useEvidenceTask('case-a', 'file-a'), { wrapper });
    await waitFor(() => expect(navigated.result.current.task?.bytesProcessed).toBe(64));
    expect(navigated.result.current.request).toEqual(request);
    navigated.unmount();

    cache.clear();
    const completed: EvidenceTask = {
      ...queued,
      state: 'completed',
      bytesProcessed: 100,
      result: {
        kind: 'digest',
        result: {
          scope: 'file',
          algorithm: 'sha256',
          value: 'a'.repeat(64),
          byteLength: 100,
          status: 'completed',
        },
      },
    };
    vi.mocked(getEvidenceTask).mockResolvedValue(completed);
    const reloaded = renderHook(() => useEvidenceTask('case-a', 'file-a'), { wrapper });
    await waitFor(() => expect(reloaded.result.current.task?.state).toBe('completed'));
    expect(reloaded.result.current.task?.result).toEqual(completed.result);
  });
  it('binds a task that finishes starting after the original view was unmounted', async () => {
    const pending = deferred();
    vi.mocked(startEvidenceTask).mockReturnValue(pending.promise);
    const first = renderHook(() => useEvidenceTask('case-a', 'file-a'), { wrapper });
    act(() => first.result.current.start(request));
    await waitFor(() => expect(startEvidenceTask).toHaveBeenCalledOnce());
    first.unmount();

    const navigated = renderHook(() => useEvidenceTask('case-a', 'file-a'), { wrapper });
    await act(async () => pending.resolve(queued));
    await waitFor(() => expect(navigated.result.current.task?.taskId).toBe('task-a'));
    expect(navigated.result.current.request).toEqual(request);
  });
  it('does not start without an active case', () => {
    const hook = renderHook(() => useEvidenceTask(), { wrapper });
    act(() => hook.result.current.start(request));
    expect(startEvidenceTask).not.toHaveBeenCalled();
  });
  it.each(['case', 'selection'] as const)('rejects a late start after changing %s', async (kind) => {
    const pending = deferred();
    vi.mocked(startEvidenceTask).mockReturnValue(pending.promise);
    const hook = renderHook(({ caseId, context }) => useEvidenceTask(caseId, context), { wrapper, initialProps: { caseId: 'case-a', context: 'file-a' } });
    act(() => hook.result.current.start(request));
    await waitFor(() => expect(startEvidenceTask).toHaveBeenCalledOnce());
    hook.rerender(kind === 'case' ? { caseId: 'case-b', context: 'file-a' } : { caseId: 'case-a', context: 'file-b' });
    await act(async () => pending.resolve(queued));
    expect(hook.result.current.task).toBeUndefined();
    expect(hook.result.current.loading).toBe(false);
    expect(getEvidenceTask).not.toHaveBeenCalled();
  });
  it('rejects a late task after selection A to B to A', async () => {
    const pending = deferred();
    vi.mocked(startEvidenceTask).mockReturnValue(pending.promise);
    const hook = renderHook(({ context }) => useEvidenceTask('case-a', context), { wrapper, initialProps: { context: 'file-a' } });
    act(() => hook.result.current.start(request));
    hook.rerender({ context: 'file-b' }); hook.rerender({ context: 'file-a' });
    await act(async () => pending.resolve(queued));
    expect(hook.result.current.task).toBeUndefined();
  });
  it('keeps a late cancellation in the original case cache', async () => {
    const pending = deferred();
    vi.mocked(cancelEvidenceTask).mockReturnValue(pending.promise);
    const hook = renderHook(({ caseId }) => useEvidenceTask(caseId, 'file-a'), { wrapper, initialProps: { caseId: 'case-a' } });
    act(() => hook.result.current.start(request));
    await waitFor(() => expect(hook.result.current.task?.taskId).toBe('task-a'));
    act(() => hook.result.current.cancel());
    await waitFor(() => expect(cancelEvidenceTask).toHaveBeenCalledWith('task-a'));
    hook.rerender({ caseId: 'case-b' });
    await act(async () => pending.resolve({ ...queued, state: 'cancelled' }));
    expect(cache.getQueryData<EvidenceTask>(['evidence-task', 'case-a', 'task-a'])?.state).toBe('cancelled');
    expect(cache.getQueryData(['evidence-task', 'case-b', 'task-a'])).toBeUndefined();
    expect(hook.result.current.task).toBeUndefined();
  });
  it('retries a failed task using the same request and a new attempt', async () => {
    vi.mocked(getEvidenceTask).mockResolvedValue({ ...queued, state: 'failed', errorCode: 'interrupted' });
    const hook = renderHook(() => useEvidenceTask('case-a', 'file-a'), { wrapper });
    act(() => hook.result.current.start(request));
    await waitFor(() => expect(hook.result.current.task?.state).toBe('failed'));
    vi.mocked(startEvidenceTask).mockResolvedValue({ ...queued, taskId: 'task-b' });
    vi.mocked(getEvidenceTask).mockResolvedValue({ ...queued, taskId: 'task-b', state: 'completed' });
    act(() => hook.result.current.retry());
    await waitFor(() => expect(hook.result.current.task?.taskId).toBe('task-b'));
    expect(startEvidenceTask).toHaveBeenLastCalledWith(request);
  });
});
