import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { ImageMountDialog } from './ImageMountDialog';
import type { ImageMountModel } from '@/features/files/hooks/use-image-mount-model';
import type { DataSourceSummary, MountStatus } from '@/types/models';

const source: DataSourceSummary = {
  id: 'ds-1',
  name: '检材镜像',
  kind: 'e01',
  sourcePath: 'E:\\evidence.E01',
  importedAt: '2026-08-04T00:00:00Z',
  platform: 'windows',
  importState: 'ready',
  partitions: [{
    index: 3,
    name: 'Windows',
    kindLabel: 'Basic data partition',
    status: 'ready',
    offset: 0,
    length: 1024 * 1024,
    filesystem: 'NTFS',
  }],
};

function createModel(overrides: Partial<ImageMountModel> = {}): ImageMountModel {
  return {
    dialogOpen: true,
    openDialog: vi.fn(),
    setDialogOpen: vi.fn(),
    dataSources: [source],
    selectedSourceId: source.id,
    setSelectedSourceId: vi.fn(),
    selectedSource: source,
    mountMode: 'logicalPartition',
    setMountMode: vi.fn(),
    partitions: source.partitions ?? [],
    selectedPartitionIndex: '3',
    setSelectedPartitionIndex: vi.fn(),
    selectedPartition: source.partitions?.[0],
    mountPoint: 'auto',
    setMountPoint: vi.fn(),
    mounts: [],
    selectedMount: undefined,
    isLoadingMounts: false,
    isSubmitting: false,
    isMounting: false,
    isUnmounting: false,
    error: undefined,
    submit: vi.fn().mockResolvedValue(undefined),
    unmount: vi.fn().mockResolvedValue(undefined),
    refresh: vi.fn(),
    mountPointOptions: ['M:', 'N:'],
    ...overrides,
  };
}

describe('ImageMountDialog', () => {
  it('shows the selected source, partition and immutable read-only contract', () => {
    render(<ImageMountDialog model={createModel()} />);

    expect(screen.getByText('挂载只读镜像')).toBeInTheDocument();
    expect(screen.getAllByText('检材镜像 (WINDOWS / E01)')).toHaveLength(2);
    expect(screen.getByText('证据保护已启用')).toBeInTheDocument();
    expect(screen.getByText(/仅允许读取、目录列举和属性查询/)).toBeInTheDocument();
  });

  it('submits the real feature action from the mount button', () => {
    const submit = vi.fn().mockResolvedValue(undefined);
    render(<ImageMountDialog model={createModel({ submit })} />);

    fireEvent.click(screen.getByRole('button', { name: '挂载' }));

    expect(submit).toHaveBeenCalledTimes(1);
  });

  it('shows physical-disk semantics without partition or drive-letter controls', () => {
    render(<ImageMountDialog model={createModel({ mountMode: 'physicalDisk' })} />);

    expect(screen.getByText(/整份 E01\/raw 镜像/)).toBeInTheDocument();
    expect(screen.queryByText('文件系统分区')).not.toBeInTheDocument();
    expect(screen.queryByText('盘符')).not.toBeInTheDocument();
    expect(screen.getByText('Windows 物理磁盘')).toBeInTheDocument();
  });

  it('exposes an active mount and delegates unmount to the feature model', () => {
    const unmount = vi.fn().mockResolvedValue(undefined);
    const mount: MountStatus = {
      target: {
        mountId: 'mount-1',
        dataSourceId: source.id,
        partitionIndex: 3,
        filesystem: 'NTFS',
        mountPoint: 'M:',
        readOnly: true,
        mode: 'logicalPartition',
      },
      state: 'mounted',
      activeHandleCount: 0,
    };
    render(<ImageMountDialog model={createModel({ selectedMount: mount, unmount })} />);

    fireEvent.click(screen.getByRole('button', { name: '卸载' }));

    expect(unmount).toHaveBeenCalledWith('mount-1');
    expect(screen.getByText('M: · 已挂载')).toBeInTheDocument();
  });
});
