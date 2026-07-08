import { createElement } from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { DataSourceSelector } from './DataSourceSelector';
import type { DataSourceSummary } from '@/types/models';

const dsWindows: DataSourceSummary = {
  id: 'ds-1',
  name: 'Win10-C盘',
  kind: 'logical_directory',
  sourcePath: 'C:\\Cases\\win10',
  importedAt: '2026-06-01T10:00:00Z',
  partitions: [
    {
      index: 0,
      name: 'C:',
      kindLabel: 'Basic data partition',
      status: 'supported',
      offset: 0,
      length: 1024,
      filesystem: 'NTFS',
    },
  ],
};

const dsE01: DataSourceSummary = {
  id: 'ds-2',
  name: 'Evidence-001',
  kind: 'e01',
  sourcePath: 'E:\\cases\\evidence.E01',
  importedAt: '2026-06-02T10:00:00Z',
  partitions: [
    {
      index: 1,
      name: 'root',
      kindLabel: 'Linux root',
      status: 'supported',
      offset: 0,
      length: 2048,
      filesystem: 'XFS',
    },
  ],
};

const dsRaw: DataSourceSummary = {
  id: 'ds-3',
  name: 'Ubuntu-Server',
  kind: 'raw',
  sourcePath: '/mnt/images/ubuntu.raw',
  importedAt: '2026-06-03T10:00:00Z',
  partitions: [
    {
      index: 0,
      name: 'rootfs',
      kindLabel: 'Linux filesystem',
      status: 'supported',
      offset: 0,
      length: 4096,
      filesystem: 'EXT4',
    },
  ],
};

describe('DataSourceSelector', () => {
  it('renders a button for each data source plus the "all" toggle', () => {
    const onSelect = vi.fn();
    render(
      createElement(DataSourceSelector, {
        dataSources: [dsWindows, dsE01],
        onSelect,
      }),
    );
    expect(screen.getByText('全部数据源')).toBeDefined();
    expect(screen.getByText('Win10-C盘')).toBeDefined();
    expect(screen.getByText('Evidence-001')).toBeDefined();
  });

  it('shows platform badge next to each data source name', () => {
    render(
      createElement(DataSourceSelector, {
        dataSources: [dsWindows, dsE01, dsRaw],
        onSelect: vi.fn(),
      }),
    );
    expect(screen.getByText('(Windows)')).toBeDefined();
    expect(screen.getAllByText('(Linux)')).toHaveLength(2);
    expect(screen.queryByText('(E01)')).toBeNull();
    expect(screen.queryByText('(RAW)')).toBeNull();
  });

  it('calls onSelect with the data source id when a button is clicked', () => {
    const onSelect = vi.fn();
    render(
      createElement(DataSourceSelector, {
        dataSources: [dsWindows, dsE01],
        onSelect,
      }),
    );
    fireEvent.click(screen.getByLabelText('Win10-C盘'));
    expect(onSelect).toHaveBeenCalledWith('ds-1');
  });

  it('calls onSelect with undefined when "全部数据源" is selected', () => {
    const onSelect = vi.fn();
    render(
      createElement(DataSourceSelector, {
        dataSources: [dsWindows],
        selectedId: 'ds-1',
        onSelect,
      }),
    );
    fireEvent.click(screen.getByLabelText('全部数据源'));
    expect(onSelect).toHaveBeenCalledWith(undefined);
  });

  it('renders null when dataSources is empty', () => {
    const { container } = render(
      createElement(DataSourceSelector, {
        dataSources: [],
        onSelect: vi.fn(),
      }),
    );
    expect(container.innerHTML).toBe('');
  });

  it('highlights the currently selected data source', () => {
    render(
      createElement(DataSourceSelector, {
        dataSources: [dsWindows, dsE01],
        selectedId: 'ds-1',
        onSelect: vi.fn(),
      }),
    );
    const selected = screen.getByLabelText('Win10-C盘');
    expect(selected.getAttribute('data-state')).toBe('on');
  });
});
