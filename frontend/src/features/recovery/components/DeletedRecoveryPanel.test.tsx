import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type {
  DeletedFileRecovery,
  DeletedRecoveryPage,
  RecoveryProvenanceRange,
} from '@/types/models';
import { DeletedRecoveryPanel } from './DeletedRecoveryPanel';
import type { DeletedRecoveryViewModel } from '../types';

const recoveryId = `recovery:${'a'.repeat(64)}`;

function candidate(
  overrides: Partial<DeletedFileRecovery> = {},
): DeletedFileRecovery {
  return {
    id: recoveryId,
    dataSourceId: 'source-linux',
    partitionIndex: 2,
    filesystemType: 'ext4',
    inode: '42',
    originalPath: '/var/tmp/deleted.txt',
    entryType: 'file',
    declaredSize: 4096,
    recoverableBytes: 0,
    completeness: 'metadata_only',
    allocationState: 'unverified',
    recoveryMethod: 'ext4_jbd2_deleted_inode',
    confidence: 0.82,
    provenanceRanges: [],
    warnings: [],
    ...overrides,
  };
}

function page(recoveries: DeletedFileRecovery[]): DeletedRecoveryPage {
  return {
    scan: {
      id: 'scan-1',
      dataSourceId: 'source-linux',
      partitionIndex: 2,
      filesystemType: 'ext4',
      parserVersion: 'ext4-jbd2-v1',
      logKind: 'internal_journal',
      snapshotIdentitySha256: 'b'.repeat(64),
      state: 'complete',
      transactionCount: 12,
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

function model(overrides: Partial<DeletedRecoveryViewModel> = {}): DeletedRecoveryViewModel {
  return {
    partitions: [{
      index: 2,
      name: 'root',
      kindLabel: 'Linux filesystem',
      status: 'ready',
      offset: 0,
      length: 1024 * 1024,
      filesystem: 'ext4',
    }],
    selectedPartitionIndex: 2,
    selectPartition: vi.fn(),
    state: 'unscanned',
    recoveries: [],
    total: 0,
    failures: [],
    selectRecovery: vi.fn(),
    contentRanges: [],
    selectRange: vi.fn(),
    hashQuery: '',
    hashQueryValid: false,
    setHashQuery: vi.fn(),
    runHashSearch: vi.fn(),
    clearHashSearch: vi.fn(),
    scanning: false,
    hashSearching: false,
    reading: false,
    exporting: false,
    runScan: vi.fn(),
    readSelectedRange: vi.fn(),
    exportSelected: vi.fn(),
    hasPreviousPage: false,
    hasNextPage: false,
    previousPage: vi.fn(),
    nextPage: vi.fn(),
    ...overrides,
  };
}

describe('DeletedRecoveryPanel', () => {
  it('reports unsupported when the source has no NTFS/EXT4/XFS partition', () => {
    render(<DeletedRecoveryPanel model={model({ partitions: [], state: 'unsupported' })} />);

    expect(screen.getByText('当前数据源没有可执行删除恢复的 NTFS/EXT4/XFS 分区')).toBeDefined();
  });

  it('starts a real backend scan from the unscanned state', () => {
    const runScan = vi.fn();
    render(<DeletedRecoveryPanel model={model({ runScan })} />);

    fireEvent.click(screen.getByRole('button', { name: '开始扫描' }));

    expect(runScan).toHaveBeenCalledTimes(1);
    expect(screen.getByText('当前分区尚未执行删除恢复扫描')).toBeDefined();
  });

  it('keeps metadata-only XFS candidates non-readable and non-exportable', () => {
    const recovery = candidate({ filesystemType: 'xfs' });
    const resultPage = page([recovery]);
    render(<DeletedRecoveryPanel model={model({
      state: 'ready',
      page: resultPage,
      recoveries: [recovery],
      total: 1,
      selectedRecovery: recovery,
      selectedRecoveryId: recovery.id,
    })} />);

    expect(screen.getByText('该候选没有可读取的已验证内容区间')).toBeDefined();
    expect(screen.getByRole('button', { name: '导出完整恢复文件' }).hasAttribute('disabled')).toBe(true);
  });

  it('shows the persisted MFT sequence for an NTFS candidate', () => {
    const recovery = candidate({
      filesystemType: 'ntfs',
      inode: '1024',
      mftSequence: 9,
      recoveryMethod: 'ntfs_mft_metadata',
    });
    render(<DeletedRecoveryPanel model={model({
      state: 'ready',
      page: page([recovery]),
      recoveries: [recovery],
      total: 1,
      selectedRecovery: recovery,
      selectedRecoveryId: recovery.id,
    })} />);

    expect(screen.getByText('MFT sequence')).toBeDefined();
    expect(screen.getByText('9')).toBeDefined();
  });

  it('shows common content hashes and lowercases hash search input', () => {
    const recovery = candidate({
      contentMd5: 'a'.repeat(32),
      contentSha1: 'b'.repeat(40),
      contentSha256: 'c'.repeat(64),
    });
    const setHashQuery = vi.fn();
    const runHashSearch = vi.fn();
    render(<DeletedRecoveryPanel model={model({
      state: 'ready',
      page: page([recovery]),
      recoveries: [recovery],
      total: 1,
      selectedRecovery: recovery,
      selectedRecoveryId: recovery.id,
      hashQuery: 'a'.repeat(32),
      hashQueryValid: true,
      setHashQuery,
      runHashSearch,
    })} />);

    expect(screen.getByText('MD5')).toBeDefined();
    expect(screen.getByText('SHA-1')).toBeDefined();
    expect(screen.getByText('SHA-256')).toBeDefined();
    fireEvent.change(screen.getByRole('textbox', { name: '恢复文件哈希' }), {
      target: { value: 'ABCDEF' },
    });
    expect(setHashQuery).toHaveBeenCalledWith('abcdef');
    fireEvent.click(screen.getByRole('button', { name: '按哈希查找' }));
    expect(runHashSearch).toHaveBeenCalledTimes(1);
  });

  it('reads only the selected verified provenance range', () => {
    const range: RecoveryProvenanceRange = {
      ordinal: 3,
      rangeRole: 'content',
      sourceKind: 'filesystem',
      logicalOffset: 4096,
      sourceOffset: 8192,
      length: 512,
      allocationState: 'free',
      sha256: 'c'.repeat(64),
    };
    const recovery = candidate({
      completeness: 'partial',
      allocationState: 'free',
      recoverableBytes: 512,
      provenanceRanges: [range],
    });
    const readSelectedRange = vi.fn();
    render(<DeletedRecoveryPanel model={model({
      state: 'ready',
      page: page([recovery]),
      recoveries: [recovery],
      total: 1,
      selectedRecovery: recovery,
      selectedRecoveryId: recovery.id,
      contentRanges: [range],
      selectedRangeOrdinal: 3,
      readSelectedRange,
    })} />);

    fireEvent.click(screen.getByRole('button', { name: '读取' }));

    expect(readSelectedRange).toHaveBeenCalledTimes(1);
  });
});
