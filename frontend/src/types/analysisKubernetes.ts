import type { AnalysisParseStatus } from './analysis';

export interface KubernetesClusterArtifact {
  dataSourceId: string;
  fileId: string;
  path: string;
  kind: string;
  size: number;
  deleted: boolean;
  encrypted: boolean;
  status: AnalysisParseStatus;
  detail?: string;
  diagnostics: string[];
}

export interface KubernetesClusterNode {
  dataSourceId: string;
  sourceName: string;
  nodeName?: string;
  controlPlane: boolean;
  artifactCount: number;
  parsedArtifactCount: number;
  failedArtifactCount: number;
  status: AnalysisParseStatus;
  diagnostics: string[];
}

export interface KubernetesClusterSummary {
  status: AnalysisParseStatus;
  scopeId?: string;
  scopeName?: string;
  selectedDataSourceId: string;
  expectedMemberCount: number;
  readyMemberCount: number;
  controlPlaneMemberCount: number;
  artifactCount: number;
  nodes: KubernetesClusterNode[];
  artifacts: KubernetesClusterArtifact[];
  diagnostics: string[];
}
