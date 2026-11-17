import type { ReactNode } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, cleanup, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useImageSteg } from '@/features/files/hooks/use-image-steg';
import { analyzeImageSteg } from '@/lib/api/files';
import type { ImageStegReport } from '@/types/models';

vi.mock('@/lib/api/files', () => ({ analyzeImageSteg: vi.fn() }));

const clients: QueryClient[] = [];
function wrapper() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  clients.push(client);
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
}

function report(overrides: Partial<ImageStegReport> = {}): ImageStegReport {
  return {
    status: 'completed',
    format: 'png',
    width: 2,
    height: 1,
    planes: [],
    blocks: [],
    blocksScored: 0,
    blocksTotal: 0,
    warnings: [],
    ...overrides,
  };
}

beforeEach(() => {
  vi.mocked(analyzeImageSteg).mockReset();
});
afterEach(() => {
  cleanup();
  clients.splice(0).forEach((client) => client.clear());
});

describe('image steganalysis session query', () => {
  it('never analyzes on mount and only runs on explicit request', async () => {
    vi.mocked(analyzeImageSteg).mockResolvedValue(report());
    const hook = renderHook(() => useImageSteg('file-a', 'case-a'), { wrapper: wrapper() });
    expect(hook.result.current.running).toBe(false);
    expect(hook.result.current.report).toBeUndefined();
    expect(analyzeImageSteg).not.toHaveBeenCalled();

    act(() => hook.result.current.run());
    await waitFor(() => expect(hook.result.current.report?.status).toBe('completed'));
    expect(analyzeImageSteg).toHaveBeenCalledTimes(1);
    expect(analyzeImageSteg).toHaveBeenCalledWith('file-a');
  });

  it('does nothing without a selected file', () => {
    const hook = renderHook(() => useImageSteg(undefined, 'case-a'), { wrapper: wrapper() });
    act(() => hook.result.current.run());
    expect(analyzeImageSteg).not.toHaveBeenCalled();
  });

  it('surfaces read failures and allows a retry', async () => {
    const failure = { code: 'IMAGE_STEG_FAILED', message: '读取失败', category: 'io', recoverable: true };
    vi.mocked(analyzeImageSteg).mockRejectedValueOnce(failure).mockResolvedValueOnce(report());
    const hook = renderHook(() => useImageSteg('file-a', 'case-a'), { wrapper: wrapper() });

    act(() => hook.result.current.run());
    await waitFor(() => expect(hook.result.current.errorText).toBe('读取失败'));
    expect(hook.result.current.report).toBeUndefined();

    act(() => hook.result.current.run());
    await waitFor(() => expect(hook.result.current.report?.status).toBe('completed'));
    expect(hook.result.current.errorText).toBeUndefined();
    expect(analyzeImageSteg).toHaveBeenCalledTimes(2);
  });

  it('drops the displayed result when another file is selected', async () => {
    vi.mocked(analyzeImageSteg).mockResolvedValueOnce(report({ dominantPlane: 'r' }));
    const hook = renderHook(({ fileId }) => useImageSteg(fileId, 'case-a'), {
      initialProps: { fileId: 'file-a' },
      wrapper: wrapper(),
    });
    act(() => hook.result.current.run());
    await waitFor(() => expect(hook.result.current.report?.dominantPlane).toBe('r'));

    hook.rerender({ fileId: 'file-b' });
    expect(hook.result.current.report).toBeUndefined();
    expect(hook.result.current.running).toBe(false);
    expect(analyzeImageSteg).toHaveBeenCalledTimes(1);
  });
});
