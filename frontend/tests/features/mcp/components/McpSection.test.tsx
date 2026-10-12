import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { McpSectionController, type McpSectionControllerProps } from '@/features/mcp/containers/McpSectionController';
import { McpHostToolsPanelView } from '@/features/mcp/components/McpHostToolsPanelView';
import i18n from '@/i18n';

function props(): McpSectionControllerProps {
  return {
    servers: [], selectedServerId: null, loading: false, error: null,
    onAdd: vi.fn().mockResolvedValue(undefined), onConnect: vi.fn(), onDisconnect: vi.fn(),
    onRemove: vi.fn(), onSelect: vi.fn(), testConnection: vi.fn().mockResolvedValue({ success: true }),
  };
}

describe('MCP settings controls', () => {
  it('adds a server using labelled form fields and preserves permission defaults', async () => {
    const model = props();
    render(<McpSectionController {...model} />);
    fireEvent.click(screen.getByRole('button', { name: '添加服务器' }));
    fireEvent.change(screen.getByLabelText('服务器名称 *'), { target: { value: 'Mock reader' } });
    fireEvent.change(screen.getByLabelText('SSE URL *'), { target: { value: 'http://127.0.0.1:3010/sse' } });
    fireEvent.click(screen.getByRole('button', { name: '添加' }));
    await waitFor(() => expect(model.onAdd).toHaveBeenCalledWith(expect.objectContaining({
      name: 'Mock reader', transportType: 'sse', url: 'http://127.0.0.1:3010/sse', enabled: true,
      autoConnect: false, permissions: expect.objectContaining({
        resourceAccess: 'readOnly', promptAccess: 'readOnly', toolAccess: 'allowAll', networkPolicy: 'localhostOnly',
      }),
    })));
    await waitFor(() => expect(screen.queryByRole('dialog')).not.toBeInTheDocument());
  });

  it('closes the dialog through the shared close action', () => {
    render(<McpSectionController {...props()} />);
    fireEvent.click(screen.getByRole('button', { name: '添加服务器' }));
    fireEvent.click(screen.getByRole('button', { name: '关闭' }));
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
  });

  it('keeps the resource and tool panes keyboard selectable', async () => {
    render(<McpSectionController {...props()} selectedServerId="mock-local"
      resourceList={<div>Mock resources</div>} toolList={<div>Mock tools</div>} />);
    const resource = screen.getByRole('tab', { name: '暴露的 Resources' });
    const tool = screen.getByRole('tab', { name: '可用 Tools' });
    expect(screen.getByText('Mock resources')).toBeVisible();
    tool.focus();
    fireEvent.keyDown(tool, { key: 'Enter' });
    await waitFor(() => expect(screen.getByText('Mock tools')).toBeVisible());
    expect(resource).toHaveAttribute('aria-selected', 'false');
    expect(tool).toHaveAttribute('aria-selected', 'true');
  });

  it('copies the endpoint and toggles the host without text inside its action button', () => {
    const onCopy = vi.fn();
    const onEnable = vi.fn();
    render(<McpHostToolsPanelView t={i18n.t} status={{ settings: { enabled: true, disabledTools: [] },
      running: true, endpoint: 'http://127.0.0.1:3001/mcp' }} loading={false} copied={false} copyError={false}
      onEnable={onEnable} onCopy={onCopy} onRetry={vi.fn()} toolList={null} />);
    const copy = screen.getByRole('button', { name: '复制连接地址' });
    expect(copy.textContent).toBe('');
    fireEvent.click(copy);
    expect(onCopy).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole('checkbox', { name: '启用本机 MCP 服务' }));
    expect(onEnable).toHaveBeenCalledWith(false);
  });
});
