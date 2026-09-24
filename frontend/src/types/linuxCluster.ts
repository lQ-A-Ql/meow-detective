export interface LinuxEvidenceSetSummary {
  importSetId: string;
  name: string;
  state: string;
  memberCount: number;
  readyCount: number;
  failedCount: number;
  capabilityLevel: string;
  manifestDigest?: string;
  manifestSchemaVersion?: number;
  collectedAt?: string;
  members: LinuxEvidenceSetMemberSummary[];
  scopes: LinuxTopologyScopeSummary[];
  edges: LinuxTopologyEdgeSummary[];
  derivedSources: LinuxDerivedSourceSummary[];
  diagnostics: string[];
}

export interface LinuxEvidenceSetListItem {
  importSetId: string;
  name: string;
  state: string;
  memberCount: number;
  readyCount: number;
  failedCount: number;
}

export interface LinuxEvidenceSetMemberSummary {
  memberIndex: number;
  dataSourceId?: string;
  sourceName: string;
  sourcePath: string;
  sourceKind: string;
  importState: string;
  hashStatus: string;
  provenanceStatus: string;
}

export interface LinuxTopologyScopeSummary {
  id: string;
  kind: string;
  name: string;
  identityState: string;
  status: string;
  evidenceCompleteness: string;
  memberCount: number;
  memberSourceIds: string[];
  memberRoles: LinuxTopologyMemberSummary[];
  diagnostics: string[];
}

export interface LinuxTopologyMemberSummary {
  dataSourceId: string;
  role: string;
  memberIndex?: number;
  confidence: string;
}

export interface LinuxTopologyEdgeSummary {
  sourceScopeId: string;
  targetScopeId: string;
  kind: string;
  confidence: string;
  provenance: string;
}

export interface LinuxDerivedSourceSummary {
  dataSourceId: string;
  kind: string;
  importState: string;
  provenanceStatus: string;
  sourcePath: string;
}

export interface LinuxEvidenceEvent {
  eventId: string;
  dataSourceId: string;
  sourceObjectId: string;
  eventType: string;
  eventTime: string;
  observedTime?: string;
  ingestTime?: string;
  timezone?: string;
  clockSkewSeconds?: number;
  nativeSequence?: string;
  actor?: string;
  resource?: string;
  action?: string;
  outcome?: string;
  parserId?: string;
  parserVersion?: string;
  rawDigest?: string;
  confidence?: number;
  completeness: string;
  title: string;
  description: string;
}

export interface KubernetesAnalysisRun {
  runId: string;
  importSetId: string;
  scopeId: string;
  state: string;
  attempt: number;
  expectedMemberCount: number;
  parsedArtifactCount: number;
  failedArtifactCount: number;
  diagnostics: string[];
  startedAt: string;
  finishedAt?: string;
}
