import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { createElement, type ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useInfrastructureHostFacts } from '@/features/infrastructure/hooks/useInfrastructureHostFacts';

const mocks = vi.hoisted(() => ({
  currentCase: vi.fn(),
  dataSources: vi.fn(),
  artifactSummary: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.currentCase,
  useDataSources: mocks.dataSources,
}));

vi.mock('@/lib/api/analysis', () => ({
  getLinuxArtifactSummary: mocks.artifactSummary,
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
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client }, children);
  };
}

const source = (overrides: Record<string, unknown> = {}) => ({
  id: 'source-1',
  name: 'node-a image',
  kind: 'e01',
  platform: 'linux',
  importState: 'ready',
  sourcePath: 'E:\\node-a.E01',
  importedAt: '2026-08-06T00:00:00Z',
  partitions: [],
  fileCount: 10,
  evidenceSize: 1024,
  ...overrides,
});

describe('useInfrastructureHostFacts', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.currentCase.mockReturnValue(queryResult({ id: 'case-1', name: 'cluster case' }));
    mocks.dataSources.mockReturnValue(queryResult([source()]));
    mocks.artifactSummary.mockResolvedValue({});
  });

  it('reads host facts only for ready Linux sources and keeps the preferred OS name', async () => {
    mocks.dataSources.mockReturnValue(
      queryResult([
        source(),
        source({ id: 'source-2', importState: 'importing' }),
        source({ id: 'source-3', platform: 'windows' }),
      ]),
    );
    mocks.artifactSummary.mockResolvedValue({
      systemInfo: {
        hostname: 'node-a',
        osPrettyName: 'openEuler 24.03 LTS',
        osId: 'openeuler',
        osVersionId: '24.03',
        kernelVersions: ['6.6.1-1', '6.6.0-1'],
        accountCount: 2,
        userAccountCount: 1,
        lockedAccountCount: 0,
      },
    });

    const { result } = renderHook(() => useInfrastructureHostFacts(), { wrapper: createWrapper() });

    await waitFor(() => expect(result.current.get('source-1')?.hostname).toBe('node-a'));
    expect(result.current.get('source-1')).toEqual({
      hostname: 'node-a',
      operatingSystem: 'openEuler 24.03 LTS',
      operatingSystemVersion: '24.03',
      kernelVersion: '6.6.1-1',
    });
    expect(result.current.has('source-2')).toBe(false);
    expect(result.current.has('source-3')).toBe(false);
    expect(mocks.artifactSummary).toHaveBeenCalledTimes(1);
    expect(mocks.artifactSummary).toHaveBeenCalledWith({ dataSourceId: 'source-1', offset: 0, limit: 1 });
  });

  it('falls back to the OS id and leaves absent facts undefined', async () => {
    mocks.artifactSummary.mockResolvedValue({
      systemInfo: { osId: 'alpine', accountCount: 0, userAccountCount: 0, lockedAccountCount: 0 },
    });

    const { result } = renderHook(() => useInfrastructureHostFacts(), { wrapper: createWrapper() });

    await waitFor(() => expect(result.current.get('source-1')?.operatingSystem).toBe('alpine'));
    expect(result.current.get('source-1')).toEqual({
      hostname: undefined,
      operatingSystem: 'alpine',
      operatingSystemVersion: undefined,
      kernelVersion: undefined,
    });
  });

  it('does not fabricate host facts when the Linux summary has no system info', async () => {
    mocks.artifactSummary.mockResolvedValue({ status: 'parsed', systemInfo: undefined });

    const { result } = renderHook(() => useInfrastructureHostFacts(), { wrapper: createWrapper() });

    await waitFor(() => expect(mocks.artifactSummary).toHaveBeenCalledTimes(1));
    expect(result.current.get('source-1')).toStrictEqual({
      hostname: undefined,
      operatingSystem: undefined,
      operatingSystemVersion: undefined,
      kernelVersion: undefined,
    });
  });

  it('does not query summaries without an active case but still keys the ready sources', () => {
    mocks.currentCase.mockReturnValue(queryResult(undefined));

    const { result } = renderHook(() => useInfrastructureHostFacts(), { wrapper: createWrapper() });

    expect(result.current.size).toBe(1);
    expect(result.current.get('source-1')?.hostname).toBeUndefined();
    expect(mocks.artifactSummary).not.toHaveBeenCalled();
  });

  it('returns an empty map when no data source is loaded', () => {
    mocks.dataSources.mockReturnValue(queryResult([]));

    const { result } = renderHook(() => useInfrastructureHostFacts(), { wrapper: createWrapper() });

    expect(result.current.size).toBe(0);
    expect(mocks.artifactSummary).not.toHaveBeenCalled();
  });

  it('returns an empty map when the data source query has no payload yet', () => {
    mocks.dataSources.mockReturnValue(queryResult(undefined));

    const { result } = renderHook(() => useInfrastructureHostFacts(), { wrapper: createWrapper() });

    expect(result.current.size).toBe(0);
    expect(mocks.artifactSummary).not.toHaveBeenCalled();
  });
});
