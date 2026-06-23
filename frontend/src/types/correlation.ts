import type { VerificationGuaranteeLevel } from './governance';
import type { CorrelationFamilyCoverage } from './governance';

export type CorrelationConfidence = 'direct' | 'strong' | 'weak' | 'heuristic';

export type CorrelationNodeKind = 'file' | 'artifact' | 'timelineEvent';

export type CorrelationEdgeKind =
  | 'sourceReference'
  | 'sharedSourceObject'
  | 'temporalContext'
  | 'pathMatch'
  | 'nameMatch'
  | 'recoveredOriginalPath';

export interface CorrelationJumpTarget {
  route: string;
  targetId: string;
  label: string;
}

export interface CorrelationProvenance {
  sourceKind: string;
  sourceRecordId: string;
  sourceLabel: string;
  producer?: string;
  producerVersion?: string;
  guaranteeLevel: VerificationGuaranteeLevel;
  warningSummary: string[];
}

export interface CorrelationNode {
  id: string;
  kind: CorrelationNodeKind;
  title: string;
  subtitle?: string;
  sourceObjectId?: string;
  relatedCount: number;
  badges: string[];
  jumps: CorrelationJumpTarget[];
}

export interface CorrelationEdge {
  id: string;
  kind: CorrelationEdgeKind;
  fromNodeId: string;
  toNodeId: string;
  summary: string;
  confidence: CorrelationConfidence;
}

export interface CorrelationCluster {
  id: string;
  title: string;
  summary: string;
  confidence: CorrelationConfidence;
  families: string[];
  primaryFileId: string;
  artifactCount: number;
  timelineCount: number;
  nodeIds: string[];
  edgeIds: string[];
  provenance: CorrelationProvenance[];
}

export interface CorrelationLead {
  id: string;
  title: string;
  summary: string;
  confidence: CorrelationConfidence;
  families: string[];
  primaryFileId: string;
  supportingNodeIds: string[];
  matchSignals: string[];
  jumps: CorrelationJumpTarget[];
  provenance: CorrelationProvenance[];
  caveats: string[];
}

export interface CorrelationSnapshot {
  generatedAt: string;
  nodeCount: number;
  edgeCount: number;
  clusterCount: number;
  leadCount: number;
  familyCoverage: CorrelationFamilyCoverage[];
  nodes: CorrelationNode[];
  edges: CorrelationEdge[];
  clusters: CorrelationCluster[];
  leads: CorrelationLead[];
}
