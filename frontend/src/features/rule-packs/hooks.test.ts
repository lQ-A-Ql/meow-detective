import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  listLoadedRulePacks: vi.fn(),
  loadRulePack: vi.fn(),
  validateRulePack: vi.fn(),
}));

vi.mock('@/lib/api/rule-packs', () => ({
  listLoadedRulePacks: mocks.listLoadedRulePacks,
  loadRulePack: mocks.loadRulePack,
  validateRulePack: mocks.validateRulePack,
}));

import { useLoadedRulePacks, useLoadRulePack, useValidateRulePack } from './hooks';

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

describe('rule-packs hooks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.listLoadedRulePacks.mockResolvedValue([
      { id: 'rp-1', name: 'Default Pack', ruleCount: 5 },
    ]);
    mocks.loadRulePack.mockResolvedValue({
      id: 'rp-2',
      name: 'Custom Pack',
      ruleCount: 3,
    });
    mocks.validateRulePack.mockResolvedValue({ valid: true, errors: [] });
  });

  it('fetches loaded rule packs', async () => {
    const { result } = renderHook(() => useLoadedRulePacks(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.listLoadedRulePacks).toHaveBeenCalledTimes(1);
    expect(result.current.data).toHaveLength(1);
  });

  it('loads a rule pack by path and invalidates queries', async () => {
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');

    const wrapper = function Wrapper({ children }: { children: React.ReactNode }) {
      return createElement(QueryClientProvider, { client: qc }, children);
    };

    const { result } = renderHook(() => useLoadRulePack(), { wrapper });

    await result.current.mutateAsync('/path/to/pack.json');

    expect(mocks.loadRulePack).toHaveBeenCalledWith('/path/to/pack.json');
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['rule-packs', 'loaded'] });
  });

  it('validates a rule pack by id and invalidates queries', async () => {
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');

    const wrapper = function Wrapper({ children }: { children: React.ReactNode }) {
      return createElement(QueryClientProvider, { client: qc }, children);
    };

    const { result } = renderHook(() => useValidateRulePack(), { wrapper });

    await result.current.mutateAsync('rp-1');

    expect(mocks.validateRulePack).toHaveBeenCalledWith('rp-1');
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['rule-packs', 'loaded'] });
  });

  it('exposes error state when load fails', async () => {
    mocks.loadRulePack.mockRejectedValue(new Error('file not found'));

    const { result } = renderHook(() => useLoadRulePack(), {
      wrapper: createWrapper(),
    });

    result.current.mutate('/missing/pack.json');

    await waitFor(() => expect(result.current.isError).toBe(true));
    expect((result.current.error as Error).message).toBe('file not found');
  });

  it('exposes error state when validate fails', async () => {
    mocks.validateRulePack.mockRejectedValue(new Error('invalid pack'));

    const { result } = renderHook(() => useValidateRulePack(), {
      wrapper: createWrapper(),
    });

    result.current.mutate('bad-pack');

    await waitFor(() => expect(result.current.isError).toBe(true));
    expect((result.current.error as Error).message).toBe('invalid pack');
  });
});
