import type { RegistryHiveDescriptor, RegistryHiveSession, RegistryKeyMetadata } from '@/types/registryBrowser';
import type { RegistryTreeNode, RegistryTreePage } from './types';

export function registryTreeNodes(hives: RegistryHiveDescriptor[], session: RegistryHiveSession | undefined,
  pages: Record<string, RegistryTreePage>, expanded: ReadonlySet<string>, loading: ReadonlySet<string>,
  errors: Record<string, string>, instanceId: string): RegistryTreeNode[] {
  const keyNode = (key: RegistryKeyMetadata, hive: RegistryHiveDescriptor, depth: number, position: number, setSize: number): RegistryTreeNode => {
    const page = pages[key.keyId];
    const isExpanded = expanded.has(key.keyId);
    return { id: key.keyId, domId: `${instanceId}-${key.keyId}`, label: key.name, depth,
      expanded: isExpanded, hasChildren: key.subkeyCount > 0, position, setSize, hive, key,
      children: isExpanded ? (page?.keys ?? []).map((child, index) => keyNode(child, hive, depth + 1, index + 1, key.subkeyCount)) : [],
      loading: loading.has(key.keyId), error: errors[key.keyId], nextCursor: isExpanded ? page?.page.nextCursor : undefined };
  };
  return hives.map((hive, index) => {
    const root = hive.fileId === session?.hive.fileId ? session.rootKey : undefined;
    const node = root ? keyNode(root, hive, 0, index + 1, hives.length) : undefined;
    return { ...node, id: hive.fileId, domId: `${instanceId}-${hive.fileId}`,
      label: hive.profileLabel ? `${hive.name} · ${hive.profileLabel}` : hive.name,
      hive, key: root, depth: 0, position: index + 1, setSize: hives.length,
      expanded: node?.expanded ?? false, hasChildren: root ? root.subkeyCount > 0 : hive.readState === 'readable' || hive.readState === 'candidate',
      children: node?.children ?? [], loading: node?.loading ?? false, error: node?.error,
      nextCursor: node?.nextCursor };
  });
}

export function flattenRegistryTree(nodes: RegistryTreeNode[]): RegistryTreeNode[] {
  return nodes.flatMap((node) => [node, ...flattenRegistryTree(node.children)]);
}

/** Keep ancestors of matching loaded keys; no unloaded branch is presented as searched. */
export function filterRegistryTree(nodes: RegistryTreeNode[], query: string): RegistryTreeNode[] {
  if (!query.trim()) return nodes;
  const search = query.toLowerCase();
  return nodes.flatMap((node) => {
    const children = filterRegistryTree(node.children, query);
    return node.label.toLowerCase().includes(search) || children.length ? [{ ...node, children }] : [];
  });
}
