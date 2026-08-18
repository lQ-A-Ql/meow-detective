import { createElement } from 'react';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  usePluginActions: vi.fn(),
  useRecoverWeChatKeys: vi.fn(),
  useRunAnalysisExtraction: vi.fn(),
}));

vi.mock('@/features/analysis/plugin-action-hooks', () => ({
  usePluginActions: mocks.usePluginActions,
  useRecoverWeChatKeys: mocks.useRecoverWeChatKeys,
}));

vi.mock('@/features/analysis/hooks', () => ({
  useRunAnalysisExtraction: mocks.useRunAnalysisExtraction,
}));

import { PluginActionsSection } from './PluginActionsSection';
import type { PluginActionDescriptor } from '@/types/models';

const FILE_ACTION: PluginActionDescriptor = {
  id: 'recoverKeys',
  label: '从内存镜像恢复数据库密钥',
  description: '扫描内存镜像并离线验证匹配',
  inputKind: 'file',
};

function recoveryState(overrides: Record<string, unknown> = {}) {
  return {
    mutateAsync: vi.fn().mockResolvedValue(undefined),
    isPending: false,
    data: undefined,
    error: null,
    reset: vi.fn(),
    ...overrides,
  };
}

function rerunState(overrides: Record<string, unknown> = {}) {
  return {
    mutateAsync: vi.fn().mockResolvedValue(undefined),
    isPending: false,
    isSuccess: false,
    error: null,
    reset: vi.fn(),
    ...overrides,
  };
}

