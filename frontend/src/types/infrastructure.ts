export interface InfrastructureGraphNode {
  id: string;
  domain: 'environment' | 'storage' | 'analysis';
  kind: string;
  name: string;
  status: string;
  confidence: string;
  provenanceJson: string;
  version?: string;
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
  networkFacts?: InfrastructureNetworkFact[];
  networkDiagnostics?: string[];
}

export interface InfrastructureNetworkFact {
  id: string;
  dataSourceId: string;
  environmentObjectId: string;
  fileId: string;
  sourcePath: string;
  lineNumber: number;
  factKind: string;
  subject: string;
  value: string;
  assertionKind: string;
  confidence: string;
  parser: string;
}
