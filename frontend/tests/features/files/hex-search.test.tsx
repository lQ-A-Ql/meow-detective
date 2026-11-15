import { act, fireEvent, render, renderHook, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { HexSearchToolbar } from '@/components/preview/HexSearchToolbar';
import { useHexSearch } from '@/features/files/hooks/use-hex-search';
import { useHexSearchStore } from '@/stores/hex-search-store';
import * as filesApi from '@/lib/api/files';
import type { HexSearchStatus } from '@/types/models';

vi.mock('@/lib/api/files', () => ({
  startHexSearch: vi.fn(),
  getHexSearchStatus: vi.fn(),
  cancelHexSearch: vi.fn(),
}));

const startHexSearch = vi.mocked(filesApi.startHexSearch);
const getHexSearchStatus = vi.mocked(filesApi.getHexSearchStatus);
const cancelHexSearch = vi.mocked(filesApi.cancelHexSearch);

function status(overrides: Partial<HexSearchStatus> = {}): HexSearchStatus {
  return {
    taskId: 'hex-search:c1:f1:1',
    fileId: 'f1',
    phase: 'running',
    scannedBytes: 0,
    totalBytes: 1024,
    matches: [],
    truncated: false,
    ...overrides,
  };
}

beforeEach(() => {
  useHexSearchStore.setState({ byFile: {} });
  startHexSearch.mockReset();
  getHexSearchStatus.mockReset();
  cancelHexSearch.mockReset();
});

describe('hex search store', () => {
  it('preserves the matches array across progress-only polls and resets on a new task', () => {
    const store = useHexSearchStore.getState();
    store.beginSearch('f1', 'hex', '41 42');
    store.applyStatus('f1', status({ matches: [4, 12] }));
    const matches = useHexSearchStore.getState().byFile.f1.matches;
    useHexSearchStore.getState().applyStatus('f1', status({ matches: [4, 12], scannedBytes: 512 }));
    expect(useHexSearchStore.getState().byFile.f1.matches).toBe(matches);
    expect(useHexSearchStore.getState().byFile.f1.activeIndex).toBe(0);

    useHexSearchStore.getState().setActiveIndex('f1', 1);
    useHexSearchStore
      .getState()
      .applyStatus('f1', status({ taskId: 'hex-search:c1:f1:2', matches: [4, 12, 20], scannedBytes: 1024 }));
    const next = useHexSearchStore.getState().byFile.f1;
    expect(next.matches).not.toBe(matches);
    expect(next.activeIndex).toBe(0);
  });
});

describe('useHexSearch', () => {
  it('starts a scan, jumps to the first match, and stops polling on completion', async () => {
    startHexSearch.mockResolvedValue(status({ matches: [16, 32] }));
    getHexSearchStatus.mockResolvedValue(status({ phase: 'completed', matches: [16, 32], scannedBytes: 1024 }));
    const onJump = vi.fn();
    const { result } = renderHook(() => useHexSearch({ fileId: 'f1', onJump }));

    await act(async () => {
      await result.current.start('hex', '10 20');
    });
    expect(startHexSearch).toHaveBeenCalledWith({ fileId: 'f1', mode: 'hex', pattern: '10 20' });
    await waitFor(() => expect(result.current.state?.matches).toEqual([16, 32]));
    await waitFor(() => expect(onJump).toHaveBeenCalledWith('0x10'));

    await waitFor(() => expect(getHexSearchStatus).toHaveBeenCalled(), { timeout: 1500 });
    await waitFor(() => expect(result.current.state?.phase).toBe('completed'));
    expect(result.current.state?.scannedBytes).toBe(1024);
  });

  it('cycles matches and wraps in both directions', async () => {
    startHexSearch.mockResolvedValue(status({ matches: [16, 32] }));
    const onJump = vi.fn();
    const { result } = renderHook(() => useHexSearch({ fileId: 'f1', onJump }));
    await act(async () => {
      await result.current.start('hex', '10 20');
    });
    await waitFor(() => expect(result.current.state?.activeIndex).toBe(0));

    act(() => result.current.next());
    expect(result.current.state?.activeIndex).toBe(1);
    await waitFor(() => expect(onJump).toHaveBeenCalledWith('0x20'));
    act(() => result.current.next());
    expect(result.current.state?.activeIndex).toBe(0);
    act(() => result.current.previous());
    expect(result.current.state?.activeIndex).toBe(1);
  });

  it('marks the scan cancelling and clears local state on reset', async () => {
    startHexSearch.mockResolvedValue(status());
    cancelHexSearch.mockResolvedValue(true);
    const { result } = renderHook(() => useHexSearch({ fileId: 'f1', onJump: vi.fn() }));
    await act(async () => {
      await result.current.start('ascii', 'needle');
    });

    await act(async () => {
      await result.current.cancel();
    });
    expect(cancelHexSearch).toHaveBeenCalledWith({ fileId: 'f1' });
    expect(result.current.state?.phase).toBe('cancelling');

    await act(async () => {
      await result.current.reset();
    });
    expect(result.current.state).toBeUndefined();
  });
});

describe('HexSearchToolbar', () => {
  it('starts hex searches, rejects odd patterns, and clears on escape', () => {
    const onStart = vi.fn();
    const onReset = vi.fn();
    render(
      <HexSearchToolbar
        fileId="f1"
        onStart={onStart}
        onCancel={vi.fn()}
        onReset={onReset}
        onPrevious={vi.fn()}
        onNext={vi.fn()}
      />,
    );
    const input = screen.getByRole('textbox');

    fireEvent.change(input, { target: { value: '41 42' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onStart).toHaveBeenCalledWith('hex', '41 42');

    fireEvent.change(input, { target: { value: 'abc' } });
    fireEvent.keyDown(input, { key: 'Enter' });
    expect(onStart).toHaveBeenCalledTimes(1);
    expect(screen.getByText('模式无效')).toBeInTheDocument();

    fireEvent.keyDown(input, { key: 'Escape' });
    expect(onReset).toHaveBeenCalled();
    expect(input).toHaveValue('');
  });

  it('switches to ASCII and submits UTF-8 text', () => {
    const onStart = vi.fn();
    render(
      <HexSearchToolbar
        fileId="f1"
        onStart={onStart}
        onCancel={vi.fn()}
        onReset={vi.fn()}
        onPrevious={vi.fn()}
        onNext={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: 'ASCII' }));
    fireEvent.change(screen.getByRole('textbox'), { target: { value: 'needle' } });
    fireEvent.keyDown(screen.getByRole('textbox'), { key: 'Enter' });
    expect(onStart).toHaveBeenCalledWith('ascii', 'needle');
  });
});
