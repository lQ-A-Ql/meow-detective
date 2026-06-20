export type ApiMode = 'mock' | 'tauri';

export interface ApiErrorDto {
  code: string;
  message: string;
  category?: ErrorCategory;
  details?: unknown;
  recoverable: boolean;
}

export type ErrorCategory =
  | 'validation'
  | 'unsupported'
  | 'io'
  | 'parser'
  | 'security'
  | 'external'
  | 'timeout'
  | 'internal';

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
  | 'partition-progress'
  | 'import-phase-progress'
  | 'import-partial-result'
  | 'job-cancellation'
  | 'cache-index-status'
  | 'performance-report-ready';

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
  sourceHash?: string;
  hashStatus?: string;
  canonicalPath?: string;
  evidenceSize?: number;
  readerKind?: string;
  provenanceStatus?: string;
  warnings?: string[];
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

export interface AnalysisExtractionRequest {
  categories: string[];
}

export interface AnalysisExtractionPageRequest {
  offset?: number;
  limit?: number;
}

export interface AnalysisExtractionRun {
  status: AnalysisParseStatus;
  scannedCount: number;
  artifactCount: number;
  timelineEventCount: number;
  generatedAt: string;
  warnings: string[];
}

export interface RegistryExtractionSummary {
  status: AnalysisParseStatus;
  total: number;
  values: RegistryValue[];
  generatedAt: string;
  warnings: string[];
}

export interface RegistryValue {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  hivePath: string;
  keyPath: string;
  valueName: string;
  valueType: string;
  data: string;
  parser: string;
  createdAt: string;
}

export interface RegistryRunKey {
  keyPath: string;
  valueName: string;
  command: string;
  timestamp?: string;
}

export interface RecentDoc {
  fileName: string;
  extension: string;
  lastAccessed?: string;
  lnkTarget?: string;
}

export interface UserAssistEntry {
  executable: string;
  runCount: number;
  lastRun?: string;
  focusTimeMs: number;
}

export interface MountPoint {
  driveLetter?: string;
  volumeGuid?: string;
  lastMounted?: string;
}

export interface NtuserInfo {
  runKeys: RegistryRunKey[];
  recentDocs: RecentDoc[];
  userAssist: UserAssistEntry[];
  typedUrls: string[];
  wordWheelQuery: string[];
  mountPoints: MountPoint[];
  warnings: string[];
}

export type BrowserKind = 'Chrome' | 'Edge' | 'Firefox' | string;

export interface BrowserHistorySummary {
  status: AnalysisParseStatus;
  visitTotal: number;
  downloadTotal: number;
  visits: BrowserVisit[];
  downloads: BrowserDownload[];
  generatedAt: string;
  warnings: string[];
}

export interface BrowserVisit {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  browser: BrowserKind;
  profile: string;
  url: string;
  title: string;
  visitTime?: string;
  visitCount: number;
}

export interface BrowserDownload {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  browser: BrowserKind;
  profile: string;
  url: string;
  targetPath: string;
  startTime?: string;
  totalBytes: number;
}

export interface EmailExtractionSummary {
  status: AnalysisParseStatus;
  total: number;
  messages: EmailMessage[];
  generatedAt: string;
  warnings: string[];
}

export interface EmailMessage {
  artifactId: string;
  fileId: string;
  sourcePath: string;
  sentAt?: string;
  from: string;
  to: string[];
  cc: string[];
  bcc: string[];
  subject: string;
  messageId: string;
  attachments: string[];
  bodyPreview: string;
}

export type VerificationGuaranteeLevel =
  | 'guaranteed'
  | 'bestEffort'
  | 'experimental'
  | 'notGuaranteed';

export type SupportMaturity = 'ga' | 'beta' | 'experimental' | 'unsupported';

export type VerificationResult = 'passed' | 'partial' | 'pending' | 'failed';

export interface VerificationChainStatus {
  chain: string;
  displayName: string;
  maturity: SupportMaturity;
  guaranteeLevel: VerificationGuaranteeLevel;
  fixtureTier: string;
  expectedJsonVersion: string;
  verifiedSampleCount: number;
  result: VerificationResult;
  notes: string[];
}

export interface ParserSupportMatrixSummary {
  gaCount: number;
  betaCount: number;
  experimentalCount: number;
  unsupportedCount: number;
  documentedLimitCount: number;
}

export interface ParserSupportMatrixEntry {
  chain: string;
  platform: string;
  maturity: SupportMaturity;
  verifiedSamples: string[];
  baseline: string;
  guaranteeSummary: string;
  notes: string[];
}

