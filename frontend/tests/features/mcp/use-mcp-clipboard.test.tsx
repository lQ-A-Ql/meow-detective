import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useMcpClipboard } from '@/features/mcp/use-mcp-clipboard';

const originalClipboard = Object.getOwnPropertyDescriptor(navigator, 'clipboard');

describe('useMcpClipboard', () => {
  const writeText = vi.fn(async (_value: string) => undefined);

  beforeEach(() => {
    vi.useFakeTimers();
    writeText.mockReset();
    writeText.mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText },
    });
  });

  afterEach(() => {
    vi.useRealTimers();
    if (originalClipboard) {
      Object.defineProperty(navigator, 'clipboard', originalClipboard);
    } else {
      Reflect.deleteProperty(navigator, 'clipboard');
    }
  });

  it('copies a value and clears the acknowledgement after 1500 ms', async () => {
    const { result } = renderHook(() => useMcpClipboard());

    expect(result.current.copiedValue).toBeUndefined();
    expect(result.current.copyError).toBe(false);

    await act(async () => {
      await result.current.copy('http://127.0.0.1:3001/mcp');
    });

    expect(writeText).toHaveBeenCalledWith('http://127.0.0.1:3001/mcp');
    expect(result.current.copiedValue).toBe('http://127.0.0.1:3001/mcp');
    expect(result.current.copyError).toBe(false);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_499);
    });
    expect(result.current.copiedValue).toBe('http://127.0.0.1:3001/mcp');

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(result.current.copiedValue).toBeUndefined();
  });

  it('keeps a newer value when an older acknowledgement timer expires', async () => {
    const { result } = renderHook(() => useMcpClipboard());

    await act(async () => {
      await result.current.copy('first');
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1_000);
    });
    await act(async () => {
      await result.current.copy('second');
    });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(600);
    });
    expect(result.current.copiedValue).toBe('second');

    await act(async () => {
      await vi.advanceTimersByTimeAsync(900);
    });
    expect(result.current.copiedValue).toBeUndefined();
  });

  it('flags a rejected clipboard write', async () => {
    writeText.mockRejectedValueOnce(new Error('clipboard denied'));
    const { result } = renderHook(() => useMcpClipboard());

    await act(async () => {
      await result.current.copy('secret');
    });

    expect(result.current.copyError).toBe(true);
    expect(result.current.copiedValue).toBeUndefined();
  });

  it('flags a missing clipboard API without writing', async () => {
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: undefined,
    });
    const { result } = renderHook(() => useMcpClipboard());

    await act(async () => {
      await result.current.copy('secret');
    });

    expect(result.current.copyError).toBe(true);
    expect(result.current.copiedValue).toBeUndefined();
    expect(writeText).not.toHaveBeenCalled();
  });

  it('clears a previous error after a successful retry', async () => {
    writeText.mockRejectedValueOnce(new Error('denied'));
    const { result } = renderHook(() => useMcpClipboard());

    await act(async () => {
      await result.current.copy('endpoint');
    });
    expect(result.current.copyError).toBe(true);

    await act(async () => {
      await result.current.copy('endpoint');
    });

    expect(result.current.copyError).toBe(false);
    expect(result.current.copiedValue).toBe('endpoint');
  });
});
