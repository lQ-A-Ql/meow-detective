import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { RegistryHardwareContainer } from '@/features/registry-hardware/containers/RegistryHardwareContainer';
import { formatHardwareBytes } from '@/features/registry-hardware/hardware-model';
import type { RegistryHardwareClient } from '@/lib/api/registry-hardware';
import type { RegistryBrowserSource, RegistryHiveSession } from '@/types/registryBrowser';
import type { RegistryHardwareDevice } from '@/types/registryHardware';
import { createRegistryBrowserMock } from '../../../mocks/registry-browser/client';
import fixture from '../../../mocks/registry-browser/data.json';

const context = { caseId: fixture.caseId, dataSourceId: fixture.dataSourceId, sourceRevision: 'synthetic-v1' };
const keyPath = 'ControlSet001\\Services\\mssmbios\\Data';
const records: RegistryHardwareDevice[] = [
  { kind: 'cpu', deviceId: 'cpu-1', name: 'Fixture CPU', manufacturer: 'CPU Vendor',
    properties: { coreCount: '8', threadCount: '16', maxSpeedMhz: '4200' }, keyPath, valueName: 'SMBiosData', sourceOffset: 8 },
  { kind: 'gpu', deviceId: 'gpu-1', name: 'Fixture GPU', properties: { capacityBytes: '12884901888' }, keyPath: 'ControlSet001\\Control\\Class\\display\\0000' },
  { kind: 'ram', deviceId: 'ram-1', name: 'DIMM_A1', properties: { capacityBytes: '17179869184', memoryType: 'DDR5', configuredSpeedMtS: '6000' }, keyPath },
  { kind: 'ram', deviceId: 'ram-2', name: 'DIMM_A2', properties: {}, keyPath },
];

function hardwareClient(): RegistryHardwareClient {
  const base = createRegistryBrowserMock();
  const sessions = new Map<string, RegistryBrowserSource>();
  return {
    ...base,
    listHives: vi.fn(async (request) => {
      const page = await base.listHives(request);
      return { ...page, hives: [{ ...page.hives[0], name: 'SYSTEM', evidencePath: '[P0]/Windows/System32/config/SYSTEM' }], nextCursor: undefined };
    }),
    openHive: vi.fn(async (request) => {
      const opened = await base.openHive(request); sessions.set(opened.sessionId, opened.source);
      return { ...opened, hive: { ...opened.hive, name: 'SYSTEM', evidencePath: '[P0]/Windows/System32/config/SYSTEM' } };
    }),
    readHardware: vi.fn(async (request) => ({ source: sessions.get(request.sessionId)!, devices: records, warnings: [] })),
    closeHive: vi.fn(base.closeHive),
  };
}

function preview(client: RegistryHardwareClient) {
  const query = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
  return render(<QueryClientProvider client={query}><RegistryHardwareContainer client={client} context={context} /></QueryClientProvider>);
}

describe('registry system hardware', () => {
  it('reads CPU/GPU/RAM on demand and excludes unknown capacities from the total', async () => {
    const client = hardwareClient();
    const view = preview(client);
    await screen.findByText('Fixture CPU');
    expect(screen.getByText('Fixture GPU')).toBeInTheDocument();
    expect(screen.getByText('已识别内存模块合计：16 GiB')).toBeInTheDocument();
    expect(screen.getByText('另有 1 条内存记录的容量未知，未计入合计。')).toBeInTheDocument();
    expect(screen.getByText('6000 MT/s')).toBeInTheDocument();
    expect(screen.getByText('12 GiB')).toBeInTheDocument();
    expect(screen.getByText('核心数')).toBeInTheDocument();
    expect(screen.getByText(`${keyPath}\\SMBiosData`)).toBeInTheDocument();
    expect(client.openHive).toHaveBeenCalledWith(expect.objectContaining({ view: 'base' }));
    view.unmount();
    await waitFor(() => expect(client.closeHive).toHaveBeenCalledTimes(1));
  });

  it('rejects hardware returned for a different source revision', async () => {
    const client = hardwareClient();
    const read = client.readHardware;
    client.readHardware = vi.fn(async (request) => {
      const result = await read(request); return { ...result, source: { ...result.source, sourceRevision: 'foreign-revision' } };
    });
    preview(client);
    await screen.findByText('浏览会话已失效，请重新打开配置单元');
    expect(screen.queryByText('Fixture GPU')).not.toBeInTheDocument();
  });

  it('releases a late session when the hardware tab has already unmounted', async () => {
    const client = hardwareClient();
    const open = client.openHive;
    let finish: ((session: RegistryHiveSession) => void) | undefined;
    let opened: RegistryHiveSession | undefined;
    client.openHive = vi.fn(async (request) => {
      opened = await open(request); return new Promise<RegistryHiveSession>((resolve) => { finish = resolve; });
    });
    const view = preview(client);
    await waitFor(() => expect(finish).toBeDefined());
    view.unmount();
    await act(async () => finish!(opened!));
    await waitFor(() => expect(client.closeHive).toHaveBeenCalledWith({ sessionId: opened!.sessionId }));
    expect(client.readHardware).not.toHaveBeenCalled();
  });

  it('formats unsigned 64-bit capacities without a lossy Number conversion', () => {
    expect(formatHardwareBytes('18446744073709551615')).toBe('16 EiB');
    expect(formatHardwareBytes('17179869184')).toBe('16 GiB');
    expect(formatHardwareBytes(undefined)).toBeUndefined();
    expect(formatHardwareBytes('-1')).toBeUndefined();
  });
});