export type KnownLimitationStatus = 'partial' | 'unsupported' | 'notGuaranteed';

export interface KnownLimitation {
  category: string;
  item: string;
  status: KnownLimitationStatus;
  summary: string;
  affectedChains: string[];
  sourceDoc: string;
}

export interface BenchmarkSnapshot {
  datasetLevel: string;
  scenario: string;
  p95Ms: number;
  memoryPeakMb?: number;
  baselineVersion: string;
}

export type BenchmarkRequirementStatus = 'covered' | 'missing' | 'exceeded';

export interface BenchmarkRequiredCheck {
  datasetLevel: string;
  scenario: string;
  thresholdP95Ms: number;
  measuredP95Ms?: number;
  status: BenchmarkRequirementStatus;
}

export interface BenchmarkSummary {
  hostProfile: string;
  baselineVersion: string;
  lastVerifiedAt: string;
  scenarios: BenchmarkSnapshot[];
  requiredChecks: BenchmarkRequiredCheck[];
  coveredRequiredCount: number;
  missingRequiredCount: number;
  exceededRequiredCount: number;
}

export interface SecurityAuditSummary {
  exportOverwriteDefault: boolean;
  exportPathGuardEnabled: boolean;
  stdioCommandWhitelistEnforced: boolean;
  sseHttpsOnly: boolean;
  embeddedCredentialsBlocked: boolean;
  mediaHandleScoped: boolean;
  errorRedactionEnabled: boolean;
  auditLogRequired: boolean;
  auditEventCount: number;
  sensitiveAuditEventCount: number;
  recentAuditEntries: SecurityAuditEntry[];
  notes: string[];
}

export interface SecurityAuditEntry {
  action: string;
  resourceType: string;
  resourceId?: string;
  createdAt: string;
  summary?: string;
  sensitive: boolean;
}

export interface ErrorTaxonomyEntry {
  category: string;
  severity: string;
  recoverable: boolean;
  examples: string[];
  redactionRule: string;
  notes: string[];
}

export type ReleaseGateStatus = 'passed' | 'warning' | 'blocked';

export interface ReleaseGateEntry {
  gateId: string;
  title: string;
  status: ReleaseGateStatus;
  evidence: string;
  detail: string;
}

export interface ReleaseScoreBreakdownEntry {
  dimension: string;
  maxScore: number;
  actualScore: number;
  deductions: string[];
}

export interface ReleaseScorecard {
  totalScore: number;
  grade: string;
  verificationScore: number;
  correlationScore: number;
  performanceScore: number;
  securityScore: number;
  breakdown: ReleaseScoreBreakdownEntry[];
  blockers: string[];
  residualRisks: string[];
}

export type CorrelationCoverageStatus = 'covered' | 'review' | 'missing';

export interface CorrelationFamilyCoverage {
  family: string;
  displayName: string;
  status: CorrelationCoverageStatus;
  leadCount: number;
  highConfidenceLeadCount: number;
  reviewLeadCount: number;
  clusterCount: number;
  sampleSignals: string[];
}

export interface GovernanceRuntimeSignals {
  dataSourceCount: number;
  hashedDataSourceCount: number;
  pendingHashDataSourceCount: number;
  warningDataSourceCount: number;
  runningJobCount: number;
  partialJobCount: number;
  failedJobCount: number;
  reportCount: number;
  correlationSnapshotAvailable: boolean;
  correlationLeadCount: number;
  correlationHighConfidenceLeadCount: number;
  correlationReviewLeadCount: number;
  correlationClusterCount: number;
  correlationRuleFamilyCount: number;
  correlationCoveredFamilyCount: number;
  correlationHighConfidenceFamilyCount: number;
  correlationFamilyCoverage: CorrelationFamilyCoverage[];
}

export interface GovernanceFactSource {
  area: string;
  factFile: string;
  factKind: string;
  derivedOutputs: string[];
  lastVerifiedAt: string;
}

export interface GovernanceRuntimeCheck {
  checkId: string;
  title: string;
  status: ReleaseGateStatus;
  evidence: string;
  detail: string;
  checkedAt: string;
  subChecks: GovernanceRuntimeSubcheck[];
}

export interface GovernanceRuntimeResults {
  checkedAt: string;
  checks: GovernanceRuntimeCheck[];
}

export interface GovernanceRuntimeSubcheck {
  checkId: string;
  title: string;
  status: ReleaseGateStatus;
  evidence: string;
  detail: string;
}

