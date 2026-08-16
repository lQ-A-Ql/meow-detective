import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  useCurrentCase: vi.fn(),
  listPluginActions: vi.fn(),
  recoverWeChatKeys: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.useCurrentCase,
}));

vi.mock('@/lib/api/analysis', () => ({
  listPluginActions: mocks.listPluginActions,
  recoverWeChatKeys: mocks.recoverWeChatKeys,
}));

import { usePluginActions, useRecoverWeChatKeys } from './plugin-action-hooks';

function createQueryClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
}

function createWrapper(queryClient: QueryClient) {
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

const ACTION = {
  id: 'recoverKeys',
  label: '从内存镜像恢复数据库密钥',
  description: 'desc',
  inputKind: 'file',
};

describe('usePluginActions', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.useCurrentCase.mockReturnValue({
      data: { id: 'case-1' },
      isSuccess: true,
    });
  });

  it('stays disabled without a pluginId', () => {
    const queryClient = createQueryClient();
    const { result } = renderHook(() => usePluginActions(undefined), {
      wrapper: createWrapper(queryClient),
    });
    expect(result.current.fetchStatus).toBe('idle');
    expect(mocks.listPluginActions).not.toHaveBeenCalled();
  });

  it('loads the descriptors for a plugin', async () => {
    mocks.listPluginActions.mockResolvedValue([ACTION]);
    const queryClient = createQueryClient();
    const { result } = renderHook(() => usePluginActions('wechat'), {
      wrapper: createWrapper(queryClient),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.listPluginActions).toHaveBeenCalledWith('wechat');
    expect(result.current.data).toEqual([ACTION]);
  });
});

describe('useRecoverWeChatKeys', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.useCurrentCase.mockReturnValue({
      data: { id: 'case-1' },
      isSuccess: true,
    });
  });

  it('calls the api and invalidates plugin module/family caches on success', async () => {
    const result = {
      candidatesSeen: 9,
      recoveredCount: 2,
      matchedDbNames: ['EnMicroMsg.db'],
      unmatchedDbNames: ['SnsMicroMsg.db'],
    };
    mocks.recoverWeChatKeys.mockResolvedValue(result);
    const queryClient = createQueryClient();
    queryClient.setQueryData(['analysis', 'plugin-modules', 'case-1', 'ds-1'], []);
    queryClient.setQueryData(
      ['analysis', 'plugin-family-entries', 'case-1', 'ds-1', 'wechat', 'Chat'],
      { pages: [], pageParams: [] },
    );

    const { result: hook } = renderHook(() => useRecoverWeChatKeys(), {
      wrapper: createWrapper(queryClient),
    });
    await hook.current.mutateAsync({ dataSourceId: 'ds-1', dumpPath: 'D:/dump.raw' });

    expect(mocks.recoverWeChatKeys).toHaveBeenCalledWith('ds-1', 'D:/dump.raw');
    await waitFor(() => expect(hook.current.data).toEqual(result));
    expect(
      queryClient.getQueryState(['analysis', 'plugin-modules', 'case-1', 'ds-1'])?.isInvalidated,
    ).toBe(true);
    expect(
      queryClient.getQueryState(
        ['analysis', 'plugin-family-entries', 'case-1', 'ds-1', 'wechat', 'Chat'],
      )?.isInvalidated,
    ).toBe(true);
  });

  it('surfaces api errors as the mutation error', async () => {
    mocks.recoverWeChatKeys.mockRejectedValue({
      code: 'COMMAND_FAILED',
      message: 'dump 读取失败',
      category: 'io',
      recoverable: true,
    });
    const queryClient = createQueryClient();
    const { result } = renderHook(() => useRecoverWeChatKeys(), {
      wrapper: createWrapper(queryClient),
    });
    await expect(
      result.current.mutateAsync({ dataSourceId: 'ds-1', dumpPath: 'D:/dump.raw' }),
    ).rejects.toMatchObject({ message: 'dump 读取失败' });
    await waitFor(() => expect(result.current.isError).toBe(true));
  });
});
