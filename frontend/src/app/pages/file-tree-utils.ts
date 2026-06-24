import type { FileTreeNode } from '@/types/models';

export function sameTreeNode(left: FileTreeNode, right: FileTreeNode) {
  return (
    left.id === right.id &&
    left.name === right.name &&
    left.depth === right.depth &&
    left.hasChildren === right.hasChildren &&
    left.entryType === right.entryType &&
    left.status === right.status &&
    left.expanded === right.expanded &&
    left.deleted === right.deleted &&
    left.hidden === right.hidden &&
    left.system === right.system
  );
}

export function sameTreeNodeList(left: FileTreeNode[], right: FileTreeNode[]) {
  if (left.length !== right.length) {
    return false;
  }

  for (let index = 0; index < left.length; index += 1) {
    if (!sameTreeNode(left[index], right[index])) {
      return false;
    }
  }

  return true;
}

export function mergeTreeNodePages(
  existing: FileTreeNode[],
  incoming: FileTreeNode[],
) {
  const merged = [...existing];
  const indexById = new Map(existing.map((node, index) => [node.id, index]));
  let changed = false;

  for (const node of incoming) {
    const existingIndex = indexById.get(node.id);
    if (existingIndex === undefined) {
      merged.push(node);
      indexById.set(node.id, merged.length - 1);
      changed = true;
      continue;
    }

    if (!sameTreeNode(merged[existingIndex], node)) {
      merged[existingIndex] = node;
      changed = true;
    }
  }

  return changed ? merged : existing;
}
