export type EdgeType = 'Contains' | 'References' | 'CorrelatesWith' | 'DerivesFrom' | 'Precedes' | 'Cites' | 'Annotates';

export type NodeType = 'File' | 'Artifact' | 'TimelineEvent' | 'Entity' | 'Lead' | 'NotebookEntry';

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
}

export interface GraphQueryResult {
  nodes: GraphNode[];
  edges: GraphEdge[];
  nodeCount: number;
  edgeCount: number;
}

export interface GraphSnapshot {
  nodeCountByType: Record<string, number>;
  edgeCountByType: Record<string, number>;
  totalNodes: number;
  totalEdges: number;
  density: number;
  largestComponentSize: number;
}

export interface GraphStats {
  nodeCountByType: Record<string, number>;
  edgeCountByType: Record<string, number>;
  totalNodes: number;
  totalEdges: number;
  density: number;
  largestComponentSize: number;
}
