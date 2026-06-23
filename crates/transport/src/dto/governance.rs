use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum VerificationGuaranteeLevelDto {
    Guaranteed,
    BestEffort,
    Experimental,
    NotGuaranteed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SupportMaturityDto {
    Ga,
    Beta,
    Experimental,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum KnownLimitationStatusDto {
    Partial,
    Unsupported,
    NotGuaranteed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum VerificationResultDto {
    Passed,
    Partial,
    Pending,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VerificationChainStatusDto {
    pub chain: String,
    pub display_name: String,
    pub maturity: SupportMaturityDto,
    pub guarantee_level: VerificationGuaranteeLevelDto,
    pub fixture_tier: String,
    pub expected_json_version: String,
    pub verified_sample_count: u32,
    pub result: VerificationResultDto,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ParserSupportMatrixSummaryDto {
    pub ga_count: u32,
    pub beta_count: u32,
    pub experimental_count: u32,
    pub unsupported_count: u32,
    pub documented_limit_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ParserSupportMatrixEntryDto {
    pub chain: String,
    pub platform: String,
    pub maturity: SupportMaturityDto,
    pub verified_samples: Vec<String>,
    pub baseline: String,
    pub guarantee_summary: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KnownLimitationDto {
    pub category: String,
    pub item: String,
    pub status: KnownLimitationStatusDto,
    pub summary: String,
    pub affected_chains: Vec<String>,
    pub source_doc: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkSnapshotDto {
    pub dataset_level: String,
    pub scenario: String,
    pub p95_ms: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_peak_mb: Option<u32>,
    pub baseline_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BenchmarkRequirementStatusDto {
    Covered,
    Missing,
    Exceeded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkRequiredCheckDto {
    pub dataset_level: String,
    pub scenario: String,
    pub threshold_p95_ms: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub measured_p95_ms: Option<u32>,
    pub status: BenchmarkRequirementStatusDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkSummaryDto {
    pub host_profile: String,
    pub baseline_version: String,
    pub last_verified_at: String,
    pub scenarios: Vec<BenchmarkSnapshotDto>,
    pub required_checks: Vec<BenchmarkRequiredCheckDto>,
    pub covered_required_count: u32,
    pub missing_required_count: u32,
    pub exceeded_required_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SecurityAuditEntryDto {
    pub action: String,
    pub resource_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub sensitive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SecurityAuditSummaryDto {
    pub export_overwrite_default: bool,
    pub export_path_guard_enabled: bool,
    pub stdio_command_whitelist_enforced: bool,
    pub sse_https_only: bool,
    pub embedded_credentials_blocked: bool,
    pub media_handle_scoped: bool,
    pub error_redaction_enabled: bool,
    pub audit_log_required: bool,
    pub audit_event_count: u32,
    pub sensitive_audit_event_count: u32,
    pub recent_audit_entries: Vec<SecurityAuditEntryDto>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ErrorTaxonomyEntryDto {
    pub category: String,
    pub severity: String,
    pub recoverable: bool,
    pub examples: Vec<String>,
    pub redaction_rule: String,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ReleaseGateStatusDto {
    Passed,
    Warning,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseGateEntryDto {
    pub gate_id: String,
    pub title: String,
    pub status: ReleaseGateStatusDto,
    pub evidence: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseScoreBreakdownEntryDto {
    pub dimension: String,
    pub max_score: u32,
    pub actual_score: u32,
    pub deductions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseScorecardDto {
    pub total_score: u32,
    pub grade: String,
    pub verification_score: u32,
    pub correlation_score: u32,
    pub performance_score: u32,
    pub security_score: u32,
    pub breakdown: Vec<ReleaseScoreBreakdownEntryDto>,
    pub blockers: Vec<String>,
    pub residual_risks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CorrelationCoverageStatusDto {
    Covered,
    Review,
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CorrelationFamilyCoverageDto {
    pub family: String,
    pub display_name: String,
    pub status: CorrelationCoverageStatusDto,
    pub lead_count: u32,
    pub high_confidence_lead_count: u32,
    pub review_lead_count: u32,
    pub cluster_count: u32,
    pub sample_signals: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceRuntimeSignalsDto {
    pub data_source_count: u32,
    pub hashed_data_source_count: u32,
    pub pending_hash_data_source_count: u32,
    pub warning_data_source_count: u32,
    pub running_job_count: u32,
    pub partial_job_count: u32,
    pub failed_job_count: u32,
    pub report_count: u32,
    pub correlation_snapshot_available: bool,
    pub correlation_lead_count: u32,
    pub correlation_high_confidence_lead_count: u32,
    pub correlation_review_lead_count: u32,
    pub correlation_cluster_count: u32,
    pub correlation_rule_family_count: u32,
    pub correlation_covered_family_count: u32,
    pub correlation_high_confidence_family_count: u32,
    pub correlation_family_coverage: Vec<CorrelationFamilyCoverageDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceFactSourceDto {
    pub area: String,
    pub fact_file: String,
    pub fact_kind: String,
    pub derived_outputs: Vec<String>,
    pub last_verified_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceRuntimeSubcheckDto {
    pub check_id: String,
    pub title: String,
    pub status: ReleaseGateStatusDto,
    pub evidence: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceRuntimeCheckDto {
    pub check_id: String,
    pub title: String,
    pub status: ReleaseGateStatusDto,
    pub evidence: String,
    pub detail: String,
    pub checked_at: String,
    pub sub_checks: Vec<GovernanceRuntimeSubcheckDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceRuntimeResultsDto {
    pub checked_at: String,
    pub checks: Vec<GovernanceRuntimeCheckDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct V2GovernanceSnapshotDto {
    pub generated_at: String,
    pub fact_sources: Vec<GovernanceFactSourceDto>,
    pub runtime_results: GovernanceRuntimeResultsDto,
    pub verification_chains: Vec<VerificationChainStatusDto>,
    pub support_matrix: ParserSupportMatrixSummaryDto,
    pub support_matrix_entries: Vec<ParserSupportMatrixEntryDto>,
    pub known_limitations: Vec<KnownLimitationDto>,
    pub benchmark: BenchmarkSummaryDto,
    pub security: SecurityAuditSummaryDto,
    pub error_taxonomy_entries: Vec<ErrorTaxonomyEntryDto>,
    pub release_gates: Vec<ReleaseGateEntryDto>,
    pub release_scorecard: ReleaseScorecardDto,
    pub runtime_signals: GovernanceRuntimeSignalsDto,
}
