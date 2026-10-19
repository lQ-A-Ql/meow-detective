import { act, renderHook, waitFor } from '@testing-library/react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useFileNavigation } from '@/features/files/hooks/use-file-navigation';
import { fileJumpQuery } from '@/features/files/hooks/file-jump-query';
import { resetSearchContext } from '@/features/case/hooks/reset-search-context';
import { useSelectionStore } from '@/stores/selection-store';
import { useUiStore } from '@/stores/ui-store';
import type { FileJumpContext } from '@/types/models';

const mocks = vi.hoisted(() => ({ resolve: vi.fn(), navigate: vi.fn() }));
vi.mock('@/lib/api/files', () => ({ getFileJumpContext: mocks.resolve }));
vi.mock('react-router', () => ({ useNavigate: () => mocks.navigate }));

function context(id: string): FileJumpContext {
  const target = { id, name: 'hidden.txt', path: '[P1]/hidden/hidden.txt', entryType: 'file' as const,
    hidden: true, system: false, deleted: false, readOnly: false, archive: false };
  return { target, directory: { ...target, id: 'parent', entryType: 'directory' },
    ancestorDirectoryIds: ['root', 'parent'], rowOffset: 1_000, requiresShowHidden: true };
}
function setup() {
  const cache = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  cache.setQueryData(['case', 'current'], { id: 'case-a' });
  const wrapper = ({ children }: { children: React.ReactNode }) => <QueryClientProvider client={cache}>{children}</QueryClientProvider>;
  return { cache, wrapper };
}

describe('source-owned file navigation', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useSelectionStore.setState({ selectedDirectoryId: undefined, selectedFileId: undefined });
    useUiStore.setState({ fileSortKey: 'name', fileSortDirection: 'asc' });
  });

  it('resolves and caches off-page hidden context before navigating with an atomic selection', async () => {
    const target = context('ds:source-a:mft:1:42');
    mocks.resolve.mockResolvedValue(target);
    const { cache, wrapper } = setup();
    const { result } = renderHook(() => useFileNavigation('case-a'), { wrapper });
    act(() => result.current.jump(target.target.id));
    await waitFor(() => expect(mocks.navigate).toHaveBeenCalledWith('/files'));
    expect(mocks.resolve).toHaveBeenCalledWith(target.target.id, { showHidden: true, pageLimit: 500, sortKey: 'name', sortDirection: 'asc' });
    expect(cache.getQueryData(fileJumpQuery('case-a', target.target.id, true).queryKey)).toEqual(target);
    expect(useSelectionStore.getState()).toMatchObject({ selectedDirectoryId: 'parent', selectedFileId: target.target.id });
  });

  it('keeps the current browser selection on resolution errors and permits a retry', async () => {
    mocks.resolve.mockRejectedValueOnce(new Error('source unavailable')).mockResolvedValueOnce(context('file-a'));
    const { wrapper } = setup();
    const { result } = renderHook(() => useFileNavigation('case-a'), { wrapper });
    act(() => result.current.jump('file-a'));
    await waitFor(() => expect(result.current.errorText).toBe('source unavailable'));
    expect(mocks.navigate).not.toHaveBeenCalled();
    expect(useSelectionStore.getState().selectedFileId).toBeUndefined();
    act(() => result.current.jump('file-a'));
    await waitFor(() => expect(mocks.navigate).toHaveBeenCalledTimes(1));
  });

  it('discards a late result even after switching away from and back to the same case', async () => {
    let finish!: (value: FileJumpContext) => void;
    mocks.resolve.mockReturnValue(new Promise<FileJumpContext>((resolve) => { finish = resolve; }));
    const { cache, wrapper } = setup();
    const { result, rerender } = renderHook(({ id }) => useFileNavigation(id), { initialProps: { id: 'case-a' }, wrapper });
    act(() => result.current.jump('old-file'));
    await waitFor(() => expect(mocks.resolve).toHaveBeenCalledTimes(1));
    const summary = { name: 'case', createdAt: '', updatedAt: '' };
    act(() => resetSearchContext(cache, { ...summary, id: 'case-b' }));
    rerender({ id: 'case-b' });
    act(() => resetSearchContext(cache, { ...summary, id: 'case-a' }));
    rerender({ id: 'case-a' });
    await act(async () => finish(context('old-file')));
    await waitFor(() => expect(cache.getMutationCache().getAll()[0].state.status).not.toBe('pending'));
    expect(mocks.navigate).not.toHaveBeenCalled();
    expect(useSelectionStore.getState().selectedFileId).toBeUndefined();
  });

  it('publishes only the newest requested target when lookups finish out of order', async () => {
    let finish!: (value: FileJumpContext) => void;
    mocks.resolve.mockImplementation((id: string) => id === 'old-file'
      ? new Promise<FileJumpContext>((resolve) => { finish = resolve; }) : Promise.resolve(context(id)));
    const { cache, wrapper } = setup();
    const { result } = renderHook(() => useFileNavigation('case-a'), { wrapper });
    act(() => result.current.jump('old-file'));
    await waitFor(() => expect(mocks.resolve).toHaveBeenCalledTimes(1));
    act(() => result.current.jump('new-file'));
    await waitFor(() => expect(useSelectionStore.getState().selectedFileId).toBe('new-file'));
    await act(async () => finish(context('old-file')));
    await waitFor(() => expect(cache.getMutationCache().getAll()[0].state.status).not.toBe('pending'));
    expect(mocks.navigate).toHaveBeenCalledTimes(1);
    expect(useSelectionStore.getState().selectedFileId).toBe('new-file');
  });
});
