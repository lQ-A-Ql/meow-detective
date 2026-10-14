import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useNtfsTechnicalFile } from '@/features/files/hooks/use-ntfs-technical-file';
import { inspectNtfsFile } from '@/lib/api/files';
import { ntfsTechnicalFile } from '../support/ntfs-technical';

vi.mock('@/lib/api/files', () => ({ inspectNtfsFile: vi.fn() }));
const clients: QueryClient[] = [];
function wrapper() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  clients.push(client);
  return ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}
beforeEach(() => { vi.mocked(inspectNtfsFile).mockReset(); });
afterEach(() => { cleanup(); clients.splice(0).forEach((client) => client.clear()); });

describe('NTFS source-bound query', () => {
  it('requests a globally scoped MFT file without changing its ID', async () => {
    const technical = ntfsTechnicalFile();
    vi.mocked(inspectNtfsFile).mockResolvedValue(technical);
    const hook = renderHook(() => useNtfsTechnicalFile('case-a', 'ds:sample-source:mft:0:42', true), { wrapper: wrapper() });
    expect(hook.result.current.eligible).toBe(true);
    await waitFor(() => expect(hook.result.current.technical).toEqual(technical));
    expect(inspectNtfsFile).toHaveBeenCalledWith('ds:sample-source:mft:0:42');
  });

  it.each([
    ['case-a', 'ds:sample-source:inode:42', true],
    ['case-a', 'ds:sample-source:mft:0:42', false],
    [undefined, 'ds:sample-source:mft:0:42', true],
    ['case-a', 'mft:42', true],
  ] as const)('does not query ineligible selection %s / %s', (caseId, fileId, isFile) => {
    const hook = renderHook(() => useNtfsTechnicalFile(caseId, fileId, isFile), { wrapper: wrapper() });
    expect(hook.result.current.eligible).toBe(false);
    expect(hook.result.current.loading).toBe(false);
    expect(inspectNtfsFile).not.toHaveBeenCalled();
  });

  it('isolates the same file ID in different cases', async () => {
    const first = ntfsTechnicalFile();
    const second = { ...first, inode: 84 };
    vi.mocked(inspectNtfsFile).mockResolvedValueOnce(first).mockResolvedValueOnce(second);
    const hook = renderHook(({ caseId }) => useNtfsTechnicalFile(caseId, 'ds:sample-source:mft:0:42', true), {
      initialProps: { caseId: 'case-a' }, wrapper: wrapper(),
    });
    await waitFor(() => expect(hook.result.current.technical?.inode).toBe(42));
    hook.rerender({ caseId: 'case-b' });
    expect(hook.result.current.technical).toBeUndefined();
    await waitFor(() => expect(hook.result.current.technical?.inode).toBe(84));
    expect(inspectNtfsFile).toHaveBeenCalledTimes(2);
  });

  it('exposes typed Unsupported separately from read failures', async () => {
    vi.mocked(inspectNtfsFile).mockRejectedValue({ code: 'unsupported', message: 'Not an image-backed NTFS file', category: 'unsupported' });
    const hook = renderHook(() => useNtfsTechnicalFile('case-a', 'ds:sample-source:mft:42', true), { wrapper: wrapper() });
    await waitFor(() => expect(hook.result.current.unsupported).toBe(true));
    expect(hook.result.current.errorText).toBe('Not an image-backed NTFS file');
    expect(hook.result.current.technical).toBeUndefined();
  });

  it('retries transient read errors and projects a bounded run list', async () => {
    vi.mocked(inspectNtfsFile).mockRejectedValueOnce({ code: 'read', message: '读取失败', category: 'io' }).mockResolvedValueOnce(ntfsTechnicalFile(33));
    const hook = renderHook(() => useNtfsTechnicalFile('case-a', 'ds:sample-source:mft:42', true), { wrapper: wrapper() });
    await waitFor(() => expect(hook.result.current.errorText).toBe('读取失败'));
    act(() => hook.result.current.retry());
    await waitFor(() => expect(hook.result.current.technical).toBeDefined());
    expect(hook.result.current.errorText).toBeUndefined();
    expect(hook.result.current.runs.rows).toHaveLength(32);
    expect(hook.result.current.runs.total).toBe(33);
  });

  it('hides previous technical data when a refresh fails closed', async () => {
    vi.mocked(inspectNtfsFile).mockResolvedValueOnce(ntfsTechnicalFile()).mockRejectedValueOnce({ code: 'corrupt', message: '结构损坏', category: 'parser' });
    const hook = renderHook(() => useNtfsTechnicalFile('case-a', 'ds:sample-source:mft:42', true), { wrapper: wrapper() });
    await waitFor(() => expect(hook.result.current.technical).toBeDefined());
    act(() => hook.result.current.retry());
    await waitFor(() => expect(hook.result.current.errorText).toBe('结构损坏'));
    expect(hook.result.current.technical).toBeUndefined();
    expect(hook.result.current.runs.total).toBe(0);
  });
});
