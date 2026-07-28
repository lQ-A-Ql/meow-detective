import { describe, expect, it } from 'vitest';
import {
  collectTreeNodeIds,
  findEvictableTreeCacheKey,
  findTreeNode,
  findTreeRoot,
  flattenExpandedTree,
  rebaseTreeNodeDepths,
} from './file-tree-utils';
import type { FileTreeNode } from '@/types/models';

function node(id: string, depth: number): FileTreeNode {
  return {
    id,
    name: id,
    depth,
    hasChildren: false,
    deleted: false,
    hidden: false,
    system: false,
  };
}

describe('file-tree-utils', () => {
  it('rebases backend child depths under the active parent depth', () => {
    const result = rebaseTreeNodeDepths(
      [node('efi', 1), node('EFI', 2)],
      1,
    );

    expect(result.map((item) => item.depth)).toEqual([2, 3]);
  });

  it('preserves already aligned depths without cloning', () => {
    const children = [node('efi', 2), node('EFI', 3)];

    expect(rebaseTreeNodeDepths(children, 1)).toBe(children);
  });

  it('stops at self and ancestor cycles while preserving tree order', () => {
    const root = node('root', 0);
    const child = node('child', 1);
    const sibling = node('sibling', 1);
    const children = {
      root: [root, child, sibling],
      child: [root],
    };
    const expanded = new Set(['root', 'child']);

    expect(flattenExpandedTree([root], children, expanded).map((entry) => entry.id)).toEqual([
      'root',
      'child',
      'sibling',
    ]);
    expect(findTreeNode('child', [root], children)).toBe(child);
    expect(findTreeRoot('child', [root], children)).toBe(root);
    expect(Array.from(collectTreeNodeIds([root], children))).toEqual([
      'root',
      'child',
      'sibling',
    ]);
  });

  it('flattens a deeply nested tree without consuming the JavaScript call stack', () => {
    const depth = 20_000;
    const nodes = Array.from({ length: depth }, (_, index) => node(`node-${index}`, index));
    const children: Record<string, FileTreeNode[]> = {};
    for (let index = 0; index < nodes.length - 1; index += 1) {
      children[nodes[index].id] = [nodes[index + 1]];
    }

    const flattened = flattenExpandedTree(
      [nodes[0]],
      children,
      new Set(nodes.map((entry) => entry.id)),
    );

    expect(flattened).toHaveLength(depth);
    expect(flattened.at(-1)?.id).toBe(`node-${depth - 1}`);
  });

  it('evicts a collapsed cache entry before a visible expanded ancestor', () => {
    const root = node('root', 0);
    const expandedBranch = node('expanded', 1);
    const collapsedBranch = node('collapsed', 1);
    const nested = node('nested', 2);
    const children = {
      root: [expandedBranch, collapsedBranch],
      expanded: [nested],
      collapsed: [],
    };

    expect(
      findEvictableTreeCacheKey(
        [root],
        children,
        new Set(['root', 'expanded']),
        new Set(),
      ),
    ).toBe('collapsed');
  });

  it('keeps the cache intact when every entry renders an expanded branch', () => {
    const root = node('root', 0);
    const branch = node('branch', 1);
    const children = {
      root: [branch],
      branch: [],
    };

    expect(
      findEvictableTreeCacheKey(
        [root],
        children,
        new Set(['root', 'branch']),
        new Set(),
      ),
    ).toBeUndefined();
  });
});
