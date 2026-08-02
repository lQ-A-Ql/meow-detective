import type { BatchStatus } from './batch';
import type { FamilyCount } from './artifacts';
import type { DataSourceSummary } from './dataSource';
import type { GraphStats } from './graph';
import type { NotebookStats } from './notebook';

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

export interface PlatformCoverage {
  windowsArtifactFamilies: number;
  linuxArtifactFamilies: number;
  crossPlatformArtifactFamilies: number;
  unknownArtifactFamilies: number;
  totalFamilies: number;
  windowsFamilies: string[];
  linuxFamilies: string[];
  crossPlatformFamilies: string[];
  unknownFamilies: string[];
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
  loadStatus: string;
  executionStatus: string;
}

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

export interface V3GovernanceSnapshot extends V2GovernanceSnapshot {
  graphStatistics: GraphStats;
  platformCoverage: PlatformCoverage;
  rulePackCoverage: RulePackStatus;
  batchStatus: BatchStatus;
  notebookStats: NotebookStats;
}

export interface CorrelationOverview {
  nodeCount: number;
  edgeCount: number;
  clusterCount: number;
  leadCount: number;
  familyCoverage: CorrelationFamilyCoverage[];
}

export interface CaseOverviewSnapshot {
  generatedAt: string;
  dataSources: DataSourceSummary[];
  timelineEventCount: number;
  artifactFamilyCounts: FamilyCount[];
  correlationStatistics: CorrelationOverview;
  platformCoverage: PlatformCoverage;
  rulePackCoverage: RulePackStatus;
  batchStatus: BatchStatus;
}