export interface V2GovernanceSnapshot {
  generatedAt: string;
  factSources: GovernanceFactSource[];
  runtimeResults: GovernanceRuntimeResults;
  verificationChains: VerificationChainStatus[];
  supportMatrix: ParserSupportMatrixSummary;
  supportMatrixEntries: ParserSupportMatrixEntry[];
  knownLimitations: KnownLimitation[];
  benchmark: BenchmarkSummary;
  security: SecurityAuditSummary;
  errorTaxonomyEntries: ErrorTaxonomyEntry[];
  releaseGates: ReleaseGateEntry[];
  releaseScorecard: ReleaseScorecard;
  runtimeSignals: GovernanceRuntimeSignals;
}

// ── V3 Governance types ─────────────────────────────────────────────────

export interface GraphStats {
  nodeCountByType: Record<string, number>;
  edgeCountByType: Record<string, number>;
  totalNodes: number;
  totalEdges: number;
  density: number;
  largestComponentSize: number;
}

export interface PlatformCoverage {
  windowsArtifactFamilies: number;
  linuxArtifactFamilies: number;
  macosArtifactFamilies: number;
  crossPlatformArtifactFamilies: number;
  totalFamilies: number;
  windowsFamilies: string[];
  linuxFamilies: string[];
  macosFamilies: string[];
  crossPlatformFamilies: string[];
}

export interface RulePackInfo {
  name: string;
  version: string;
  author: string;
  ruleCount: number;
  scope: string[];
}

export interface RulePackStatus {
  loadedPacks: RulePackInfo[];
  totalRuleCount: number;
  executionStatus: string;
}

export interface BatchStatus {
  activeJobs: number;
  completedJobs: number;
  failedJobs: number;
  queuedJobs: number;
  totalJobs: number;
}

export interface NotebookStats {
  entryCount: number;
  citationCount: number;
}

export interface V3GovernanceSnapshot extends V2GovernanceSnapshot {
  graphStatistics: GraphStats;
  platformCoverage: PlatformCoverage;
  rulePackCoverage: RulePackStatus;
  batchStatus: BatchStatus;
  notebookStats: NotebookStats;
}

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
  deleted?: boolean;
  hidden: boolean;
  system: boolean;
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
  hidden: boolean;
  system: boolean;
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

export interface FileJumpContext {
  target: FileEntryRow;
  directory: FileEntryRow;
  ancestorDirectoryIds: string[];
  rowOffset: number;
  requiresShowHidden: boolean;
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
  parserId?: string;
  parserVersion?: string;
  confidence?: number;
  sourceAttribution?: string;
  attrs: Record<string, unknown>;
}

export interface ArtifactRow {
  id: string;
  artifactType: string;
  title: string;
  summary: string;
  sourceObjectId?: string;
  createdAt: string;
  extractorId?: string;
  extractorVersion?: string;
  confidence?: number;
  sourceAttribution?: string;
  attrs: Record<string, unknown>;
}

export interface FamilyCount {
  family: string;
  count: number;
}

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

export type ImportPhase =
  | 'queued'
  | 'attach'
  | 'probe'
  | 'enumerate'
  | 'mergeEnumeration'
  | 'analyze'
  | 'mergeAnalysis'
  | 'hashEvidence'
  | 'buildIndexes'
  | 'finalize';

export type ImportPhaseState =
  | 'pending'
  | 'running'
  | 'completed'
  | 'skipped'
  | 'cancelling'
  | 'cancelled'
  | 'failed'
  | 'partial';

export interface ImportPhaseMetrics {
  elapsedMs: number;
  rssMb: number;
  workers: number;
  rowsProcessed: number;
  rowsTotal?: number;
  rowsPerSec?: number;
  bytesProcessed: number;
  bytesTotal?: number;
  mbPerSec?: number;
  warnings: number;
  skipped: number;
  failed: number;
}

export interface ImportPhaseProgress {
  jobId: string;
  caseId: string;
  dataSourceId?: string;
  phase: ImportPhase;
  state: ImportPhaseState;
  percent: number;
  detail: string;
  metrics: ImportPhaseMetrics;
  partialResults: PartialResult[];
  cancellable: boolean;
  cancelRequested: boolean;
}

export type PartialResultKind =
  | 'fileTree'
  | 'fileRows'
  | 'partition'
  | 'timelineEvents'
  | 'timelineBuckets'
  | 'artifactFamily'
  | 'searchIndex'
  | 'evidenceHash';

export type ResultFreshness = 'ready' | 'partial' | 'deferred' | 'stale' | 'invalidated';

export interface PartialResult {
  kind: PartialResultKind;
  scopeId: string;
  readyCount: number;
  totalEstimate?: number;
  queryKey: string;
  freshness: ResultFreshness;
}

