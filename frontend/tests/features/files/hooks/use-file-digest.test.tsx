import type { ReactNode } from 'react';
import { StrictMode, useState } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useFileDigest } from '@/features/files/hooks/use-file-digest';
import { calculateEvidenceDigest } from '@/lib/api/digest';
import type { DigestAlgorithm, EvidenceDigest } from '@/types/digest';

vi.mock('@/lib/api/digest', () => ({ calculateEvidenceDigest: vi.fn() }));
const clients: QueryClient[] = [];
const target = { caseId: 'case-a', fileId: 'ds:source-a:mft:0:42', available: true };
const digest: EvidenceDigest = { scope: 'file', algorithm: 'sha256', value: 'a'.repeat(64), byteLength: 3, status: 'completed' };

function wrapper({ children }: { children: ReactNode }) {
  const [client] = useState(() => {
    const created = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
    clients.push(created);
    return created;
  });
  return <StrictMode><QueryClientProvider client={client}>{children}</QueryClientProvider></StrictMode>;
}

function deferred() {
  let resolve!: (value: EvidenceDigest) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<EvidenceDigest>((done, fail) => { resolve = done; reject = fail; });
  return { promise, resolve, reject };
}

beforeEach(() => { vi.mocked(calculateEvidenceDigest).mockReset(); });
afterEach(() => { cleanup(); clients.splice(0).forEach((client) => client.clear()); });

describe('file digest identity', () => {
  it.each<DigestAlgorithm>(['md5', 'sha1', 'sha256', 'sm3'])('keeps the %s request bound to the clicked file', async (algorithm) => {
    vi.mocked(calculateEvidenceDigest).mockResolvedValue({ ...digest, algorithm });
    const hook = renderHook((props) => useFileDigest(props), { initialProps: target, wrapper });
    act(() => hook.result.current.mutate(algorithm));
    await waitFor(() => expect(hook.result.current.data?.algorithm).toBe(algorithm));
    expect(calculateEvidenceDigest).toHaveBeenCalledWith({ scope: 'file', algorithm, fileId: target.fileId });
  });

  it('clears a completed result when a different file is selected', async () => {
    vi.mocked(calculateEvidenceDigest).mockResolvedValue(digest);
    const hook = renderHook((props) => useFileDigest(props), { initialProps: target, wrapper });
    act(() => hook.result.current.mutate('sha256'));
    await waitFor(() => expect(hook.result.current.data).toEqual(digest));
    hook.rerender({ ...target, fileId: 'ds:source-a:mft:0:43' });
    expect(hook.result.current.data).toBeUndefined();
    expect(hook.result.current.isPending).toBe(false);
  });

  it.each(['success', 'error'] as const)('does not project a late %s from file A onto B', async (outcome) => {
    const pending = deferred();
    vi.mocked(calculateEvidenceDigest).mockReturnValue(pending.promise);
    const hook = renderHook((props) => useFileDigest(props), { initialProps: target, wrapper });
    act(() => hook.result.current.mutate('sha256'));
    await waitFor(() => expect(hook.result.current.isPending).toBe(true));
    hook.rerender({ ...target, fileId: 'ds:source-a:mft:0:43' });
    await act(async () => {
      if (outcome === 'success') pending.resolve(digest);
      else pending.reject({ code: 'read', message: 'File A failed', category: 'io' });
    });
    expect(hook.result.current.data).toBeUndefined();
    expect(hook.result.current.errorText).toBeUndefined();
    expect(hook.result.current.isPending).toBe(false);
  });

  it('rejects the first selection of A after A to B to A', async () => {
    const pending = deferred();
    vi.mocked(calculateEvidenceDigest).mockReturnValue(pending.promise);
    const hook = renderHook((props) => useFileDigest(props), { initialProps: target, wrapper });
    act(() => hook.result.current.mutate('sha256'));
    hook.rerender({ ...target, fileId: 'ds:source-a:mft:0:43' });
    hook.rerender(target);
    await act(async () => pending.resolve(digest));
    expect(hook.result.current.data).toBeUndefined();
    expect(hook.result.current.isPending).toBe(false);
  });

  it('isolates identical file IDs in different cases', async () => {
    vi.mocked(calculateEvidenceDigest).mockResolvedValue(digest);
    const hook = renderHook((props) => useFileDigest(props), { initialProps: target, wrapper });
    act(() => hook.result.current.mutate('sha256'));
    await waitFor(() => expect(hook.result.current.data).toEqual(digest));
    hook.rerender({ ...target, caseId: 'case-b' });
    expect(hook.result.current.data).toBeUndefined();
  });

  it('keeps the newest attempt when older work completes last', async () => {
    const old = deferred();
    const fresh = deferred();
    vi.mocked(calculateEvidenceDigest).mockReturnValueOnce(old.promise).mockReturnValueOnce(fresh.promise);
    const hook = renderHook((props) => useFileDigest(props), { initialProps: target, wrapper });
    act(() => hook.result.current.mutate('sha256'));
    act(() => hook.result.current.mutate('sha1'));
    const latest = { ...digest, algorithm: 'sha1' as const, value: 'b'.repeat(40) };
    await act(async () => fresh.resolve(latest));
    await waitFor(() => expect(hook.result.current.data).toEqual(latest));
    await act(async () => old.resolve(digest));
    expect(hook.result.current.data).toEqual(latest);
  });

  it.each([
    { ...target, caseId: undefined }, { ...target, fileId: undefined }, { ...target, available: false },
  ])('does not call the API for an unavailable target', (props) => {
    const hook = renderHook(() => useFileDigest(props), { wrapper });
    act(() => hook.result.current.mutate('sha256'));
    expect(hook.result.current.available).toBe(false);
    expect(calculateEvidenceDigest).not.toHaveBeenCalled();
  });

  it('uses the shared typed error adapter and clears errors on selection changes', async () => {
    vi.mocked(calculateEvidenceDigest).mockRejectedValue({ code: 'read', message: '读取失败', category: 'io' });
    const hook = renderHook((props) => useFileDigest(props), { initialProps: target, wrapper });
    act(() => hook.result.current.mutate('sha256'));
    await waitFor(() => expect(hook.result.current.errorText).toBe('读取失败'));
    hook.rerender({ ...target, fileId: 'ds:source-a:mft:0:43' });
    expect(hook.result.current.errorText).toBeUndefined();
  });
});
