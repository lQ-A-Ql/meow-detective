import { describe, expect, it } from 'vitest';
import { rebaseTreeNodeDepths } from './file-tree-utils';
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
});