describe('PluginActionsSection', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.usePluginActions.mockReturnValue({ data: [], isLoading: false });
    mocks.useRecoverWeChatKeys.mockReturnValue(recoveryState());
    mocks.useRunAnalysisExtraction.mockReturnValue(rerunState());
  });

  it('renders nothing when the plugin declares no actions', () => {
    const { container } = render(createElement(PluginActionsSection, {
      dataSourceId: 'ds-1',
      pluginId: 'prefetch',
    }));
    expect(container.firstChild).toBeNull();
  });

  it('renders descriptor label/description and the file picker for inputKind=file', () => {
    mocks.usePluginActions.mockReturnValue({ data: [FILE_ACTION], isLoading: false });
    render(createElement(PluginActionsSection, {
      dataSourceId: 'ds-1',
      pluginId: 'wechat',
    }));
    expect(screen.getByText('插件操作')).toBeDefined();
    expect(screen.getByText('从内存镜像恢复数据库密钥')).toBeDefined();
    expect(screen.getByText('扫描内存镜像并离线验证匹配')).toBeDefined();
    expect(screen.getByLabelText('输入文件路径')).toBeDefined();
  });

  it('fills the path via the injected file picker', async () => {
    mocks.usePluginActions.mockReturnValue({ data: [FILE_ACTION], isLoading: false });
    const pickFilePath = vi.fn().mockResolvedValue('D:/dumps/wechat.raw');
    render(createElement(PluginActionsSection, {
      dataSourceId: 'ds-1',
      pluginId: 'wechat',
      pickFilePath,
    }));
    fireEvent.click(screen.getByRole('button', { name: /选择文件/ }));
    await waitFor(() => {
      expect(screen.getByLabelText('输入文件路径')).toHaveProperty('value', 'D:/dumps/wechat.raw');
    });
    expect(pickFilePath).toHaveBeenCalledTimes(1);
  });

  it('blocks the run and shows a hint when the required file is missing', async () => {
    mocks.usePluginActions.mockReturnValue({ data: [FILE_ACTION], isLoading: false });
    const recovery = recoveryState();
    mocks.useRecoverWeChatKeys.mockReturnValue(recovery);
    render(createElement(PluginActionsSection, {
      dataSourceId: 'ds-1',
      pluginId: 'wechat',
    }));
    fireEvent.click(screen.getByRole('button', { name: '运行' }));
    await waitFor(() => {
      expect(screen.getByText('请先选择输入文件。')).toBeDefined();
    });
    expect(recovery.mutateAsync).not.toHaveBeenCalled();
  });

  it('runs the recovery with the chosen dump path', async () => {
    mocks.usePluginActions.mockReturnValue({ data: [FILE_ACTION], isLoading: false });
    const recovery = recoveryState();
    mocks.useRecoverWeChatKeys.mockReturnValue(recovery);
    render(createElement(PluginActionsSection, {
      dataSourceId: 'ds-1',
      pluginId: 'wechat',
    }));
    fireEvent.change(screen.getByLabelText('输入文件路径'), {
      target: { value: '  D:/dump.raw  ' },
    });
    fireEvent.click(screen.getByRole('button', { name: '运行' }));
    await waitFor(() => {
      expect(recovery.mutateAsync).toHaveBeenCalledWith({
        dataSourceId: 'ds-1',
        dumpPath: 'D:/dump.raw',
      });
    });
  });

  it('disables the run button while the action is running', () => {
    mocks.usePluginActions.mockReturnValue({ data: [FILE_ACTION], isLoading: false });
    mocks.useRecoverWeChatKeys.mockReturnValue(recoveryState({ isPending: true }));
    render(createElement(PluginActionsSection, {
      dataSourceId: 'ds-1',
      pluginId: 'wechat',
    }));
    const runButton = screen.getByRole('button', { name: /运行中/ });
    expect(runButton).toHaveProperty('disabled', true);
    expect(screen.getByRole('button', { name: /选择文件/ })).toHaveProperty('disabled', true);
  });

  it('shows the api error message when the run fails', () => {
    mocks.usePluginActions.mockReturnValue({ data: [FILE_ACTION], isLoading: false });
    mocks.useRecoverWeChatKeys.mockReturnValue(recoveryState({
      error: { code: 'X', message: 'dump 读取失败', category: 'io', recoverable: true },
    }));
    render(createElement(PluginActionsSection, {
      dataSourceId: 'ds-1',
      pluginId: 'wechat',
    }));
    expect(screen.getByText('dump 读取失败')).toBeDefined();
  });

  it('shows the recovery result and triggers the plugin analysis rerun', async () => {
    mocks.usePluginActions.mockReturnValue({ data: [FILE_ACTION], isLoading: false });
    mocks.useRecoverWeChatKeys.mockReturnValue(recoveryState({
      data: {
        candidatesSeen: 12,
        recoveredCount: 3,
        matchedDbNames: ['EnMicroMsg.db', 'SnsMicroMsg.db'],
        unmatchedDbNames: ['Favorite.db'],
        recoveredKeys: [{ databaseName: 'EnMicroMsg.db', keyHex: 'ab'.repeat(32) }],
      },
    }));
    const rerun = rerunState();
    mocks.useRunAnalysisExtraction.mockReturnValue(rerun);
    render(createElement(PluginActionsSection, {
      dataSourceId: 'ds-1',
      pluginId: 'wechat',
    }));
    expect(screen.getByText(/扫描候选/).textContent).toContain('12');
    expect(screen.getByText(/恢复成功/).textContent).toContain('3');
    expect(screen.getByText(/EnMicroMsg\.db, SnsMicroMsg\.db/)).toBeDefined();
    expect(screen.getByText(/Favorite\.db/)).toBeDefined();
    expect(screen.getByText(/重新运行该数据源的插件分析/)).toBeDefined();

    fireEvent.click(screen.getByRole('button', { name: /重新运行插件分析/ }));
    await waitFor(() => {
      expect(rerun.mutateAsync).toHaveBeenCalledWith({
        dataSourceId: 'ds-1',
        categories: ['PluginArtifacts'],
      });
    });
  });

  it('reports verified plaintext keys to the plugin title owner', async () => {
    mocks.usePluginActions.mockReturnValue({ data: [FILE_ACTION], isLoading: false });
    const result = {
      candidatesSeen: 1,
      recoveredCount: 1,
      matchedDbNames: ['message_0.db'],
      unmatchedDbNames: [],
      recoveredKeys: [{ databaseName: 'message_0.db', keyHex: 'cd'.repeat(32) }],
    };
    mocks.useRecoverWeChatKeys.mockReturnValue(recoveryState({
      mutateAsync: vi.fn().mockResolvedValue(result),
    }));
    const onRecoveredKeys = vi.fn();
    render(createElement(PluginActionsSection, {
      dataSourceId: 'ds-1',
      pluginId: 'wechat',
      onRecoveredKeys,
    }));
    fireEvent.change(screen.getByLabelText('输入文件路径'), {
      target: { value: 'D:/dump.raw' },
    });
    fireEvent.click(screen.getByRole('button', { name: '运行' }));

    await waitFor(() => {
      expect(onRecoveredKeys).toHaveBeenCalledWith(result.recoveredKeys);
    });
  });

  it('marks unknown action ids as unsupported and disables the run', () => {
    mocks.usePluginActions.mockReturnValue({
      data: [{ id: 'otherAction', label: '其他操作', inputKind: 'none' }],
      isLoading: false,
    });
    render(createElement(PluginActionsSection, {
      dataSourceId: 'ds-1',
      pluginId: 'wechat',
    }));
    expect(screen.getByText('其他操作')).toBeDefined();
    expect(screen.getByText('该操作暂不支持在界面中运行。')).toBeDefined();
    expect(screen.getByRole('button', { name: '运行' })).toHaveProperty('disabled', true);
  });
});
