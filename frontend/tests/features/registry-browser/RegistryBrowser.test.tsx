import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { RegistryBrowserContainer } from '@/features/registry-browser/containers/RegistryBrowserContainer';
import { createRegistryBrowserMock } from '../../../mocks/registry-browser/client';
import fixture from '../../../mocks/registry-browser/data.json';
import type { RegistryKeyMetadata } from '@/types/registryBrowser';

const context = { caseId: fixture.caseId, dataSourceId: fixture.dataSourceId, sourceRevision: 'synthetic-v1' };
function preview(client = createRegistryBrowserMock()) {
  const query = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
  return render(<QueryClientProvider client={query}><RegistryBrowserContainer client={client} context={context}
    initialKeyPath="Microsoft\\Windows\\CurrentVersion" /></QueryClientProvider>);
}

describe('registry browser P0 workspace', () => {
  it('clears expired bytes and rediscovers the source revision before reopening', async () => {
    const base = createRegistryBrowserMock();
    let revision = 'synthetic-v1';
    let expire = true;
    const client = { ...base,
      listHives: vi.fn(async (request) => ({ ...await base.listHives(request), sourceRevision: revision })),
      openHive: vi.fn(async (request) => { const session = await base.openHive(request); return { ...session, source: { ...session.source, sourceRevision: revision } }; }),
      readValueRange: vi.fn(async (request) => {
        if (expire) { expire = false; revision = 'synthetic-v2'; throw { code: 'expiredSession' }; }
        return base.readValueRange(request);
      }),
    } satisfies typeof base;
    preview(client); await screen.findByText('ProductName');
    fireEvent.click(screen.getByText('Sequence'));
    await screen.findByText('浏览会话已失效，请重新打开配置单元');
    expect(screen.queryByText('ProductName')).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: '重试' }));
    await screen.findByRole('treeitem', { name: 'Microsoft' });
    expect(client.openHive).toHaveBeenCalledTimes(2);
    expect(client.listHives).toHaveBeenCalledTimes(2);
  });
  it('navigates from a tree to real default values and an empty key without fabricating rows', async () => {
    preview();
    await screen.findByText('ProductName');
    expect(screen.getByRole('treeitem', { name: 'CurrentVersion' })).not.toHaveAttribute('aria-expanded');
    expect(screen.getByText('(默认)')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('treeitem', { name: 'EmptyKey' }));
    await screen.findByText('此键没有值');
    expect(screen.queryByText('ProductName')).not.toBeInTheDocument();
    expect(screen.queryByText('(默认)')).not.toBeInTheDocument();
  });

  it('retains distinct user hives and clears the previous user value on switch', async () => {
    preview(); await screen.findByText('ProductName');
    fireEvent.click(screen.getByRole('treeitem', { name: 'NTUSER.DAT · Synthetic-A' }));
    await screen.findByText('ProfileLabel');
    const values = screen.getByRole('region', { name: '当前键的值' });
    expect(within(values).getByText('Synthetic-A')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('treeitem', { name: 'NTUSER.DAT · Synthetic-B' }));
    await waitFor(() => expect(within(values).getByText('Synthetic-B')).toBeInTheDocument());
    expect(within(values).queryByText('Synthetic-A')).not.toBeInTheDocument();
  });

  it('loads a bounded range on value selection and preserves u64 text exactly', async () => {
    const client = createRegistryBrowserMock();
    const read = vi.spyOn(client, 'readValueRange');
    preview(client); await screen.findByText('ProductName');
    expect(read).not.toHaveBeenCalled();
    fireEvent.click(screen.getByText('Sequence'));
    const detail = screen.getByRole('region', { name: '所选值详情' });
    await waitFor(() => expect(within(detail).getByText('18446744073709551615')).toBeInTheDocument());
    expect(read.mock.calls[0][0].length).toBe(65536);
    fireEvent.click(screen.getByText('LargePayload'));
    await waitFor(() => expect(screen.getByRole('button', { name: '下一段字节' })).toBeEnabled());
    fireEvent.click(screen.getByRole('button', { name: '下一段字节' }));
    await waitFor(() => expect(read).toHaveBeenLastCalledWith(expect.objectContaining({ valueId: 'large', offset: 65536, length: 65536 })));
    await waitFor(() => expect(screen.getByRole('button', { name: '下一段字节' })).toBeDisabled());
  });

  it('shows partial coverage and a corrupt key as different states', async () => {
    preview(); await screen.findByText('ProductName');
    fireEvent.click(screen.getByRole('treeitem', { name: 'PartialValues' }));
    await screen.findByText('部分值未能读取，不能视为完整结果');
    expect(screen.getByText('UnknownType')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('treeitem', { name: 'DamagedBranch' }));
    await screen.findByText('当前键损坏，其他分支仍可浏览');
    expect(screen.queryByText('UnknownType')).not.toBeInTheDocument();
  });

  it('moves keyboard focus without changing selection and ignores path input keys', async () => {
    preview(); await screen.findByText('ProductName');
    const current = screen.getByRole('treeitem', { name: 'CurrentVersion' });
    current.focus(); fireEvent.keyDown(current, { key: 'ArrowLeft' });
    expect(screen.getByRole('treeitem', { name: 'Windows' })).toHaveFocus();
    expect(current).toHaveAttribute('aria-selected', 'true');
    fireEvent.keyDown(screen.getByRole('treeitem', { name: 'Windows' }), { key: 'ArrowRight' });
    expect(current).toHaveFocus();
    const input = screen.getByRole('textbox', { name: 'Hive 内键路径' });
    input.focus(); fireEvent.keyDown(input, { key: 'ArrowDown' });
    expect(input).toHaveFocus();
  });

  it('rejects a late path lookup after another key is selected and resets its pending state', async () => {
    const client = createRegistryBrowserMock();
    let sessionId = '';
    const open = client.openHive;
    client.openHive = async (request) => { const session = await open(request); sessionId = session.sessionId; return session; };
    preview(client); await screen.findByText('ProductName');
    const empty = await client.resolveKey({ sessionId, keyPath: 'EmptyKey' });
    let resolve!: (key: RegistryKeyMetadata) => void;
    const pending = new Promise<RegistryKeyMetadata>((done) => { resolve = done; });
    vi.spyOn(client, 'resolveKey').mockReturnValueOnce(pending);
    const input = screen.getByRole('textbox', { name: 'Hive 内键路径' });
    fireEvent.change(input, { target: { value: 'EmptyKey' } });
    fireEvent.click(screen.getByRole('button', { name: '定位键' }));
    expect(screen.getByRole('button', { name: '定位键' })).toBeDisabled();
    fireEvent.click(screen.getByRole('treeitem', { name: 'PartialValues' }));
    await screen.findByText('UnknownType');
    await act(async () => { resolve(empty); await pending; });
    expect(input).toHaveValue('PartialValues');
    expect(screen.getByRole('button', { name: '定位键' })).toBeEnabled();
    expect(screen.queryByText('此键没有值')).not.toBeInTheDocument();
  });

  it('renders source absence and source read failure independently', async () => {
    const empty = preview(createRegistryBrowserMock('emptySource'));
    await screen.findByText('此数据源未发现配置单元');
    empty.unmount();
    preview(createRegistryBrowserMock('failedSource'));
    await waitFor(() => expect(screen.getByRole('alert')).toHaveTextContent('来源暂不可读'));
    expect(screen.getByRole('button', { name: '重试' })).toBeInTheDocument();
  });
});
