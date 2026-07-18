import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { DataSourcesPanel } from './CaseOverview';
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
        onDelete={vi.fn()}
      />,
    );

    expect(screen.getByText('处理失败').getAttribute('title')).toBe('Search phase failed');
    expect(screen.getByText('phase 4/6')).toBeTruthy();
    expect(screen.getByText('failed 1')).toBeTruthy();
    expect(screen.getByText('deferred 1')).toBeTruthy();
  });
});
