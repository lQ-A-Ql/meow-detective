import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: ({ count, estimateSize }: { count: number; estimateSize: () => number }) => ({
    getTotalSize: () => count * estimateSize(),
    getVirtualItems: () => Array.from({ length: count }, (_, index) => ({ index, key: index, size: estimateSize(), start: index * estimateSize() })),
    scrollToOffset: vi.fn(),
  }),
}));

import { McpToolList } from '@/features/mcp/components/McpToolList';

describe('McpToolList', () => {
  it('filters tools and sends edited JSON arguments', async () => {
    const onTestTool = vi.fn().mockResolvedValue({ success: true, data: { ok: true } });
    render(<McpToolList
      tools={[
        { name: 'queryTimeline', description: 'Query timeline', inputSchema: { type: 'object', properties: { limit: { type: 'number' } }, required: ['limit'] } },
        { name: 'searchFiles', description: 'Search files', inputSchema: { type: 'object' } },
      ]}
      loading={false}
      onRefresh={vi.fn()}
      onTestTool={onTestTool}
    />);

    expect(screen.getByText('queryTimeline')).toBeInTheDocument();
    fireEvent.change(screen.getByPlaceholderText('搜索工具名称或描述'), { target: { value: 'search' } });
    expect(screen.getByText('searchFiles')).toBeInTheDocument();
    expect(screen.queryByText('queryTimeline')).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole('button', { name: /searchFiles/ }));
    const editor = screen.getByRole('textbox', { name: 'searchFiles 参数' });
    fireEvent.change(editor, { target: { value: '{"path":"/evidence"}' } });
    fireEvent.click(screen.getByRole('button', { name: '测试工具' }));

    await waitFor(() => expect(onTestTool).toHaveBeenCalledWith('searchFiles', { path: '/evidence' }));
    expect(screen.getByText(/"ok": true/)).toBeInTheDocument();
  });

  it('shows invalid JSON without calling the tool', async () => {
    const onTestTool = vi.fn();
    render(<McpToolList tools={[{ name: 'lookup', description: '', inputSchema: { type: 'object' } }]} loading={false} onRefresh={vi.fn()} onTestTool={onTestTool} />);
    fireEvent.click(screen.getByRole('button', { name: /lookup/ }));
    fireEvent.change(screen.getByRole('textbox', { name: 'lookup 参数' }), { target: { value: '{broken' } });
    fireEvent.click(screen.getByRole('button', { name: '测试工具' }));

    expect(screen.getByText('参数必须是有效的 JSON。')).toBeInTheDocument();
    expect(onTestTool).not.toHaveBeenCalled();
  });
});
