import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: ({ count, estimateSize }: { count: number; estimateSize: () => number }) => ({
    getTotalSize: () => count * estimateSize(),
    getVirtualItems: () => Array.from({ length: count }, (_, index) => ({ index, key: index, size: estimateSize(), start: index * estimateSize() })),
    scrollToOffset: vi.fn(),
  }),
}));

const getDataSourcesMock = vi.hoisted(() => vi.fn());
const listPluginModulesMock = vi.hoisted(() => vi.fn());
vi.mock('@/lib/api/case', () => ({ getDataSources: getDataSourcesMock }));
vi.mock('@/lib/api/analysis', () => ({ listPluginModules: listPluginModulesMock }));

import { McpHostToolsPanel } from '@/features/mcp/components/McpHostToolsPanel';

describe('McpHostToolsPanel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('lists sanitized data source information without exposing host paths', async () => {
    getDataSourcesMock.mockResolvedValueOnce([{
      id: 'ds-1', name: 'Evidence', kind: 'e01', platform: 'windows', importedAt: '2026-10-06T00:00:00Z',
      sourcePath: 'C:/private/evidence.E01', fileCount: 12, partitions: [],
    }]);
    render(<McpHostToolsPanel />);

    fireEvent.click(screen.getByRole('button', { name: /forensics\.list_data_sources/ }));
    fireEvent.click(screen.getByRole('button', { name: '测试工具' }));

    await waitFor(() => expect(getDataSourcesMock).toHaveBeenCalledTimes(1));
    expect(screen.getByText(/"id": "ds-1"/)).toBeInTheDocument();
    expect(screen.queryByText('C:/private/evidence.E01')).not.toBeInTheDocument();
  });

  it('passes dataSourceId to plugin module lookup', async () => {
    listPluginModulesMock.mockResolvedValueOnce([{ pluginId: 'plugin.test', displayName: 'Test Plugin', totalCount: 2, families: [], warnings: [] }]);
    render(<McpHostToolsPanel />);

    fireEvent.click(screen.getByRole('button', { name: /forensics\.list_plugin_modules/ }));
    fireEvent.change(screen.getByRole('textbox', { name: 'forensics.list_plugin_modules 参数' }), { target: { value: '{"dataSourceId":"ds-1"}' } });
    fireEvent.click(screen.getByRole('button', { name: '测试工具' }));

    await waitFor(() => expect(listPluginModulesMock).toHaveBeenCalledWith('ds-1'));
    expect(screen.getByText(/plugin\.test/)).toBeInTheDocument();
  });
});
