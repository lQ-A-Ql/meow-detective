import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useBitLockerPanelState } from '@/features/files/hooks/use-bitlocker-panel-state';
import type { BitLockerVolumeModel } from '@/features/files/hooks/use-bitlocker-volume';

describe('BitLocker transient panel state', () => {
  let clipboardValue = '';
  const writeText = vi.fn(async (value: string) => { clipboardValue = value; });
  const readText = vi.fn(async () => clipboardValue);
  beforeEach(() => {
    vi.useFakeTimers(); clipboardValue = ''; vi.clearAllMocks();
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText, readText } });
  });
  afterEach(() => { vi.useRealTimers(); });
  const model = () => ({ unlock: vi.fn().mockResolvedValue(true) }) as unknown as BitLockerVolumeModel;

  it('clears credential state before awaiting unlock', () => {
    const volume = model(); const { result } = renderHook(() => useBitLockerPanelState(volume));
    act(() => result.current.setCredential('synthetic-password'));
    act(() => result.current.submitCredential());
    expect(result.current.credential).toBe('');
    expect(volume.unlock).toHaveBeenCalledWith('password', 'synthetic-password');
  });

  it('expires a copied recovery password', async () => {
    const { result } = renderHook(() => useBitLockerPanelState(model()));
    await act(() => result.current.copyRecoveredPassword('synthetic-recovery-password'));
    expect(result.current.passwordCopied).toBe(true);
    await act(() => vi.advanceTimersByTimeAsync(1500));
    expect(clipboardValue).toBe(''); expect(result.current.passwordCopied).toBe(false);
  });

  it('does not erase a clipboard value written by another application', async () => {
    const { result } = renderHook(() => useBitLockerPanelState(model()));
    await act(() => result.current.copyRecoveredPassword('synthetic-recovery-password'));
    clipboardValue = 'new unrelated value';
    await act(() => vi.advanceTimersByTimeAsync(1500));
    expect(clipboardValue).toBe('new unrelated value');
  });

  it('cleans a late clipboard write after unmount', async () => {
    let resolve!: () => void;
    writeText.mockImplementationOnce((value: string) => new Promise<void>((done) => {
      resolve = () => { clipboardValue = value; done(); };
    }));
    const { result, unmount } = renderHook(() => useBitLockerPanelState(model()));
    const pending = result.current.copyRecoveredPassword('synthetic-recovery-password');
    unmount(); resolve(); await pending;
    expect(clipboardValue).toBe('');
  });

  it('handles denied clipboard writes without an unhandled rejection', async () => {
    writeText.mockRejectedValueOnce(new Error('denied'));
    const { result } = renderHook(() => useBitLockerPanelState(model()));
    await act(() => result.current.copyRecoveredPassword('synthetic-recovery-password'));
    expect(result.current.passwordCopied).toBe(false);
  });
});
