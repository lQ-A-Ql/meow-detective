import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { FileTreePanel } from '@/features/files/components/FileTreePanel';
import type { FileTreePanelProps } from '@/features/files/components/FileTreePanel';

function createProps(overrides: Partial<FileTreePanelProps> = {}): FileTreePanelProps {
  return {
    filteredTreeNodes: [],
    treeLoading: false,
    activeDirectoryId: undefined,
    expandedIds: new Set(),
    activeChildrenPage: undefined,
    activeTreeChildrenLoaded: 0,
    canLoadMoreTreeChildren: false,
    loadMoreActiveTreeChildren: vi.fn(),
    toggleDirectory: vi.fn(),
    displayNodeName: (name) => name,
    filterQuery: '',
    setFilterQuery: vi.fn(),
    treeWidth: 224,
    isResizing: false,
    onResizeStart: vi.fn(),
    treeContainerRef: { current: null },
    dataSources: [],
    FILE_BROWSER_PAGE_LIMIT: 500,
    ...overrides,
  };
}

describe('FileTreePanel', () => {
  it('shows a loading hint while the root tree is being fetched', () => {
    render(<FileTreePanel {...createProps({ treeLoading: true })} />);
    expect(screen.getByText('正在加载目录树...')).toBeInTheDocument();
    expect(screen.queryByText('导入数据源后显示目录树。')).not.toBeInTheDocument();
  });

  it('renders the empty hint only when nothing is loading and no nodes exist', () => {
    render(<FileTreePanel {...createProps()} />);
    expect(screen.getByText('导入数据源后显示目录树。')).toBeInTheDocument();
    expect(screen.queryByText('正在加载目录树...')).not.toBeInTheDocument();
  });

  it('keeps data-source nodes visible while children stream in', () => {
    render(
      <FileTreePanel
        {...createProps({
          treeLoading: true,
          filteredTreeNodes: [{
            id: 'data-source:ds-1',
            name: '测试镜像',
            depth: 0,
            hasChildren: true,
            dataSourceId: 'ds-1',
            deleted: false,
            hidden: false,
            system: false,
          }],
          dataSources: [{
            id: 'ds-1',
            name: '测试镜像',
            kind: 'e01',
            sourcePath: 'E:/images/test.E01',
            importedAt: '2026-08-01T00:00:00Z',
            platform: 'windows',
          }],
        })}
      />,
    );
    expect(screen.getByText('正在加载目录树...')).toBeInTheDocument();
    expect(screen.getByText('测试镜像')).toBeInTheDocument();
  });
});