export type CancelReason = 'userRequested' | 'caseClosing' | 'memoryLimit' | 'superseded';

export interface CancelJobRequest {
  jobId: string;
  reason: CancelReason;
  drainTimeoutMs: number;
}

export type CancellationState = 'notRequested' | 'requested' | 'acknowledged' | 'draining' | 'cancelled' | 'timedOut';

export interface JobCancellation {
  jobId: string;
  requestedAt?: string;
  acknowledgedAt?: string;
  state: CancellationState;
  safeToClose: boolean;
  detail: string;
}

export interface IndexCacheStatus {
  cacheKey: string;
  state: string;
  indexedCount: number;
  totalCount?: number;
  updatedAt: string;
  message?: string;
}

export interface PerformanceReportSummary {
  reportId: string;
  jobId?: string;
  generatedAt: string;
  elapsedMs: number;
  peakMemoryBytes?: number;
  summary: string;
}

export interface PerformanceMetric {
  key: string;
  value: number;
  unit: string;
}

export interface PerformanceReport {
  summary: PerformanceReportSummary;
  metrics: PerformanceMetric[];
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

export interface ExportOptions {
  overwrite?: boolean;
}

// ── Batch ──

export type BatchPhaseName = 'Mount' | 'Catalog' | 'ExtractArtifacts' | 'Index' | 'Correlate' | 'Export';

export type BatchPhaseState = 'pending' | 'running' | 'completed' | 'failed' | 'skipped';

export type BatchJobStatus = 'pending' | 'running' | 'paused' | 'completed' | 'failed' | 'cancelled';

export interface ResourceLimits {
  memoryMb: number;
  threadCount: number;
}

export interface BatchPlan {
  name: string;
  dataSourceIds: string[];
  phases: BatchPhaseName[];
  resourceLimits: ResourceLimits;
}

export interface BatchPlanSummary {
  name: string;
  dataSourceIds: string[];
  dataSourceCount: number;
  phases: BatchPhaseName[];
  phaseCount: number;
  resourceLimits: ResourceLimits;
}

export interface BatchPhaseProgress {
  phase: BatchPhaseName;
  state: BatchPhaseState;
  progress: number;
  detail: string;
}

export interface BatchJobLogLine {
  ts: string;
  level: 'info' | 'warn' | 'error';
  message: string;
}

export interface BatchJob {
  id: string;
  name: string;
  status: BatchJobStatus;
  progress: number;
  phases: BatchPhaseProgress[];
  plan: BatchPlanSummary;
  createdAt: string;
  startedAt?: string;
  completedAt?: string;
  elapsedMs?: number;
  etaMs?: number;
  fileCount: number;
  artifactCount: number;
  logTail: BatchJobLogLine[];
}

// ── Rule Packs ──

export interface RulePackSummary {
  id: string;
  name: string;
  version: string;
  author?: string;
  description?: string;
  status: 'loaded' | 'error' | 'validating';
  ruleCount: number;
  loadedAt: string;
  warnings: string[];
  errors: string[];
  coveredFamilies: string[];
}

export interface RulePackValidationResult {
  packId: string;
  valid: boolean;
  errors: string[];
  warnings: string[];
  coverage: RulePackCoverage;
}

export interface RulePackCoverage {
  coveredFamilies: string[];
  uncoveredFamilies: string[];
  coveragePercent: number;
}

// ── Notebook ──

export type NotebookEntryType = 'note' | 'observation' | 'finding' | 'lead';

export type NotebookEntryStatus = 'draft' | 'review' | 'final';

export interface NotebookEntry {
  id: string;
  caseId: string;
  parentId?: string;
  title: string;
  content: string;
  entryType: NotebookEntryType;
  status: NotebookEntryStatus;
  tags: string[];
  citationNodeIds: string[];
  createdAt: string;
  updatedAt: string;
}

export interface NotebookEntryListItem {
  id: string;
  parentId?: string;
  title: string;
  entryType: NotebookEntryType;
  status: NotebookEntryStatus;
  tags: string[];
  replyCount: number;
  createdAt: string;
  updatedAt: string;
}

export interface CreateEntryRequest {
  title: string;
  content: string;
  entryType: NotebookEntryType;
  tags?: string[];
  parentId?: string;
}

export interface UpdateEntryRequest {
  entryId: string;
  title?: string;
  content?: string;
  entryType?: NotebookEntryType;
  tags?: string[];
  status?: NotebookEntryStatus;
}

export interface AddCitationRequest {
  entryId: string;
  nodeIds: string[];
}
