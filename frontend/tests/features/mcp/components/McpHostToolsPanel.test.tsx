import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getMcpHostStatus, listMcpHostTools, callMcpHostTool, setMcpHostSettings } from '@/lib/api/mcp-host';
import { McpHostToolsPanel } from '@/features/mcp/components/McpHostToolsPanel';

vi.mock('@/lib/api/mcp-host', () => ({ getMcpHostStatus: vi.fn(), listMcpHostTools: vi.fn(), callMcpHostTool: vi.fn(), setMcpHostSettings: vi.fn() }));

const status = { running: true, endpoint: 'http://127.0.0.1:3001/mcp', settings: { enabled: true, disabledTools: [] } };
function renderPanel() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={client}><McpHostToolsPanel /></QueryClientProvider>);
}

describe('McpHostToolsPanel', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(getMcpHostStatus).mockResolvedValue(status);
    vi.mocked(listMcpHostTools).mockResolvedValue([
      { name: 'forensics.list_data_sources', description: '数据源', inputSchema: { type: 'object' } },
      { name: 'forensics.list_plugin_modules', description: '插件', inputSchema: { type: 'object', properties: { dataSourceId: { type: 'string' } }, required: ['dataSourceId'] } },
    ]);
  });

  it('uses the backend tool catalog and sends source/plugin calls to the shared MCP execution route', async () => {
    vi.mocked(callMcpHostTool).mockResolvedValue({ success: true, data: [{ pluginId: 'plugin.test' }] });
    renderPanel();
    fireEvent.click(await screen.findByRole('button', { name: /forensics\.list_plugin_modules/ }));
    fireEvent.change(screen.getByRole('textbox', { name: 'forensics.list_plugin_modules 参数' }), { target: { value: '{"dataSourceId":"source-1"}' } });
    fireEvent.click(screen.getByRole('button', { name: '测试工具' }));
    await waitFor(() => expect(callMcpHostTool).toHaveBeenCalledWith('forensics.list_plugin_modules', { dataSourceId: 'source-1' }));
    expect(await screen.findByText(/plugin\.test/)).toBeInTheDocument();
  });

  it('saves a service stop and copies the actual listener address', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } });
    vi.mocked(setMcpHostSettings).mockResolvedValue({ ...status, running: false, settings: { enabled: false, disabledTools: [] } });
    renderPanel();
    await screen.findByText('运行中');
    fireEvent.click(screen.getByRole('button', { name: '复制连接地址' }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(status.endpoint));
    fireEvent.click(screen.getByRole('checkbox', { name: '启用本机 MCP 服务' }));
    await waitFor(() => expect(setMcpHostSettings).toHaveBeenCalledWith({ enabled: false, disabledTools: [] }, expect.anything()));
  });

  it('keeps a disabled tool listed and blocks both call controls', async () => {
    vi.mocked(getMcpHostStatus).mockResolvedValue({ ...status, settings: { enabled: true, disabledTools: ['forensics.list_data_sources'] } });
    renderPanel();
    fireEvent.click(await screen.findByRole('button', { name: /forensics\.list_data_sources/ }));
    expect(screen.getByRole('button', { name: '测试工具' })).toBeDisabled();
    expect(screen.getAllByTitle('使用当前参数测试')[0]).toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: '测试工具' }));
    expect(callMcpHostTool).not.toHaveBeenCalled();
  });

  it('shows a save failure without changing the confirmed tool policy', async () => {
    vi.mocked(setMcpHostSettings).mockRejectedValue(new Error('保存配置失败'));
    renderPanel();
    await screen.findByText('运行中');
    fireEvent.click(screen.getAllByTitle('禁用工具')[0]);
    expect(await screen.findByText('保存配置失败')).toBeInTheDocument();
    expect(screen.getAllByTitle('禁用工具')).toHaveLength(2);
  });
});
