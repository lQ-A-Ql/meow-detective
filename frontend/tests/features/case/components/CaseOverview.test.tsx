import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { DataSourcesPanel } from '@/features/case/components/CaseOverview';
import type { DataSourceSummary } from '@/types/models';

const dataSource: DataSourceSummary = {
  id: 'derived-rbd',
  name: 'vm-100-disk-0',
  kind: 'ceph_rbd',
  sourcePath: 'ceph-rbd://cluster/image',
  importedAt: '2026-07-17T00:00:00Z',
  platform: 'linux',
  importState: 'ready',
  processing: {
    state: 'failed',
    totalCount: 6,
    readyCount: 4,
    pendingCount: 0,
    runningCount: 0,
    failedCount: 1,
    deferredCount: 1,
    lastError: 'Search phase failed',
    phases: [],
  },
};

describe('DataSourcesPanel', () => {
  it('renders the backend processing aggregate without deriving phase semantics', () => {
    render(
      <DataSourcesPanel
        dataSources={[dataSource]}
        editingDataSourceId={undefined}
        editingDataSourceName=""
        setEditingDataSourceId={vi.fn()}
        setEditingDataSourceName={vi.fn()}
        onRename={vi.fn()}
        onRequestDelete={vi.fn()}
      />,
    );

    expect(screen.getByText('处理失败').getAttribute('title')).toBe('Search phase failed');
    expect(screen.getByText('阶段 4/6')).toBeTruthy();
    expect(screen.getByText('失败 1')).toBeTruthy();
    expect(screen.getByText('延后 1')).toBeTruthy();
  });

  it('does not render an automatic evidence hash progress row', () => {
    render(
      <DataSourcesPanel
        dataSources={[{ ...dataSource, hashStatus: 'pending' }]}
        editingDataSourceId={undefined}
        editingDataSourceName=""
        setEditingDataSourceId={vi.fn()}
        setEditingDataSourceName={vi.fn()}
        onRename={vi.fn()}
        onRequestDelete={vi.fn()}
      />,
    );

    expect(screen.queryByText('后台计算中')).toBeNull();
    expect(screen.queryByText('42%')).toBeNull();
  });

  it('offers original disk hashing for image and physical disk sources', async () => {
    const onCalculate = vi.fn();
    render(
      <DataSourcesPanel
        dataSources={[{ ...dataSource, id: 'physical-0', kind: 'raw', sourcePath: 'disk.raw' }]}
        editingDataSourceId={undefined}
        editingDataSourceName=""
        setEditingDataSourceId={vi.fn()}
        setEditingDataSourceName={vi.fn()}
        onRename={vi.fn()}
        onRequestDelete={vi.fn()}
        onCalculateLogicalDiskDigest={onCalculate}
      />,
    );
    expect(screen.getByText('原始磁盘哈希')).toBeTruthy();
    screen.getByRole('button', { name: 'SHA256' }).click();
    expect(onCalculate).toHaveBeenCalledWith('physical-0', 'sha256');
  });
});
