import type { FileJumpContext } from '@/types/models';
import { dataSourceNodeId } from './file-tree-utils';

export function directoryExpansionIds(context: FileJumpContext, includeDirectory = false) {
  const ids = context.ancestorDirectoryIds.filter((id) => includeDirectory || id !== context.directory.id);
  const sourceId = context.ancestorNodes?.[0]?.dataSourceId;
  return sourceId ? [dataSourceNodeId(sourceId), ...ids] : ids;
}

export function hiddenDirectoryDestination(context: FileJumpContext) {
  const nodes = context.ancestorNodes ?? [];
  const hiddenIndex = nodes.findIndex((node) => node.hidden || node.system);
  if (hiddenIndex > 0) return { hidden: true, directoryId: nodes[hiddenIndex - 1].id };
  if (hiddenIndex === 0) {
    return { hidden: true, directoryId: nodes[0].dataSourceId ? dataSourceNodeId(nodes[0].dataSourceId) : undefined };
  }
  return { hidden: context.requiresShowHidden, directoryId: context.directory.parentId };
}
