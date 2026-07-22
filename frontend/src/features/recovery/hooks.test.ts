import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type {
  DataSourceSummary,
  DeletedFileRecovery,
  DeletedRecoveryPage,
} from '@/types/models';
import { useDeletedRecoveryModel } from './hooks';

const mocks = vi.hoisted(() => ({
  useCurrentCase: vi.fn(),
  listDeletedRecoveries: vi.fn(),
  runDeletedRecovery: vi.fn(),
  readDeletedRecoveryRange: vi.fn(),
  exportDeletedRecovery: vi.fn(),
  saveDialog: vi.fn(),
  toastSuccess: vi.fn(),
  toastError: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.useCurrentCase,
}));

vi.mock('@/lib/api/files', () => ({
  listDeletedRecoveries: mocks.listDeletedRecoveries,
  runDeletedRecovery: mocks.runDeletedRecovery,
  readDeletedRecoveryRange: mocks.readDeletedRecoveryRange,
  exportDeletedRecovery: mocks.exportDeletedRecovery,
}));

vi.mock('@/lib/platform/dialog', () => ({
  saveDialog: mocks.saveDialog,
}));

vi.mock('sonner', () => ({
  toast: {
    success: mocks.toastSuccess,
    error: mocks.toastError,
  },
}));

const recoveryId = `recovery:${'a'.repeat(64)}`;

const source: DataSourceSummary = {
  id: 'source-linux',
  name: 'Linux evidence',
  kind: 'e01',
  sourcePath: 'D:/evidence/linux.E01',
  importedAt: '2026-07-21T00:00:00Z',
  platform: 'linux',
  importState: 'ready',
  partitions: [
    {
      index: 1,
      name: 'boot',
      kindLabel: 'NTFS',
      status: 'ready',
      offset: 0,
      length: 1024,
      filesystem: 'ntfs',
    },
    {
      index: 2,
      name: 'root',
      kindLabel: 'XFS',
      status: 'ready',
      offset: 1024,
      length: 1024 * 1024,
      filesystem: 'xfs',
    },
  ],
};

const windowsSource: DataSourceSummary = {
  ...source,
  id: 'source-windows',
  name: 'Windows evidence',
  platform: 'windows',
};

function recovery(overrides: Partial<DeletedFileRecovery> = {}): DeletedFileRecovery {
  return {
    id: recoveryId,
    dataSourceId: source.id,
    partitionIndex: 2,
    filesystemType: 'ext4',
    inode: '42',
    originalPath: '/tmp/deleted.bin',
    entryType: 'file',
    declaredSize: 4,
    recoverableBytes: 4,
    completeness: 'partial',
    allocationState: 'free',
    recoveryMethod: 'ext4_jbd2_deleted_inode',
    confidence: 0.9,
    provenanceRanges: [{
      ordinal: 1,
      rangeRole: 'content',
      sourceKind: 'filesystem',
      logicalOffset: 0,
      sourceOffset: 4096,
      length: 4,
      allocationState: 'free',
      sha256: 'b'.repeat(64),
    }],
    warnings: [],
    ...overrides,
  };
}

function recoveryPage(recoveries: DeletedFileRecovery[]): DeletedRecoveryPage {
  return {
    scan: {
      id: 'scan-1',
      dataSourceId: source.id,
      partitionIndex: 2,
      filesystemType: 'xfs',
      parserVersion: 'xfs-log-v2',
      logKind: 'internal_log',
      snapshotIdentitySha256: 'c'.repeat(64),
      state: 'complete',
      transactionCount: 10,
      candidateCount: recoveries.length,
      warnings: [],
      startedAt: '2026-07-21T00:00:00Z',
      completedAt: '2026-07-21T00:00:01Z',
      issues: [],
    },
    recoveries,
    offset: 0,
    limit: 100,
    total: recoveries.length,
  };
}

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

