import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { apiClient } from '@/lib/api/client';
import { RegistryExtractionPanelContainer } from '@/features/analysis/containers/RegistryExtractionPanelContainer';
import { createRegistryBrowserMock } from '../../../mocks/registry-browser/client';
import fixture from '../../../mocks/registry-browser/data.json';
import type { RegistryBrowserClient } from '@/lib/api/registry-browser';

vi.mock('@/lib/api/client', () => ({ apiClient: { request: vi.fn() } }));

describe('native raw registry tab', () => {
  it('uses the registered request envelope and closes its session on leaving the tab', async () => {
    const waitOptions = { timeout: 10_000 };
    const browser = createRegistryBrowserMock();
    const close = vi.spyOn(browser, 'closeHive');
    const calls = new Map<string, keyof RegistryBrowserClient>([
      ['list_registry_hives', 'listHives'], ['open_registry_hive', 'openHive'], ['resolve_registry_key', 'resolveKey'],
      ['list_registry_subkeys', 'listSubkeys'], ['list_registry_values', 'listValues'], ['read_registry_value_range', 'readValueRange'], ['close_registry_hive', 'closeHive'],
    ]);
    vi.mocked(apiClient.request).mockImplementation(async (command, payload) => {
      if (command === 'get_current_case') return { id: fixture.caseId };
      if (command === 'get_data_sources') return [{ id: fixture.dataSourceId, platform: 'windows', importState: 'ready', importedAt: 'synthetic-v1' }];
      const method = calls.get(command);
      if (!method || !payload?.request) throw new Error(`unexpected command: ${command}`);
      return (browser[method] as (request: unknown) => Promise<unknown>)(payload.request);
    });
    const query = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
    render(<QueryClientProvider client={query}><RegistryExtractionPanelContainer dataSourceId={fixture.dataSourceId} /></QueryClientProvider>);
    fireEvent.mouseDown(screen.getByRole('tab', { name: '原始键值' }), { button: 0 });
    await screen.findByRole('treeitem', { name: 'Microsoft' }, waitOptions);
    fireEvent.change(screen.getByLabelText('Hive 内键路径'), { target: { value: 'Microsoft\\Windows\\CurrentVersion' } });
    fireEvent.submit(screen.getByLabelText('Hive 内键路径').closest('form')!);
    await screen.findByText('ProductName', undefined, waitOptions);
    fireEvent.click(screen.getByText('Sequence'));
    await within(screen.getByRole('region', { name: '所选值详情' }))
      .findByText('18446744073709551615', undefined, waitOptions);
    expect(apiClient.request).toHaveBeenCalledWith('read_registry_value_range', { request: expect.objectContaining({ length: 65536, offset: 0 }) });
    const opens = vi.mocked(apiClient.request).mock.calls.filter(([command]) => command === 'open_registry_hive').length;
    await act(async () => { await query.refetchQueries({ queryKey: ['case', 'data-sources'] }); });
    expect(vi.mocked(apiClient.request).mock.calls.filter(([command]) => command === 'open_registry_hive')).toHaveLength(opens);
    expect(close).not.toHaveBeenCalled();
    expect(within(screen.getByRole('region', { name: '所选值详情' })).getByText('18446744073709551615')).toBeInTheDocument();
    fireEvent.mouseDown(screen.getByRole('tab', { name: '用户账户' }), { button: 0 });
    await waitFor(() => expect(close).toHaveBeenCalled(), waitOptions);
    expect(screen.queryByRole('tree')).not.toBeInTheDocument();
  });
});
