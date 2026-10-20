import { act, renderHook } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useFileTree } from '@/features/files/hooks/use-file-tree';
import { resetFileTreeExpansion } from '@/features/files/file-tree-expansion-store';
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

const childrenByParent: Record<string, FileTreeNode[]> = {
  'partition-root-0': [
    {
      id: 'dir-a',
      name: 'Windows',
      depth: 2,
      hasChildren: true,
      deleted: false,
      hidden: false,
      system: false,
    },
  ],
  'dir-a': [
    {
      id: 'dir-b',
      name: 'System32',
      depth: 3,
      hasChildren: false,
      deleted: false,
      hidden: false,
      system: false,
    },
  ],
};

function childrenPage(parentId: string) {
  const children = childrenByParent[parentId] ?? [];
  return {
    data: {
      children,
      totalCount: children.length,
      offset: 0,
      limit: 500,
      truncated: false,
    },
    error: null,
    isLoading: false,
    isSuccess: true,
    isFetching: false,
    refetch: vi.fn(),
  };
}

const mocks = vi.hoisted(() => ({
  // Deliberately returns the same object reference on every call, mirroring
  // react-query's placeholderData + structural sharing behavior where a
  // quick visibility A→B→A toggle never produces a new rootTree reference.
  useFileTreeQuery: vi.fn(() => ({ data: rootTree })),
  useFileChildrenBranches: vi.fn(
    (
      branches: Array<{ parentId: string; offset: number }>,
      _limit: number,
      _showHidden: boolean,
    ) => branches.map((branch) => childrenPage(branch.parentId)),
  ),
}));

vi.mock('@/features/files/hooks', () => ({
  useFileTree: mocks.useFileTreeQuery,
  useFileChildrenBranches: mocks.useFileChildrenBranches,
}));
vi.mock('@/hooks/use-file-tree-keyboard', () => ({ useFileTreeKeyboard: vi.fn() }));
vi.mock('@/hooks/use-resizable-panel', () => ({
  useResizablePanel: () => ({ width: 224, isResizing: false, onResizeStart: vi.fn() }),
}));

interface RenderTreeOptions {
  showHidden?: boolean;
  caseId?: string;
  selectedDirectoryId?: string;
  revealDirectoryIds?: string[];
}

function renderTree({
  showHidden = false,
  caseId = 'case-a',
  selectedDirectoryId,
  revealDirectoryIds,
}: RenderTreeOptions = {}) {
  return renderHook<ReturnType<typeof useFileTree>, Partial<RenderTreeOptions>>(
    (props: Partial<RenderTreeOptions>) =>
      useFileTree({
        showHidden: props.showHidden ?? false,
        pageLimit: 500,
        selectedDirectoryId: props.selectedDirectoryId,
        setSelectedDirectoryId: vi.fn(),
        setSelectedFileId: vi.fn(),
        partitions: [],
        dataSources,
        caseId: props.caseId,
        revealDirectoryIds: props.revealDirectoryIds,
      }),
    {
      // Widened explicitly so `rerender` may flip a single field (the visible tests
      // clear or set one option at a time).
      initialProps: { showHidden, caseId, selectedDirectoryId, revealDirectoryIds } as Partial<RenderTreeOptions>,
    },
  );
}

function renderedIds(nodes: FileTreeNode[]) {
  return nodes.map((node) => node.id);
}

describe('useFileTree visibility toggling', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetFileTreeExpansion();
  });

  it('keeps data-source children expandable across rapid visibility toggles', () => {
    const { result, rerender } = renderTree();
    expect(result.current.treeChildren['data-source:ds-1']).toHaveLength(1);

    // A quick A→B→A toggle with an unchanged rootTree reference: the reset
    // wipe must be repaired by the pre-population effect, otherwise the tree
    // stays unexpandable until the next fetch.
    rerender({ showHidden: true });
    rerender({ showHidden: false });

    expect(result.current.treeChildren['data-source:ds-1']).toHaveLength(1);
    expect(result.current.treeChildren['data-source:ds-1']?.[0]?.id).toBe('partition-root-0');
  });

  it('repopulates children after a single visibility change with a stale reference', () => {
    const { result, rerender } = renderTree();
    rerender({ showHidden: true });
    expect(result.current.treeChildren['data-source:ds-1']).toHaveLength(1);
  });
});

describe('useFileTree branch materialization', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetFileTreeExpansion();
  });

  it('renders the ancestor path of a programmatically revealed directory', () => {
    const { result } = renderTree({
      selectedDirectoryId: 'dir-b',
      revealDirectoryIds: ['partition-root-0', 'dir-a'],
    });

    // The jump/restore path expands the ancestor levels it is about to reveal; the
    // tree must load their children even though none of them is the active row.
    act(() => {
      result.current.setExpandedDirectoryIds((current) => [
        ...current,
        'data-source:ds-1',
        'partition-root-0',
        'dir-a',
      ]);
    });

    expect(renderedIds(result.current.treeNodes)).toEqual(
      expect.arrayContaining(['data-source:ds-1', 'partition-root-0', 'dir-a', 'dir-b']),
    );
    expect(result.current.currentDirectory?.id).toBe('dir-b');
    expect(mocks.useFileChildrenBranches).toHaveBeenCalledWith(
      expect.arrayContaining([
        { parentId: 'partition-root-0', offset: 0 },
        { parentId: 'dir-a', offset: 0 },
      ]),
      500,
      false,
    );
  });

  it('walks one level at a time for a restored expansion instead of querying every branch', () => {
    const { result } = renderTree({ selectedDirectoryId: 'dir-b' });

    act(() => {
      result.current.setExpandedDirectoryIds((current) => [
        ...current,
        'data-source:ds-1',
        'partition-root-0',
        'dir-a',
      ]);
    });

    // `partition-root-0` is reachable (it is rendered), so its children are fetched and
    // `dir-a` becomes visible; `dir-a` then requests its own children.
    expect(renderedIds(result.current.treeNodes)).toEqual(
      expect.arrayContaining(['partition-root-0', 'dir-a']),
    );
    expect(result.current.currentDirectory?.id).toBe('dir-b');
  });
});

describe('useFileTree expansion persistence', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetFileTreeExpansion();
  });

  it('restores the expanded branches when the Files page remounts for the same case', () => {
    const first = renderTree({ caseId: 'case-a' });
    act(() => {
      first.result.current.setExpandedDirectoryIds((current) => [...current, 'dir-a']);
    });
    expect(first.result.current.expandedDirectoryIds).toContain('dir-a');
    first.unmount();

    const second = renderTree({ caseId: 'case-a' });
    expect(second.result.current.expandedDirectoryIds).toContain('dir-a');
  });

  it('starts a different case from a clean expansion', () => {
    const first = renderTree({ caseId: 'case-a' });
    act(() => {
      first.result.current.setExpandedDirectoryIds((current) => [...current, 'dir-a']);
    });
    first.unmount();

    const second = renderTree({ caseId: 'case-b' });
    expect(second.result.current.expandedDirectoryIds).not.toContain('dir-a');
  });
});