describe('useDeletedRecoveryModel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.useCurrentCase.mockReturnValue({
      isSuccess: true,
      data: { id: 'case-1' },
    });
    mocks.saveDialog.mockResolvedValue('D:/exports/deleted.bin');
  });

  it('treats a missing persisted scan as an unscanned state', async () => {
    mocks.listDeletedRecoveries.mockRejectedValue({
      code: 'RECOVERY_SCAN_NOT_FOUND',
      message: 'scan not found',
      category: 'validation',
      recoverable: true,
    });

    const { result } = renderHook(() => useDeletedRecoveryModel(source, true), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.state).toBe('unscanned'));
    expect(result.current.error).toBeUndefined();
    expect(result.current.partitions.map((partition) => partition.index)).toEqual([2]);
  });

  it('queries only the selected source-local NTFS/EXT4/XFS partition', async () => {
    mocks.listDeletedRecoveries.mockResolvedValue(recoveryPage([]));

    const { result } = renderHook(() => useDeletedRecoveryModel(source, true), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.state).toBe('ready'));
    expect(mocks.listDeletedRecoveries).toHaveBeenCalledWith(source.id, 2, 0, 100);
  });

  it('selects NTFS and excludes Linux filesystems for a Windows source', async () => {
    mocks.listDeletedRecoveries.mockResolvedValue(recoveryPage([]));

    const { result } = renderHook(() => useDeletedRecoveryModel(windowsSource, true), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.state).toBe('ready'));
    expect(result.current.partitions.map((partition) => partition.index)).toEqual([1]);
    expect(mocks.listDeletedRecoveries).toHaveBeenCalledWith(windowsSource.id, 1, 0, 100);
  });

  it('reads bytes through the verified recovery range API', async () => {
    const item = recovery();
    mocks.listDeletedRecoveries.mockResolvedValue(recoveryPage([item]));
    mocks.readDeletedRecoveryRange.mockResolvedValue({
      recoveryId,
      offset: 0,
      bytesBase64: btoa('\0ABC'),
      bytesRead: 4,
      declaredSize: 4,
      eof: true,
      verifiedRangeOrdinals: [1],
    });
    const { result } = renderHook(() => useDeletedRecoveryModel(source, true), {
      wrapper: createWrapper(),
    });
    await waitFor(() => expect(result.current.state).toBe('ready'));

    act(() => result.current.selectRecovery(recoveryId));
    await waitFor(() => expect(result.current.selectedRangeOrdinal).toBe(1));
    act(() => result.current.readSelectedRange());

    await waitFor(() => expect(result.current.preview?.bytes).toEqual([0, 65, 66, 67]));
    expect(mocks.readDeletedRecoveryRange).toHaveBeenCalledWith(source.id, recoveryId, 0, 4);
  });

  it('exports only complete candidates through the platform save adapter', async () => {
    const item = recovery({ completeness: 'complete' });
    mocks.listDeletedRecoveries.mockResolvedValue(recoveryPage([item]));
    mocks.exportDeletedRecovery.mockResolvedValue({
      recoveryId,
      bytesWritten: 4,
      sha256: 'd'.repeat(64),
    });
    const { result } = renderHook(() => useDeletedRecoveryModel(source, true), {
      wrapper: createWrapper(),
    });
    await waitFor(() => expect(result.current.state).toBe('ready'));

    act(() => result.current.selectRecovery(recoveryId));
    act(() => result.current.exportSelected());

    await waitFor(() => expect(mocks.exportDeletedRecovery).toHaveBeenCalledTimes(1));
    expect(mocks.saveDialog).toHaveBeenCalledWith({ defaultPath: 'deleted.bin' });
    expect(mocks.exportDeletedRecovery).toHaveBeenCalledWith(
      source.id,
      recoveryId,
      'D:/exports/deleted.bin',
      false,
    );
  });

  it('runs a scan against the selected source partition', async () => {
    mocks.listDeletedRecoveries.mockRejectedValueOnce({
      code: 'RECOVERY_SCAN_NOT_FOUND',
      message: 'scan not found',
    }).mockResolvedValue(recoveryPage([]));
    mocks.runDeletedRecovery.mockResolvedValue({
      dataSourceId: source.id,
      scans: [],
      failures: [],
    });
    const { result } = renderHook(() => useDeletedRecoveryModel(source, true), {
      wrapper: createWrapper(),
    });
    await waitFor(() => expect(result.current.state).toBe('unscanned'));

    act(() => result.current.runScan());

    await waitFor(() => expect(mocks.runDeletedRecovery).toHaveBeenCalledWith(source.id, 2));
    await waitFor(() => expect(result.current.state).toBe('ready'));
  });
});
