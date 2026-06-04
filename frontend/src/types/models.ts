export type ApiMode = 'mock' | 'tauri';

export interface ApiErrorDto {
  code: string;
  message: string;
  details?: unknown;
  recoverable: boolean;
}

export type EventTopic =
  | 'case-opened'
  | 'case-closed'
  | 'job-created'
  | 'job-started'
  | 'job-progress'
  | 'job-completed'
  | 'job-failed'
  | 'job-cancelled'
  | 'data-source-imported'
  | 'artifact-added'
  | 'timeline-updated'
  | 'search-index_progress'
  | 'partition-progress';

export interface EventEnvelope<T = unknown> {
  eventId: string;
  topic: EventTopic;
  ts: string;
  payload: T;
}

export interface CaseSummary {
  id: string;
  name: string;
  number?: string;
  examiner?: string;
  createdAt: string;
  updatedAt: string;
}

export interface CaseMetrics {
  dataSourceCount: number;
  indexedFileCount: number;
  timelineEventCount: number;
  artifactCount: number;
}

export interface RecentObject {
  id: string;
  title: string;
  detail: string;
  time: string;
  kind: string;
}

export interface DataSourceSummary {
  id: string;
  name: string;
  kind: 'e01' | 'raw' | 'logical_directory' | string;
  sourcePath: string;
  importedAt: string;
  fileCount?: number;
  partitions: DataSourcePartition[];
}

export interface DataSourcePartition {
  index: number;
  name: string;
  kindLabel: string;
  status: string;
  offset: number;
  length: number;
  typeGuid?: string;
  filesystem?: string;
  unlockHint?: string;
}

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

export interface RecentCase {
  caseRoot: string;
  name: string;
  openedAt: string;
}

export interface FileTreeNode {
  id: string;
  name: string;
  depth: number;
  hasChildren: boolean;
  entryType?: 'file' | 'directory';
  size?: number;
  nodeType?: string;
  status?: string;
  expanded?: boolean;
  active?: boolean;
}

export interface FileChildrenPage {
  children: FileTreeNode[];
  totalCount: number;
  offset?: number;
  limit?: number;
  truncated?: boolean;
}

export interface FileEntryRow {
  id: string;
  parentId?: string;
  path: string;
  name: string;
  entryType: 'file' | 'directory';
  size?: number;
  ext?: string;
  deleted: boolean;
  createdAt?: string;
  modifiedAt?: string;
  accessedAt?: string;
  changedAt?: string;
  hashSha256?: string;
}

export interface FileRowsPage {
  rows: FileEntryRow[];
  totalCount: number;
  offset: number;
  limit: number;
  truncated: boolean;
}

export interface SearchSnippet {
  text: string;
  highlights: Array<{ start: number; end: number }>;
}

export interface SearchHit {
  fileId: string;
  path: string;
  score: number;
  snippets: SearchSnippet[];
}

export interface SearchResultPage {
  total: number;
  tookMs: number;
  items: SearchHit[];
}

export interface TimelineEventDto {
  id: string;
  sourceObjectId: string;
  eventType: string;
  ts: string;
  title: string;
  description: string;
  attrs: Record<string, unknown>;
}

export interface ArtifactRow {
  id: string;
  artifactType: string;
  title: string;
  summary: string;
  sourceObjectId?: string;
  createdAt: string;
  attrs: Record<string, unknown>;
}

export interface FamilyCount {
  family: string;
  count: number;
}

export interface JobSnapshot {
  id: string;
  name: string;
  scope: string;
  progress: number;
  status: 'pending' | 'running' | 'completed' | 'failed' | 'warning';
  detail: string;
  warningCount: number;
  skippedCount: number;
  failedCount: number;
  partial: boolean;
  currentPartition?: string;
  completedPartitions?: number;
  totalPartitions?: number;
  partitionProgress?: number;
}

export interface WarningItem {
  id: string;
  title: string;
  detail: string;
}

export interface TraceItem {
  id: string;
  ts: string;
  message: string;
}

export interface ViewerHandle {
  handleId: string;
  size: number;
  mime?: string;
}

export interface ViewerRangeRequest {
  handleId: string;
  offset: number;
  length: number;
}

export interface ViewerRangeResponse {
  kind: 'hex' | 'text';
  lines: string[];
  encoding?: string;
}

export interface MediaUrl {
  url?: string;
  handleId?: string;
  mimeType: string;
  size: number;
  canReadRanges: boolean;
  mode?: 'inline' | 'protocol' | 'rangeFallback';
  previewMode?: 'inline' | 'protocol' | 'rangeFallback' | 'range';
  previewBytes?: number;
}

export interface MediaRangeRequest {
  handleId: string;
  offset: number;
  length: number;
}

export interface MediaRangeResponse {
  offset: number;
  bytesBase64: string;
  bytesRead: number;
  eof: boolean;
}

export interface AppSettings {
  caseRoot: string;
  imageSearchPaths: string[];
  theme: 'light' | 'dark';
  devEventTrace: boolean;
  maxImportWorkers?: number;
  maxAnalysisWorkers?: number;
  importAnalysisMode?: 'metadataOnly' | 'budgetedContent' | 'fullContent';
}

export interface ReportTemplate {
  id: string;
  name: string;
  description: string;
}

export interface ReportHistoryItem {
  id: string;
  fileName: string;
  createdBy: string;
  createdAt: string;
  status: 'completed' | 'running';
  progress?: number;
}

export interface ExportScope {
  fileSystemMetadata: boolean;
  registry: boolean;
  fullTimeline: boolean;
  rawFileExtraction: boolean;
}
