export type EdgeType =
  | 'contains'
  | 'references'
  | 'correlatesWith'
  | 'derivesFrom'
  | 'precedes'
  | 'cites'
  | 'annotates';

export type NodeType = 'file' | 'artifact' | 'timelineEvent' | 'entity' | 'lead' | 'notebookEntry';

export interface GraphEdge {
  id: string;
  caseId: string;
  sourceId: string;
  targetId: string;
  edgeType: EdgeType;
  confidence?: number;
  provenance?: string;
  createdAt: string;
}

export interface GraphNode {
  id: string;
  caseId: string;
  nodeType: NodeType;
  label: string;
  summary: string;
  tags: string[];
  createdAt: string;
}

export interface GraphQuery {
  startIds: string[];
  edgeTypes: EdgeType[];
  maxDepth: number;
  confidenceFloor?: number;
  limit?: number;
  edgeLimit?: number;
}

export interface GraphQueryResult {
  nodes: GraphNode[];
  edges: GraphEdge[];
  nodeCount: number;
  edgeCount: number;
  truncated: boolean;
  maxDepthReached: number;
  dataSourceIds: string[];
}

export interface GraphSnapshot {
  nodeCountByType: Record<string, number>;
  edgeCountByType: Record<string, number>;
  totalNodes: number;
  totalEdges: number;
  density: number;
  largestComponentSize: number;
  dataSourceCount: number;
  crossSourceEntityCount: number;
  crossSourceEdgeCount: number;
  seedIds: string[];
  projectionBuiltAt?: string;
}

export interface GraphStats {
  nodeCountByType: Record<string, number>;
  edgeCountByType: Record<string, number>;
  totalNodes: number;
  totalEdges: number;
  density: number;
  largestComponentSize: number;
}

export interface GraphProvenanceEntry {
  edgeId: string;
  sourceRuleId?: string;
  sourceParser?: string;
  extractionTimestamp?: string;
  parserVersion?: string;
}
