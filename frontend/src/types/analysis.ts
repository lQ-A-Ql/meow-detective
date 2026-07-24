export type AnalysisParseStatus =
  | 'parsed'
  | 'partial'
  | 'notParsed'
  | 'unavailable'
  | 'candidateFound'
  | 'notFound'
  | 'failed';

export interface AnalysisProvenance {
  dataSourceId: string;
  artifactPath: string;
  parser: string;
  parsedAt: string;
  status: AnalysisParseStatus;
  warnings: string[];
}

export interface AnalysisFieldProvenance {
  field: string;
  valueName: string;
  keyPath: string;
  hivePath: string;
  parser: string;
}

export interface AnalysisSystemInfo {
  computerName?: string;
  osVersion?: string;
  buildNumber?: string;
  installDate?: string;
  registeredOwner?: string;
  organization?: string;
  productId?: string;
  networkAdapters: AnalysisNetworkAdapter[];
  bootHistory: AnalysisBootRecord[];
  timezone?: string;
  shutdownTime?: string;
  language?: string;
  status: AnalysisParseStatus;
  warnings: string[];
  provenance: AnalysisProvenance[];
  fieldProvenance: AnalysisFieldProvenance[];
}

export interface AnalysisNetworkAdapter {
  name: string;
  macAddress?: string;
  ipAddresses: string[];
  dhcpEnabled?: boolean;
  dhcpServer?: string;
}

export interface AnalysisBootRecord {
  timestamp: string;
  bootType: string;
  source: string;
  eventId?: number;
  recordId?: number;
  note?: string;
  details?: Record<string, string>;
  provenance: AnalysisProvenance;
}

export interface AnalysisFileClassification {
  category: string;
  files: AnalysisClassifiedFile[];
  fileCount: number;
  totalSize: number;
  status: AnalysisParseStatus;
  warnings: string[];
  provenance: AnalysisProvenance[];
}

export interface AnalysisClassifiedFile {
  fileId: string;
  path: string;
  name: string;
  size: number;
  fileType: string;
  magicDescription: string;
  provenance: AnalysisProvenance;
}

export interface EvidenceClassificationSummary {
  status: AnalysisParseStatus;
  categories: EvidenceCategory[];
  totals: EvidenceClassificationTotals;
  generatedAt: string;
  warnings: string[];
}

export interface EvidenceClassificationTotals {
  categoryCount: number;
  candidateFileCount: number;
  totalSize: number;
  artifactCount: number;
}

export interface EvidenceCategory {
  category: string;
  displayName: string;
  status: AnalysisParseStatus;
  fileCount: number;
  totalSize: number;
  artifactCount: number;
  confidence: number;
  sources: EvidenceSource[];
  warnings: string[];
  provenance: AnalysisProvenance[];
}

export interface EvidenceSource {
  fileId: string;
  path: string;
  size: number;
  evidenceKind: string;
  parser: string;
  status: AnalysisParseStatus;
  artifactCount: number;
  warnings: string[];
}

export interface AnalysisExtractionRequest {
  dataSourceId: string;
  categories: string[];
}

export type AnalysisExtractionPhase =
  | 'discovering'
  | 'preparing'
  | 'extracting'
  | 'persisting'
  | 'completed'
  | 'failed';

export interface AnalysisExtractionProgress {
  runId: string;
  caseId: string;
  dataSourceId: string;
  category: string;
  label: string;
  phase: AnalysisExtractionPhase;
  totalCandidates: number;
  processedCandidates: number;
  structuredCandidates: number;
  unsupportedCandidates: number;
  textFallbackCandidates: number;
  warningCandidates: number;
  checkpointHitCount: number;
  artifactCount: number;
  timelineEventCount: number;
  currentPath: string | null;
  detail: string;
}

export interface AnalysisExtractionPageRequest {
  dataSourceId: string;
  offset?: number;
  limit?: number;
}

export interface EvtxEventPageRequest extends AnalysisExtractionPageRequest {
  view?: import('./eventLog').EvtxEventView;
}

export interface AnalysisExtractionRun {
  status: AnalysisParseStatus;
  scannedCount: number;
  checkpointHitCount: number;
  artifactCount: number;
  timelineEventCount: number;
  sections: AnalysisExtractionSectionRun[];
  generatedAt: string;
  warnings: string[];
}

export interface AnalysisExtractionSectionRun {
  key: string;
  label: string;
  status: AnalysisParseStatus;
  scannedCount: number;
  artifactCount: number;
  timelineEventCount: number;
  warnings: string[];
}

export interface ClassifiedFileRow {
  fileId: string;
  name: string;
  path: string;
  size: number;
  magicType?: string;
  classificationSource: 'magic' | 'metadata' | string;
}

export interface ClassificationSubcategory {
  name: string;
  fileCount: number;
  totalSize: number;
  files: ClassifiedFileRow[];
  truncated: boolean;
}

export interface ClassificationGroup {
  category: string;
  displayName: string;
  fileCount: number;
  totalSize: number;
  subcategories: ClassificationSubcategory[];
}

export interface FileClassificationBoard {
  status: AnalysisParseStatus;
  generatedAt: string;
  totalFiles: number;
  totalSize: number;
  magicClassifiedCount: number;
  metadataClassifiedCount: number;
  groups: ClassificationGroup[];
  warnings?: string[];
}
