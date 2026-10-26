import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { Settings } from '@/app/pages/Settings';

const mocks = vi.hoisted(() => ({
  appSettings: vi.fn(),
  saveAppSettings: vi.fn(),
  saveSettingsMutation: {
    isPending: false,
    mutateAsync: vi.fn(),
  },
}));

vi.mock('@/features/settings/hooks', () => ({
  useAppSettings: mocks.appSettings,
  useSaveAppSettings: mocks.saveAppSettings,
}));

vi.mock('@/features/mcp/containers/McpHostToolsPanelContainer', () => ({
  McpHostToolsPanelContainer: () => <div data-testid="mcp-host-panel" />,
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
    mocks.appSettings.mockReturnValue({
      data: {
        caseRoot: 'D:\\Cases',
        imageSearchPaths: ['D:\\Images', 'E:\\Evidence'],
        devEventTrace: true,
        maxImportWorkers: 1,
        maxAnalysisWorkers: 4,
        importAnalysisMode: 'budgetedContent',
        hexChunkBytes: 32768,
        maxViewerRangeLength: 1048576,
        maxInlineImagePreviewBytes: 5242880,
        maxInlineMediaPreviewBytes: 20971520,
      },
      error: null,
      isLoading: false,
    });
    mocks.saveSettingsMutation.isPending = false;
    mocks.saveSettingsMutation.mutateAsync.mockImplementation(async (settings) => settings);
    mocks.saveAppSettings.mockReturnValue(mocks.saveSettingsMutation);
  });

  it('renders without crashing', async () => {
    render(<Settings />);
    expect(await screen.findByRole('button', { name: '保存设置' })).toBeTruthy();
  });

  it('loads persisted backend settings', async () => {
    render(<Settings />);

    expect(await screen.findByDisplayValue('D:\\Cases')).toBeTruthy();
    expect((screen.getByLabelText('镜像搜索路径') as HTMLInputElement).value).toBe(
      'D:\\Images; E:\\Evidence',
    );
    expect(screen.getByLabelText('事件调试日志').getAttribute('data-state')).toBe('checked');
  });

  it('saves settings through the API wrapper and mirrors lightweight UI state locally', async () => {
    const storage = createLocalStorageMock();
    vi.stubGlobal('localStorage', storage);
    render(<Settings />);

    const caseRoot = await screen.findByLabelText('案件默认存储路径');
    fireEvent.change(caseRoot, { target: { value: 'C:\\Meow_Detective\\cases' } });
    fireEvent.change(screen.getByLabelText('镜像搜索路径'), {
      target: { value: 'D:\\Images; F:\\MoreImages' },
    });
    fireEvent.click(screen.getByLabelText('事件调试日志'));
    fireEvent.click(screen.getByRole('button', { name: '保存设置' }));

    await waitFor(() => {
      expect(mocks.saveSettingsMutation.mutateAsync).toHaveBeenCalledWith({
        caseRoot: 'C:\\Meow_Detective\\cases',
        imageSearchPaths: ['D:\\Images', 'F:\\MoreImages'],
        devEventTrace: false,
        maxImportWorkers: 1,
        maxAnalysisWorkers: 4,
        importAnalysisMode: 'budgetedContent',
        hexChunkBytes: 32768,
        maxViewerRangeLength: 1048576,
        maxInlineImagePreviewBytes: 5242880,
        maxInlineMediaPreviewBytes: 20971520,
      });
    });
    expect(await screen.findByText('设置已保存。')).toBeTruthy();
    expect(storage.setItem).toHaveBeenCalledWith(
      'Meow_Detective.localSettings',
      expect.stringContaining('"caseRoot":"C:\\\\Meow_Detective\\\\cases"'),
    );
  });

  it('keeps local fallback when backend settings cannot be loaded', async () => {
    mocks.appSettings.mockReturnValue({
      data: undefined,
      error: new Error('settings unavailable'),
      isLoading: false,
    });

    render(<Settings />);

    expect(await screen.findByDisplayValue('C:\\Meow_Detective\\cases')).toBeTruthy();
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
    expect(mocks.saveSettingsMutation.mutateAsync).not.toHaveBeenCalled();
  });
});
