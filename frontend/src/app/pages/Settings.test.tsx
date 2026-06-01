import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { Settings } from './Settings';

const mocks = vi.hoisted(() => ({
  getAppSettings: vi.fn(),
  saveAppSettings: vi.fn(),
  mcpState: {
    servers: [],
    selectedServerId: null,
    loading: false,
    error: null,
    loadConfig: vi.fn(),
    addServer: vi.fn(),
    removeServer: vi.fn(),
    connectServer: vi.fn(),
    disconnectServer: vi.fn(),
    testConnection: vi.fn(),
    selectServer: vi.fn(),
  },
}));

vi.mock('@/lib/api/settings', () => ({
  getAppSettings: mocks.getAppSettings,
  saveAppSettings: mocks.saveAppSettings,
}));

vi.mock('@/stores/mcp-store', () => ({
  useMcpStore: () => mocks.mcpState,
}));

vi.mock('@/components/mcp/McpServerItem', () => ({
  McpServerItem: () => <div data-testid="mcp-server-item" />,
}));

vi.mock('@/components/mcp/McpServerDialog', () => ({
  McpServerDialog: () => <div data-testid="mcp-server-dialog" />,
}));

vi.mock('@/components/mcp/McpResourceList', () => ({
  McpResourceList: () => <div data-testid="mcp-resource-list" />,
}));

vi.mock('@/components/mcp/McpToolList', () => ({
  McpToolList: () => <div data-testid="mcp-tool-list" />,
}));

function createLocalStorageMock() {
  const store = new Map<string, string>();
  return {
    getItem: vi.fn((key: string) => store.get(key) ?? null),
    setItem: vi.fn((key: string, value: string) => store.set(key, value)),
    removeItem: vi.fn((key: string) => store.delete(key)),
    clear: vi.fn(() => store.clear()),
  };
}

describe('Settings page', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.stubGlobal('localStorage', createLocalStorageMock());
    document.documentElement.removeAttribute('data-theme');
    document.documentElement.classList.remove('dark');
    mocks.getAppSettings.mockResolvedValue({
      caseRoot: 'D:\\Cases',
      imageSearchPaths: ['D:\\Images', 'E:\\Evidence'],
      theme: 'dark',
      devEventTrace: true,
    });
    mocks.saveAppSettings.mockImplementation(async (settings) => settings);
  });

  it('loads persisted backend settings and applies theme state', async () => {
    render(<Settings />);

    expect(await screen.findByDisplayValue('D:\\Cases')).toBeTruthy();
    expect((screen.getByLabelText('镜像搜索路径') as HTMLInputElement).value).toBe(
      'D:\\Images; E:\\Evidence',
    );
    const themeSelect = screen.getByRole('combobox') as HTMLSelectElement;
    expect(themeSelect.value).toBe('dark');
    expect((screen.getByLabelText('事件调试日志') as HTMLInputElement).checked).toBe(true);
    await waitFor(() => expect(document.documentElement.dataset.theme).toBe('dark'));
    expect(mocks.mcpState.loadConfig).toHaveBeenCalledTimes(1);
  });

  it('saves settings through the API wrapper and mirrors lightweight UI state locally', async () => {
    const storage = createLocalStorageMock();
    vi.stubGlobal('localStorage', storage);
    render(<Settings />);

    const caseRoot = await screen.findByLabelText('案件目录');
    const themeSelect = screen.getByRole('combobox') as HTMLSelectElement;
    await waitFor(() => expect(themeSelect.value).toBe('dark'));
    fireEvent.change(caseRoot, { target: { value: 'C:\\Cases' } });
    fireEvent.change(screen.getByLabelText('镜像搜索路径'), {
      target: { value: 'D:\\Images; F:\\MoreImages' },
    });
    fireEvent.change(themeSelect, { target: { value: 'light' } });
    fireEvent.click(screen.getByLabelText('事件调试日志'));
    fireEvent.click(screen.getByRole('button', { name: '保存设置' }));

    await waitFor(() => {
      expect(mocks.saveAppSettings).toHaveBeenCalledWith({
        caseRoot: 'C:\\Cases',
        imageSearchPaths: ['D:\\Images', 'F:\\MoreImages'],
        theme: 'light',
        devEventTrace: false,
      });
    });
    expect(await screen.findByText('设置已保存。')).toBeTruthy();
    expect(storage.setItem).toHaveBeenCalledWith(
      'forensics.localSettings',
      expect.stringContaining('"caseRoot":"C:\\\\Cases"'),
    );
    expect(document.documentElement.dataset.theme).toBe('light');
  });

  it('keeps local fallback when backend settings cannot be loaded', async () => {
    mocks.getAppSettings.mockRejectedValue(new Error('mock mode'));

    render(<Settings />);

    expect(await screen.findByDisplayValue('C:\\Cases')).toBeTruthy();
    expect((screen.getByLabelText('镜像搜索路径') as HTMLInputElement).value).toBe(
      'E:\\cases\\; D:\\images\\',
    );
  });

  it('rejects invalid image search path lists before saving', async () => {
    render(<Settings />);

    await screen.findByLabelText('镜像搜索路径');
    fireEvent.change(screen.getByLabelText('镜像搜索路径'), {
      target: { value: 'D:\\Images\0' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存设置' }));

    expect(await screen.findByText('镜像搜索路径包含非法字符。')).toBeTruthy();
    expect(mocks.saveAppSettings).not.toHaveBeenCalled();
  });
});
