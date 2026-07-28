import type { FileTreeNode } from '@/types/models';

type TreeChildrenById = Readonly<Record<string, readonly FileTreeNode[]>>;

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

export function rebaseTreeNodeDepths(
  nodes: FileTreeNode[],
  parentDepth: number,
) {
  if (nodes.length === 0) {
    return nodes;
  }

  const minDepth = Math.min(...nodes.map((node) => node.depth));
  const expectedDepth = parentDepth + 1;
  if (minDepth === expectedDepth) {
    return nodes;
  }

  const delta = expectedDepth - minDepth;
  return nodes.map((node) => ({
    ...node,
    depth: Math.max(0, node.depth + delta),
  }));
}

export function flattenExpandedTree(
  roots: readonly FileTreeNode[],
  childrenById: TreeChildrenById,
  expandedIds: ReadonlySet<string>,
) {
  const visible: FileTreeNode[] = [];
  const visited = new Set<string>();
  const stack = [...roots].reverse();

  while (stack.length > 0) {
    const node = stack.pop();
    if (!node || visited.has(node.id)) continue;
    visited.add(node.id);
    visible.push(node);

    if (!expandedIds.has(node.id)) continue;
    const children = childrenById[node.id] ?? [];
    for (let index = children.length - 1; index >= 0; index -= 1) {
      stack.push(children[index]);
    }
  }

  return visible;
}

/**
 * Returns a cache entry that is not needed to render the currently visible
 * expanded tree. A visible ancestor must never be evicted: doing so removes
 * the entire branch from the tree until the next backend fetch.
 */
export function findEvictableTreeCacheKey(
  roots: readonly FileTreeNode[],
  childrenById: TreeChildrenById,
  expandedIds: ReadonlySet<string>,
  pinnedIds: ReadonlySet<string>,
) {
  const visibleExpandedIds = new Set(
    flattenExpandedTree(roots, childrenById, expandedIds)
      .filter((node) => expandedIds.has(node.id))
      .map((node) => node.id),
  );

  return Object.keys(childrenById).find(
    (nodeId) => !pinnedIds.has(nodeId) && !visibleExpandedIds.has(nodeId),
  );
}

export function findTreeNode(
  nodeId: string | undefined,
  roots: readonly FileTreeNode[],
  childrenById: TreeChildrenById,
) {
  if (!nodeId) return undefined;
  const visited = new Set<string>();
  const stack = [...roots].reverse();

  while (stack.length > 0) {
    const node = stack.pop();
    if (!node || visited.has(node.id)) continue;
    if (node.id === nodeId) return node;
    visited.add(node.id);

    const children = childrenById[node.id] ?? [];
    for (let index = children.length - 1; index >= 0; index -= 1) {
      stack.push(children[index]);
    }
  }

  return undefined;
}

export function findTreeRoot(
  nodeId: string | undefined,
  roots: readonly FileTreeNode[],
  childrenById: TreeChildrenById,
) {
  if (!nodeId) return undefined;
  const visited = new Set<string>();
  const stack = roots
    .map((root) => ({ node: root, root }))
    .reverse();

  while (stack.length > 0) {
    const current = stack.pop();
    if (!current || visited.has(current.node.id)) continue;
    if (current.node.id === nodeId) return current.root;
    visited.add(current.node.id);

    const children = childrenById[current.node.id] ?? [];
    for (let index = children.length - 1; index >= 0; index -= 1) {
      stack.push({ node: children[index], root: current.root });
    }
  }

  return undefined;
}

export function collectTreeNodeIds(
  roots: readonly FileTreeNode[],
  childrenById: TreeChildrenById,
) {
  const ids = new Set<string>();
  const stack = [...roots].reverse();

  while (stack.length > 0) {
    const node = stack.pop();
    if (!node || ids.has(node.id)) continue;
    ids.add(node.id);

    const children = childrenById[node.id] ?? [];
    for (let index = children.length - 1; index >= 0; index -= 1) {
      stack.push(children[index]);
    }
  }

  return ids;
}
