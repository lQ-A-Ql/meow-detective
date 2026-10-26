import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { McpSectionView } from '@/features/mcp/components/McpSectionView';
import { McpHostToolsPanelView } from '@/features/mcp/components/McpHostToolsPanelView';
import i18n from '@/i18n';

describe('MCP host settings controls', () => {
  it('displays the host panel only while the section is expanded', () => {
    const panel = <McpHostToolsPanelView t={i18n.t}
      status={{ settings: { enabled: true, disabledTools: [] }, running: true, endpoint: 'http://127.0.0.1:3001/mcp' }}
      loading={false} copied={false} copyError={false} onEnable={() => {}} onCopy={() => {}}
      onRetry={() => {}} toolList={null} />;
    const { rerender } = render(<McpSectionView t={i18n.t} expanded onToggle={() => {}} hostToolList={panel} />);
    expect(screen.getByText('http://127.0.0.1:3001/mcp')).toBeVisible();
    rerender(<McpSectionView t={i18n.t} expanded={false} onToggle={() => {}} hostToolList={panel} />);
    expect(screen.queryByText('http://127.0.0.1:3001/mcp')).not.toBeInTheDocument();
  });

  it('copies the endpoint and disables the host through its controls', () => {
    let copies = 0;
    let enabled = true;
    render(<McpHostToolsPanelView t={i18n.t}
      status={{ settings: { enabled: true, disabledTools: [] }, running: true, endpoint: 'http://127.0.0.1:3001/mcp' }}
      loading={false} copied={false} copyError={false} onEnable={(value) => { enabled = value; }}
      onCopy={() => { copies += 1; }} onRetry={() => {}} toolList={null} />);
    const copy = screen.getByRole('button', { name: '复制连接地址' });
    expect(copy.textContent).toBe('');
    fireEvent.click(copy);
    expect(copies).toBe(1);
    fireEvent.click(screen.getByRole('checkbox', { name: '启用本机 MCP 服务' }));
    expect(enabled).toBe(false);
  });
});
