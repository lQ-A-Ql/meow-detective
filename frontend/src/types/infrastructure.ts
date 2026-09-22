export interface InfrastructureGraphNode {
  id: string;
  domain: 'environment' | 'storage' | 'analysis';
  kind: string;
  name: string;
  status: string;
  confidence: string;
  provenanceJson: string;
}

export interface InfrastructureGraphEdge {
  sourceId: string;
  targetId: string;
  relationKind: string;
  confidence: string;
  provenanceJson: string;
}

export interface InfrastructureGraph {
  nodes: InfrastructureGraphNode[];
  edges: InfrastructureGraphEdge[];
}
