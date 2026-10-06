import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { McpServerItem } from '@/features/mcp/components/McpServerItem';

const server = {
  id: 'srv-1',
  name: '本地 MCP',
  transportType: 'stdio' as const,
  command: 'node',
  enabled: true,
  connected: false,
  hasResources: true,
  hasTools: true,
  hasPrompts: false,
};

describe('McpServerItem', () => {
  it('selects from the server name control and confirms removal', () => {
    const onSelect = vi.fn();
    const onRemove = vi.fn();
    render(<McpServerItem server={server} isSelected={false} onConnect={vi.fn()} onDisconnect={vi.fn()} onRemove={onRemove} onSelect={onSelect} />);

    fireEvent.click(screen.getByRole('button', { name: /本地 MCP/ }));
    expect(onSelect).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByTitle('删除'));
    expect(screen.getByText('删除 MCP 服务器')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '删除' }));
    expect(onRemove).toHaveBeenCalledTimes(1);
  });
});
