import { createElement } from 'react';
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { FileTreeDataSourceNode } from './FileTreeDataSourceNode';
import type { DataSourceSummary, FileTreeNode } from '@/types/models';

const baseNode: FileTreeNode = {
  id: 'data-source:ds-1',
  name: 'Win10-C盘',
  depth: 0,
  hasChildren: true,
  deleted: false,
  hidden: false,
  system: false,
};

const dsLogical: DataSourceSummary = {
  id: 'ds-1',
  name: 'Win10-C盘',
  kind: 'logical_directory',
  sourcePath: 'C:\\Cases\\win10',
  importedAt: '2026-06-01T10:00:00Z',
  platform: 'windows',
};

describe('FileTreeDataSourceNode', () => {
  it('renders the data source name', () => {
    render(
      createElement(FileTreeDataSourceNode, {
        node: baseNode,
        expanded: false,
        dataSource: dsLogical,
        onClick: vi.fn(),
      }),
    );
    expect(screen.getByText('Win10-C盘')).toBeDefined();
  });

  it('shows kind badge when data source is provided', () => {
    render(
      createElement(FileTreeDataSourceNode, {
        node: baseNode,
        expanded: false,
        dataSource: dsLogical,
        onClick: vi.fn(),
      }),
    );
    expect(screen.getByText('目录')).toBeDefined();
  });

  it('does not show kind badge when data source is undefined', () => {
    render(
      createElement(FileTreeDataSourceNode, {
        node: baseNode,
        expanded: false,
        dataSource: undefined,
        onClick: vi.fn(),
      }),
    );
    expect(screen.queryByText('目录')).toBeNull();
    expect(screen.queryByText('E01')).toBeNull();
  });

  it('shows expanded chevron when node is expanded', () => {
    render(
      createElement(FileTreeDataSourceNode, {
        node: baseNode,
        expanded: true,
        dataSource: dsLogical,
        onClick: vi.fn(),
      }),
    );
    // ChevronDown should be present, ChevronRight should not
    const container = document.querySelector('[role="button"]')!;
    expect(container.innerHTML).toContain('lucide-chevron-down');
  });

  it('shows collapsed chevron when node is not expanded', () => {
    render(
      createElement(FileTreeDataSourceNode, {
        node: baseNode,
        expanded: false,
        dataSource: dsLogical,
        onClick: vi.fn(),
      }),
    );
    const container = document.querySelector('[role="button"]')!;
    expect(container.innerHTML).toContain('lucide-chevron-right');
  });

  it('calls onClick when clicked', () => {
    const onClick = vi.fn();
    render(
      createElement(FileTreeDataSourceNode, {
        node: baseNode,
        expanded: false,
        dataSource: dsLogical,
        onClick,
      }),
    );
    fireEvent.click(screen.getByText('Win10-C盘'));
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it('calls onClick on Enter key', () => {
    const onClick = vi.fn();
    render(
      createElement(FileTreeDataSourceNode, {
        node: baseNode,
        expanded: false,
        dataSource: dsLogical,
        onClick,
      }),
    );
    const button = document.querySelector('[role="button"]')!;
    fireEvent.keyDown(button, { key: 'Enter' });
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it('calls onClick on Space key', () => {
    const onClick = vi.fn();
    render(
      createElement(FileTreeDataSourceNode, {
        node: baseNode,
        expanded: false,
        dataSource: dsLogical,
        onClick,
      }),
    );
    const button = document.querySelector('[role="button"]')!;
    fireEvent.keyDown(button, { key: ' ' });
    expect(onClick).toHaveBeenCalledTimes(1);
  });
});
