import type { InfrastructureGraphEdge, InfrastructureGraphNode } from '@/types/models';

export function infrastructureInspectorModel(node: InfrastructureGraphNode, edges: InfrastructureGraphEdge[]) {
  let parser: string | undefined;
  try {
    const value: unknown = node.provenanceJson.length <= 64 * 1024 ? JSON.parse(node.provenanceJson) : undefined;
    if (value && typeof value === 'object' && !Array.isArray(value) && 'parser' in value && typeof value.parser === 'string') {
      parser = value.parser;
    }
  } catch { /* An absent or malformed provenance does not fabricate a parser name. */ }
  return {
    parser,
    linkedCount: edges.filter((edge) => edge.sourceId === node.id || edge.targetId === node.id).length,
    statusVariant: ['ready', 'complete', 'verified'].includes(node.status) ? 'default' as const : 'outline' as const,
  };
}
