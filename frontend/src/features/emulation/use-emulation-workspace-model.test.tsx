import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { createElement, type ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useEmulationWorkspaceModel } from './use-emulation-workspace-model';

const mocks = vi.hoisted(() => ({
  currentCase: vi.fn(),
  dataSources: vi.fn(),
  listSessions: vi.fn(),
  preflight: vi.fn(),
  prepare: vi.fn(),
  launch: vi.fn(),
  release: vi.fn(),
  openDialog: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.currentCase,
  useDataSources: mocks.dataSources,
}));

vi.mock('@/lib/api/emulation', () => ({
  getEmulationPreflight: mocks.preflight,
  listEmulationSessions: mocks.listSessions,
  prepareEmulation: mocks.prepare,
  launchEmulation: mocks.launch,
  releaseEmulation: mocks.release,
}));

vi.mock('@/lib/platform/dialog', () => ({
  openDialog: mocks.openDialog,
  singleDialogPath: (value: unknown) => typeof value === 'string' ? value : null,
}));

function queryResult(data: unknown) {
  return {
    data,
    error: null,
    isFetching: false,
    isLoading: false,
    isSuccess: true,
    refetch: vi.fn().mockResolvedValue({ data }),
  };
}

function createWrapper() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client }, children);
  };
}

describe('useEmulationWorkspaceModel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.currentCase.mockReturnValue(queryResult({ id: 'case-1', name: '仿真案件' }));
    mocks.dataSources.mockReturnValue(queryResult([{
      id: 'source-1',
      name: '早起王的PC镜像',
      kind: 'e01',
      platform: 'windows',
      importState: 'ready',
      sourceHash: 'a'.repeat(64),
      sourcePath: 'E:\\evidence.E01',
      importedAt: '2026-08-06T00:00:00Z',
      partitions: [],
    }]));
    mocks.listSessions.mockResolvedValue([]);
    mocks.preflight.mockResolvedValue({
      dataSourceId: 'source-1',
      installs: [],
      recommendedBootRoute: 'directSystem',
    });
    mocks.prepare.mockResolvedValue({ sessionId: 'emulation-1' });
    mocks.launch.mockResolvedValue({ sessionId: 'emulation-1', state: 'running' });
    mocks.release.mockResolvedValue({ sessionId: 'emulation-1', state: 'released' });
  });

  it('owns direct-boot confirmation and sends explicit authorization after approval', async () => {
    const confirm = vi.spyOn(window, 'confirm').mockReturnValue(true);
    const { result } = renderHook(() => useEmulationWorkspaceModel(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.selectedSourceId).toBe('source-1'));

    await act(async () => result.current.start());

    expect(confirm).toHaveBeenCalledOnce();
    expect(mocks.prepare).toHaveBeenCalledWith({
      dataSourceId: 'source-1',
      recoveryIsoPath: undefined,
      allowDirectBoot: true,
      options: { network: false, clipboard: false, timeSync: false },
    });
    expect(mocks.launch).toHaveBeenCalledWith('emulation-1');
  });

  it('owns the selected PE path and launches it without direct-boot confirmation', async () => {
    const confirm = vi.spyOn(window, 'confirm');
    mocks.openDialog.mockResolvedValue('C:\\Tools\\WinPE.iso');
    const { result } = renderHook(() => useEmulationWorkspaceModel(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.selectedSourceId).toBe('source-1'));

    await act(async () => result.current.pickRecoveryIso());
    expect(result.current.recoveryIsoPath).toBe('C:\\Tools\\WinPE.iso');
    await act(async () => result.current.start());

    expect(confirm).not.toHaveBeenCalled();
    expect(mocks.prepare).toHaveBeenCalledWith({
      dataSourceId: 'source-1',
      recoveryIsoPath: 'C:\\Tools\\WinPE.iso',
      allowDirectBoot: false,
      options: { network: false, clipboard: false, timeSync: false },
    });
  });

  it('does not prepare a session when direct boot is declined', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(false);
    const { result } = renderHook(() => useEmulationWorkspaceModel(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.selectedSourceId).toBe('source-1'));

    await act(async () => result.current.start());

    expect(mocks.prepare).not.toHaveBeenCalled();
    expect(mocks.launch).not.toHaveBeenCalled();
  });
});
