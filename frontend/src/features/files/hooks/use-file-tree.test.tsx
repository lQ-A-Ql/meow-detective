import { renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useFileTree } from '@/features/files/hooks/use-file-tree';
import type { DataSourceSummary, FileTreeNode } from '@/types/models';

const rootTree: FileTreeNode[] = [
  {
    id: 'partition-root-0',
    name: '[P0]',
    depth: 0,
    hasChildren: true,
    dataSourceId: 'ds-1',
    deleted: false,
    hidden: false,
    system: false,
  },
];

const dataSources: DataSourceSummary[] = [
  {
    id: 'ds-1',
    name: '测试镜像',
    kind: 'e01',
    sourcePath: 'E:/images/test.E01',
    importedAt: '2026-08-01T00:00:00Z',
    platform: 'windows',
  },
];

const mocks = vi.hoisted(() => ({
  // Deliberately returns the same object reference on every call, mirroring
  // react-query's placeholderData + structural sharing behavior where a
  // quick visibility A→B→A toggle never produces a new rootTree reference.
  useFileTreeQuery: vi.fn(() => ({ data: rootTree })),
  useFileChildrenPage: vi.fn(() => ({ data: undefined })),
}));

vi.mock('@/features/files/hooks', () => ({
  useFileTree: mocks.useFileTreeQuery,
  useFileChildrenPage: mocks.useFileChildrenPage,
}));
vi.mock('@/hooks/use-file-tree-keyboard', () => ({ useFileTreeKeyboard: vi.fn() }));
vi.mock('@/hooks/use-resizable-panel', () => ({
  useResizablePanel: () => ({ width: 224, isResizing: false, onResizeStart: vi.fn() }),
}));

function renderTree(showHidden: boolean) {
  return renderHook(
    ({ hidden }) =>
      useFileTree({
        showHidden: hidden,
        pageLimit: 500,
        selectedDirectoryId: undefined,
        setSelectedDirectoryId: vi.fn(),
        setSelectedFileId: vi.fn(),
        partitions: [],
        dataSources,
      }),
    { initialProps: { hidden: showHidden } },
  );
}

describe('useFileTree visibility toggling', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('keeps data-source children expandable across rapid visibility toggles', () => {
    const { result, rerender } = renderTree(false);
    expect(result.current.treeChildren['data-source:ds-1']).toHaveLength(1);

    // A quick A→B→A toggle with an unchanged rootTree reference: the reset
    // wipe must be repaired by the pre-population effect, otherwise the tree
    // stays unexpandable until the next fetch.
    rerender({ hidden: true });
    rerender({ hidden: false });

    expect(result.current.treeChildren['data-source:ds-1']).toHaveLength(1);
    expect(result.current.treeChildren['data-source:ds-1']?.[0]?.id).toBe('partition-root-0');
  });

  it('repopulates children after a single visibility change with a stale reference', () => {
    const { result, rerender } = renderTree(false);
    rerender({ hidden: true });
    expect(result.current.treeChildren['data-source:ds-1']).toHaveLength(1);
  });
});
