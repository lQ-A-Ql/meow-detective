use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AnalysisParseStatusDto {
    Parsed,
    Partial,
    NotParsed,
    Unavailable,
    CandidateFound,
    NotFound,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisProvenanceDto {
    pub data_source_id: String,
    pub artifact_path: String,
    pub parser: String,
    pub parsed_at: String,
    pub status: AnalysisParseStatusDto,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisFieldProvenanceDto {
    pub field: String,
    pub value_name: String,
    pub key_path: String,
    pub hive_path: String,
    pub parser: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisSystemInfoDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub computer_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registered_owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
    pub network_adapters: Vec<AnalysisNetworkAdapterDto>,
    pub boot_history: Vec<AnalysisBootRecordDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub status: AnalysisParseStatusDto,
    pub warnings: Vec<String>,
    pub provenance: Vec<AnalysisProvenanceDto>,
    pub field_provenance: Vec<AnalysisFieldProvenanceDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisNetworkAdapterDto {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac_address: Option<String>,
    pub ip_addresses: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dhcp_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dhcp_server: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisBootRecordDto {
    pub timestamp: String,
    pub boot_type: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub provenance: AnalysisProvenanceDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisFileClassificationDto {
    pub category: String,
    pub files: Vec<AnalysisClassifiedFileDto>,
    pub file_count: u64,
    pub total_size: u64,
    pub status: AnalysisParseStatusDto,
    pub warnings: Vec<String>,
    pub provenance: Vec<AnalysisProvenanceDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceClassificationSummaryDto {
    pub status: AnalysisParseStatusDto,
    pub categories: Vec<EvidenceCategoryDto>,
    pub totals: EvidenceClassificationTotalsDto,
    pub generated_at: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceClassificationTotalsDto {
    pub category_count: u64,
    pub candidate_file_count: u64,
    pub total_size: u64,
    pub artifact_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceCategoryDto {
    pub category: String,
    pub display_name: String,
    pub status: AnalysisParseStatusDto,
    pub file_count: u64,
    pub total_size: u64,
    pub artifact_count: u64,
    pub confidence: f32,
    pub sources: Vec<EvidenceSourceDto>,
    pub warnings: Vec<String>,
    pub provenance: Vec<AnalysisProvenanceDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceSourceDto {
    pub file_id: String,
    pub path: String,
    pub size: u64,
    pub evidence_kind: String,
    pub parser: String,
    pub status: AnalysisParseStatusDto,
    pub artifact_count: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisClassifiedFileDto {
    pub file_id: String,
    pub path: String,
    pub name: String,
    pub size: u64,
    pub file_type: String,
    pub magic_description: String,
    pub provenance: AnalysisProvenanceDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisExtractionRunDto {
    pub status: AnalysisParseStatusDto,
    pub scanned_count: u64,
    pub artifact_count: u64,
    pub timeline_event_count: u64,
    pub generated_at: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryExtractionSummaryDto {
    pub status: AnalysisParseStatusDto,
    pub total: u64,
    pub values: Vec<RegistryValueDto>,
    pub generated_at: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryValueDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub hive_path: String,
    pub key_path: String,
    pub value_name: String,
    pub value_type: String,
    pub data: String,
    pub parser: String,
    pub created_at: String,
}

// SAM User Account (structured view)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SamUserAccountDto {
    pub username: String,
    pub rid: u32,
    pub rid_hex: String,
    pub groups: Vec<String>,
    pub login_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_login: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_created: Option<String>,
    pub account_status: String, // "enabled" | "disabled" | "locked"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_hash_type: Option<String>, // "NTLM" | "LM" | "Both"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_hint: Option<String>,
    pub data_source_id: String,
    pub hive_path: String,
    pub key_path: String,
    pub parser: String,
}

// Registry Hive Overview
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryHiveOverviewDto {
    pub hive_name: String,
    pub status: AnalysisParseStatusDto,
    pub key_value_count: u64,
    pub extracted_at: String,
    pub data_source_id: String,
    pub source_path: String,
    pub txlog_merged: bool,
    pub deleted_keys_found: u32,
}

// UserAssist Entry (structured view)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAssistEntryDto {
    pub program_path: String,
    pub exec_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_exec_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_suspicious: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suspicious_reason: Option<String>,
}

// Network Profile (structured view)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkProfileDto {
    pub ssid: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_connect: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_connect: Option<String>,
    pub connect_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// Installed Software (structured view)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSoftwareDto {
    pub display_name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_suspicious: Option<bool>,
}

// USB Device History (structured view)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsbDeviceHistoryDto {
    pub device_name: String,
    pub serial_number: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_connect: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_connect: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drive_letter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_suspicious: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suspicious_reason: Option<String>,
}

// Registry Structured Summary (aggregates all structured views)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryStructuredSummaryDto {
    pub hive_overviews: Vec<RegistryHiveOverviewDto>,
    pub sam_users: Vec<SamUserAccountDto>,
    pub user_assist_entries: Vec<UserAssistEntryDto>,
    pub network_profiles: Vec<NetworkProfileDto>,
    pub installed_software: Vec<InstalledSoftwareDto>,
    pub usb_devices: Vec<UsbDeviceHistoryDto>,
    pub status: AnalysisParseStatusDto,
    pub generated_at: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHistorySummaryDto {
    pub status: AnalysisParseStatusDto,
    pub visit_total: u64,
    pub download_total: u64,
    pub visits: Vec<BrowserVisitDto>,
    pub downloads: Vec<BrowserDownloadDto>,
    pub generated_at: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserVisitDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub browser: String,
    pub profile: String,
    pub url: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visit_time: Option<String>,
    pub visit_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserDownloadDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub browser: String,
    pub profile: String,
    pub url: String,
    pub target_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<String>,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserCookieDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub browser: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub domain: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry: Option<String>,
    pub secure: bool,
    pub http_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub same_site: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSessionTabDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub browser: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub window_index: i32,
    pub tab_index: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_active: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailExtractionSummaryDto {
    pub status: AnalysisParseStatusDto,
    pub total: u64,
    pub messages: Vec<EmailMessageDto>,
    pub generated_at: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailMessageDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sent_at: Option<String>,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub message_id: String,
    pub attachments: Vec<String>,
    pub body_preview: String,
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_info_serializes_camel_case_and_status() {
        let dto = AnalysisSystemInfoDto {
            computer_name: Some("host".to_string()),
            os_version: None,
            build_number: None,
            install_date: None,
            registered_owner: None,
            organization: None,
            product_id: None,
            network_adapters: vec![AnalysisNetworkAdapterDto {
                name: "Ethernet".to_string(),
                mac_address: Some("00:11:22:33:44:55".to_string()),
                ip_addresses: vec!["192.0.2.10".to_string()],
                dhcp_enabled: Some(true),
                dhcp_server: None,
            }],
            boot_history: vec![AnalysisBootRecordDto {
                timestamp: "2026-01-01T00:00:00Z".to_string(),
                boot_type: "eventLogStarted".to_string(),
                source: "Windows/System32/winevt/Logs/System.evtx".to_string(),
                event_id: Some(6005),
                record_id: Some(42),
                note: Some("EventLog 6005 candidate, not a direct boot assertion".to_string()),
                provenance: AnalysisProvenanceDto {
                    data_source_id: "ds-1".to_string(),
                    artifact_path: "Windows/System32/winevt/Logs/System.evtx".to_string(),
                    parser: "evtx.boot_shutdown".to_string(),
                    parsed_at: "2026-01-01T00:00:00+00:00".to_string(),
                    status: AnalysisParseStatusDto::Parsed,
                    warnings: Vec::new(),
                },
            }],
            timezone: None,
            language: None,
            status: AnalysisParseStatusDto::NotParsed,
            warnings: vec!["parser unavailable".to_string()],
            provenance: vec![AnalysisProvenanceDto {
                data_source_id: "ds-1".to_string(),
                artifact_path: "Windows/System32/config/SYSTEM".to_string(),
                parser: "registry.system".to_string(),
                parsed_at: "2026-01-01T00:00:00+00:00".to_string(),
                status: AnalysisParseStatusDto::NotParsed,
                warnings: vec!["value traversal unavailable".to_string()],
            }],
            field_provenance: vec![AnalysisFieldProvenanceDto {
                field: "computerName".to_string(),
                value_name: "ComputerName".to_string(),
                key_path: "ControlSet001\\Control\\ComputerName\\ComputerName".to_string(),
                hive_path: "Windows/System32/config/SYSTEM".to_string(),
                parser: "registry.system".to_string(),
            }],
        };

        let json = serde_json::to_value(dto).unwrap();
        assert_eq!(json["computerName"], "host");
        assert_eq!(
            json["networkAdapters"][0]["macAddress"],
            "00:11:22:33:44:55"
        );
        assert_eq!(json["bootHistory"][0]["bootType"], "eventLogStarted");
        assert_eq!(json["bootHistory"][0]["eventId"], 6005);
        assert_eq!(json["bootHistory"][0]["recordId"], 42);
        assert_eq!(
            json["bootHistory"][0]["note"],
            "EventLog 6005 candidate, not a direct boot assertion"
        );
        assert_eq!(json["status"], "notParsed");
        assert_eq!(json["provenance"][0]["dataSourceId"], "ds-1");
        assert_eq!(
            json["provenance"][0]["artifactPath"],
            "Windows/System32/config/SYSTEM"
        );
        assert_eq!(
            json["provenance"][0]["parsedAt"],
            "2026-01-01T00:00:00+00:00"
        );
        assert_eq!(json["fieldProvenance"][0]["field"], "computerName");
        assert_eq!(json["fieldProvenance"][0]["valueName"], "ComputerName");
        assert!(json.get("computer_name").is_none());
    }

    #[test]
    fn provenance_serializes_required_camel_case_fields() {
        let dto = AnalysisProvenanceDto {
            data_source_id: "ds".to_string(),
            artifact_path: "Windows/System32/winevt/Logs/System.evtx".to_string(),
            parser: "evtx.boot_shutdown".to_string(),
            parsed_at: "2026-01-01T00:00:00+00:00".to_string(),
            status: AnalysisParseStatusDto::Unavailable,
            warnings: vec!["EVTX parser is unavailable".to_string()],
        };

        let json = serde_json::to_value(dto).unwrap();
        assert_eq!(json["dataSourceId"], "ds");
        assert_eq!(
            json["artifactPath"],
            "Windows/System32/winevt/Logs/System.evtx"
        );
        assert_eq!(json["parser"], "evtx.boot_shutdown");
        assert_eq!(json["parsedAt"], "2026-01-01T00:00:00+00:00");
        assert_eq!(json["status"], "unavailable");
        assert_eq!(json["warnings"][0], "EVTX parser is unavailable");
        assert!(json.get("data_source_id").is_none());
    }

    #[test]
    fn current_provenance_contract_is_bounded_to_source_attribution() {
        let dto = EvidenceCategoryDto {
            category: "ProgramExecution".to_string(),
            display_name: "Program execution".to_string(),
            status: AnalysisParseStatusDto::Parsed,
            file_count: 1,
            total_size: 98_304,
            artifact_count: 2,
            confidence: 0.95,
            sources: vec![EvidenceSourceDto {
                file_id: "file-prefetch".to_string(),
                path: "Windows/Prefetch/CMD.EXE-12345678.pf".to_string(),
                size: 98_304,
                evidence_kind: "execution_artifact".to_string(),
                parser: "prefetch".to_string(),
                status: AnalysisParseStatusDto::Parsed,
                artifact_count: 2,
                warnings: Vec::new(),
            }],
            warnings: Vec::new(),
            provenance: vec![AnalysisProvenanceDto {
                data_source_id: "ds-001".to_string(),
                artifact_path: "Windows/Prefetch/CMD.EXE-12345678.pf".to_string(),
                parser: "prefetch".to_string(),
                parsed_at: "2026-01-01T00:00:00+00:00".to_string(),
                status: AnalysisParseStatusDto::Parsed,
                warnings: Vec::new(),
            }],
        };

        let json = serde_json::to_value(dto).unwrap();
        assert!((json["confidence"].as_f64().unwrap() - 0.95).abs() < 0.000_001);
        assert_eq!(json["sources"][0]["fileId"], "file-prefetch");
        assert_eq!(json["sources"][0]["evidenceKind"], "execution_artifact");
        assert_eq!(json["sources"][0]["parser"], "prefetch");
        assert_eq!(json["provenance"][0]["dataSourceId"], "ds-001");
        assert_eq!(
            json["provenance"][0]["artifactPath"],
            "Windows/Prefetch/CMD.EXE-12345678.pf"
        );
        assert_eq!(json["provenance"][0]["parser"], "prefetch");
        assert!(json["sources"][0].get("file_id").is_none());
        assert!(json["provenance"][0].get("data_source_id").is_none());
        assert!(json["provenance"][0].get("sourceHash").is_none());
        assert!(json["provenance"][0].get("parserVersion").is_none());
    }

    #[test]
    #[ignore = "future provenance contract: add after DataSource/Artifact/Timeline schema migrations"]
    fn future_provenance_contract_includes_hash_version_and_confidence() {
        let dto = AnalysisProvenanceDto {
            data_source_id: "ds-001".to_string(),
            artifact_path: "Windows/System32/config/SYSTEM".to_string(),
            parser: "registry.system".to_string(),
            parsed_at: "2026-01-01T00:00:00+00:00".to_string(),
            status: AnalysisParseStatusDto::Parsed,
            warnings: Vec::new(),
        };

        let json = serde_json::to_value(dto).unwrap();
        assert_eq!(json["dataSourceId"], "ds-001");
        assert_eq!(json["artifactPath"], "Windows/System32/config/SYSTEM");
        assert_eq!(json["parser"], "registry.system");
        assert!(json.get("sourceHash").is_some());
        assert!(json.get("parserVersion").is_some());
        assert!(json.get("confidence").is_some());
        assert!(json.get("sourceAttribution").is_some());
        assert!(json.get("source_hash").is_none());
        assert!(json.get("parser_version").is_none());
        assert!(json.get("source_attribution").is_none());
    }

    #[test]
    fn classification_serializes_camel_case() {
        let dto = AnalysisFileClassificationDto {
            category: "Documents".to_string(),
            files: vec![AnalysisClassifiedFileDto {
                file_id: "file-1".to_string(),
                path: "doc.pdf".to_string(),
                name: "doc.pdf".to_string(),
                size: 4,
                file_type: "PDF".to_string(),
                magic_description: "PDF Document".to_string(),
                provenance: AnalysisProvenanceDto {
                    data_source_id: "ds-1".to_string(),
                    artifact_path: "doc.pdf".to_string(),
                    parser: "analysis.magic".to_string(),
                    parsed_at: "2026-01-01T00:00:00+00:00".to_string(),
                    status: AnalysisParseStatusDto::Parsed,
                    warnings: Vec::new(),
                },
            }],
            file_count: 1,
            total_size: 4,
            status: AnalysisParseStatusDto::Parsed,
            warnings: Vec::new(),
            provenance: vec![AnalysisProvenanceDto {
                data_source_id: "ds-1".to_string(),
                artifact_path: "doc.pdf".to_string(),
                parser: "analysis.magic".to_string(),
                parsed_at: "2026-01-01T00:00:00+00:00".to_string(),
                status: AnalysisParseStatusDto::Parsed,
                warnings: Vec::new(),
            }],
        };

        let json = serde_json::to_value(dto).unwrap();
        assert_eq!(json["files"][0]["fileId"], "file-1");
        assert_eq!(json["fileCount"], 1);
        assert_eq!(json["totalSize"], 4);
        assert_eq!(json["files"][0]["fileType"], "PDF");
        assert_eq!(json["files"][0]["magicDescription"], "PDF Document");
        assert_eq!(json["files"][0]["provenance"]["dataSourceId"], "ds-1");
        assert_eq!(json["provenance"][0]["artifactPath"], "doc.pdf");
    }

    #[test]
    fn governance_snapshot_serializes_camel_case() {
        let dto = V2GovernanceSnapshotDto {
            generated_at: "2026-06-12T00:00:00Z".to_string(),
            fact_sources: vec![
                GovernanceFactSourceDto {
                    area: "verification".to_string(),
                    fact_file: "testdata/governance/v2-verification-catalog.json".to_string(),
                    fact_kind: "catalog".to_string(),
                    derived_outputs: vec![
                        "verificationChains".to_string(),
                        "supportMatrixEntries".to_string(),
                        "supportMatrix".to_string(),
                    ],
                    last_verified_at: "2026-06-12T00:00:00Z".to_string(),
                },
                GovernanceFactSourceDto {
                    area: "releasePolicy".to_string(),
                    fact_file: "testdata/governance/v2-release-policy.json".to_string(),
                    fact_kind: "policy".to_string(),
                    derived_outputs: vec![
                        "releaseGates".to_string(),
                        "releaseScorecard".to_string(),
                    ],
                    last_verified_at: "2026-06-13T00:00:00Z".to_string(),
                },
                GovernanceFactSourceDto {
                    area: "knownLimitations".to_string(),
                    fact_file: "testdata/governance/v2-known-limitations.json".to_string(),
                    fact_kind: "catalog".to_string(),
                    derived_outputs: vec![
                        "knownLimitations".to_string(),
                        "supportMatrix.documentedLimitCount".to_string(),
                    ],
                    last_verified_at: "2026-06-13T00:00:00Z".to_string(),
                },
            ],
            runtime_results: GovernanceRuntimeResultsDto {
                checked_at: "2026-06-13T00:00:00Z".to_string(),
                checks: vec![GovernanceRuntimeCheckDto {
                    check_id: "docs-drift".to_string(),
                    title: "文档防漂移".to_string(),
                    status: ReleaseGateStatusDto::Passed,
                    evidence: "scripts/check-doc-drift.ps1".to_string(),
                    detail: "README / AGENTS / documentation-index / Mermaid 图块数量一致"
                        .to_string(),
                    checked_at: "2026-06-13T00:00:00Z".to_string(),
                    sub_checks: vec![GovernanceRuntimeSubcheckDto {
                        check_id: "readme-fact-sync".to_string(),
                        title: "README 事实同步".to_string(),
                        status: ReleaseGateStatusDto::Passed,
                        evidence: "crate/page/command counts match".to_string(),
                        detail: "README 关键事实与仓库扫描结果一致".to_string(),
                    }],
                }],
            },
            verification_chains: vec![VerificationChainStatusDto {
                chain: "NTFS".to_string(),
                display_name: "NTFS 文件系统".to_string(),
                maturity: SupportMaturityDto::Ga,
                guarantee_level: VerificationGuaranteeLevelDto::Guaranteed,
                fixture_tier: "public-small".to_string(),
                expected_json_version: "v1".to_string(),
                verified_sample_count: 3,
                result: VerificationResultDto::Passed,
                notes: vec!["validated".to_string()],
            }],
            support_matrix: ParserSupportMatrixSummaryDto {
                ga_count: 6,
                beta_count: 2,
                experimental_count: 1,
                unsupported_count: 4,
                documented_limit_count: 1,
            },
            support_matrix_entries: vec![ParserSupportMatrixEntryDto {
                chain: "NTFS".to_string(),
                platform: "Windows".to_string(),
                maturity: SupportMaturityDto::Ga,
                verified_samples: vec![
                    "tiny.raw".to_string(),
                    "synthetic ntfs fixture".to_string(),
                ],
                baseline: "fixture assertions / expected.json".to_string(),
                guarantee_summary: "deleted/hidden/system/orphan 为 guaranteed".to_string(),
                notes: vec!["复杂损坏样本仍不足".to_string()],
            }],
            known_limitations: vec![KnownLimitationDto {
                category: "E01".to_string(),
                item: "多段复杂样本".to_string(),
                status: KnownLimitationStatusDto::Partial,
                summary: "当前公开样本主要覆盖 tiny 单段".to_string(),
                affected_chains: vec!["E01".to_string()],
                source_doc: "docs/known-unsupported-formats.md".to_string(),
            }],
            benchmark: BenchmarkSummaryDto {
                host_profile: "Windows 11 / 32GB RAM / NVMe".to_string(),
                baseline_version: "2026.06".to_string(),
                last_verified_at: "2026-06-12T00:00:00Z".to_string(),
                scenarios: vec![BenchmarkSnapshotDto {
                    dataset_level: "medium".to_string(),
                    scenario: "search warm query".to_string(),
                    p95_ms: 1500,
                    memory_peak_mb: Some(2048),
                    baseline_version: "2026.06".to_string(),
                }],
                required_checks: vec![BenchmarkRequiredCheckDto {
                    dataset_level: "medium".to_string(),
                    scenario: "search warm query".to_string(),
                    threshold_p95_ms: 1500,
                    measured_p95_ms: Some(1500),
                    status: BenchmarkRequirementStatusDto::Covered,
                }],
                covered_required_count: 1,
                missing_required_count: 0,
                exceeded_required_count: 0,
            },
            security: SecurityAuditSummaryDto {
                export_overwrite_default: false,
                export_path_guard_enabled: true,
                stdio_command_whitelist_enforced: true,
                sse_https_only: true,
                embedded_credentials_blocked: true,
                media_handle_scoped: true,
                error_redaction_enabled: true,
                audit_log_required: true,
                audit_event_count: 6,
                sensitive_audit_event_count: 4,
                recent_audit_entries: vec![SecurityAuditEntryDto {
                    action: "mcp.tool.call".to_string(),
                    resource_type: "mcp".to_string(),
                    resource_id: Some("triage-server".to_string()),
                    created_at: "2026-06-12T00:10:00Z".to_string(),
                    summary: Some("status=ok; toolName=query_fixture_catalog".to_string()),
                    sensitive: true,
                }],
                notes: vec!["audit".to_string()],
            },
            error_taxonomy_entries: vec![ErrorTaxonomyEntryDto {
                category: "security".to_string(),
                severity: "high".to_string(),
                recoverable: false,
                examples: vec!["MCP policy block".to_string()],
                redaction_rule: "never expose credentials or raw absolute paths".to_string(),
                notes: vec!["frontend only receives sanitized messages".to_string()],
            }],
            release_gates: vec![ReleaseGateEntryDto {
                gate_id: "docs-drift".to_string(),
                title: "文档防漂移".to_string(),
                status: ReleaseGateStatusDto::Passed,
                evidence: "scripts/check-doc-drift.ps1".to_string(),
                detail: "README / AGENTS / 文档索引与 Mermaid 图块数量一致".to_string(),
            }],
            release_scorecard: ReleaseScorecardDto {
                total_score: 84,
                grade: "B".to_string(),
                verification_score: 26,
                correlation_score: 18,
                performance_score: 16,
                security_score: 24,
                breakdown: vec![ReleaseScoreBreakdownEntryDto {
                    dimension: "verification".to_string(),
                    max_score: 30,
                    actual_score: 26,
                    deductions: vec!["pending hash data source".to_string()],
                }],
                blockers: vec!["private regression pending".to_string()],
                residual_risks: vec!["browser fixture medium only".to_string()],
            },
            runtime_signals: GovernanceRuntimeSignalsDto {
                data_source_count: 2,
                hashed_data_source_count: 1,
                pending_hash_data_source_count: 1,
                warning_data_source_count: 1,
                running_job_count: 0,
                partial_job_count: 1,
                failed_job_count: 0,
                report_count: 2,
                correlation_snapshot_available: true,
                correlation_lead_count: 4,
                correlation_high_confidence_lead_count: 3,
                correlation_review_lead_count: 2,
                correlation_cluster_count: 3,
                correlation_rule_family_count: 7,
                correlation_covered_family_count: 3,
                correlation_high_confidence_family_count: 2,
                correlation_family_coverage: vec![
                    CorrelationFamilyCoverageDto {
                        family: "LNK".to_string(),
                        display_name: "LNK".to_string(),
                        status: CorrelationCoverageStatusDto::Covered,
                        lead_count: 1,
                        high_confidence_lead_count: 1,
                        review_lead_count: 0,
                        cluster_count: 1,
                        sample_signals: vec!["LNK 目标路径命中文件路径".to_string()],
                    },
                    CorrelationFamilyCoverageDto {
                        family: "Registry".to_string(),
                        display_name: "Registry".to_string(),
                        status: CorrelationCoverageStatusDto::Review,
                        lead_count: 1,
                        high_confidence_lead_count: 0,
                        review_lead_count: 1,
                        cluster_count: 1,
                        sample_signals: vec!["Registry 值数据命中文件路径".to_string()],
                    },
                ],
            },
        };

        let json = serde_json::to_value(dto).unwrap();
        assert_eq!(json["generatedAt"], "2026-06-12T00:00:00Z");
        assert_eq!(json["factSources"][0]["area"], "verification");
        assert_eq!(json["factSources"][1]["factKind"], "policy");
        assert_eq!(
            json["factSources"][2]["factFile"],
            "testdata/governance/v2-known-limitations.json"
        );
        assert_eq!(json["runtimeResults"]["checkedAt"], "2026-06-13T00:00:00Z");
        assert_eq!(json["runtimeResults"]["checks"][0]["checkId"], "docs-drift");
        assert_eq!(
            json["runtimeResults"]["checks"][0]["subChecks"][0]["checkId"],
            "readme-fact-sync"
        );
        assert_eq!(
            json["verificationChains"][0]["displayName"],
            "NTFS 文件系统"
        );
        assert_eq!(
            json["verificationChains"][0]["guaranteeLevel"],
            "guaranteed"
        );
        assert_eq!(json["supportMatrix"]["gaCount"], 6);
        assert_eq!(json["supportMatrixEntries"][0]["chain"], "NTFS");
        assert_eq!(
            json["supportMatrixEntries"][0]["verifiedSamples"][0],
            "tiny.raw"
        );
        assert_eq!(json["knownLimitations"][0]["category"], "E01");
        assert_eq!(json["knownLimitations"][0]["status"], "partial");
        assert_eq!(json["knownLimitations"][0]["affectedChains"][0], "E01");
        assert_eq!(
            json["benchmark"]["hostProfile"],
            "Windows 11 / 32GB RAM / NVMe"
        );
        assert_eq!(
            json["benchmark"]["requiredChecks"][0]["thresholdP95Ms"],
            1500
        );
        assert_eq!(
            json["benchmark"]["requiredChecks"][0]["measuredP95Ms"],
            1500
        );
        assert_eq!(json["benchmark"]["requiredChecks"][0]["status"], "covered");
        assert_eq!(json["benchmark"]["coveredRequiredCount"], 1);
        assert_eq!(json["benchmark"]["missingRequiredCount"], 0);
        assert_eq!(json["benchmark"]["exceededRequiredCount"], 0);
        assert_eq!(json["security"]["exportOverwriteDefault"], false);
        assert_eq!(json["security"]["auditEventCount"], 6);
        assert_eq!(json["security"]["sensitiveAuditEventCount"], 4);
        assert_eq!(
            json["security"]["recentAuditEntries"][0]["action"],
            "mcp.tool.call"
        );
        assert_eq!(
            json["security"]["recentAuditEntries"][0]["resourceType"],
            "mcp"
        );
        assert_eq!(json["errorTaxonomyEntries"][0]["category"], "security");
        assert_eq!(json["releaseGates"][0]["gateId"], "docs-drift");
        assert_eq!(json["releaseScorecard"]["totalScore"], 84);
        assert_eq!(
            json["releaseScorecard"]["breakdown"][0]["dimension"],
            "verification"
        );
        assert_eq!(json["runtimeSignals"]["dataSourceCount"], 2);
        assert_eq!(json["runtimeSignals"]["correlationSnapshotAvailable"], true);
        assert_eq!(json["runtimeSignals"]["correlationLeadCount"], 4);
        assert_eq!(
            json["runtimeSignals"]["correlationHighConfidenceLeadCount"],
            3
        );
        assert_eq!(json["runtimeSignals"]["correlationReviewLeadCount"], 2);
        assert_eq!(json["runtimeSignals"]["correlationClusterCount"], 3);
        assert_eq!(json["runtimeSignals"]["correlationRuleFamilyCount"], 7);
        assert_eq!(json["runtimeSignals"]["correlationCoveredFamilyCount"], 3);
        assert_eq!(
            json["runtimeSignals"]["correlationHighConfidenceFamilyCount"],
            2
        );
        assert_eq!(
            json["runtimeSignals"]["correlationFamilyCoverage"][0]["family"],
            "LNK"
        );
        assert_eq!(
            json["runtimeSignals"]["correlationFamilyCoverage"][0]["status"],
            "covered"
        );
        assert_eq!(
            json["runtimeSignals"]["correlationFamilyCoverage"][0]["sampleSignals"][0],
            "LNK 目标路径命中文件路径"
        );
        assert!(json.get("generated_at").is_none());
        assert!(json["supportMatrixEntries"][0]
            .get("verified_samples")
            .is_none());
        assert!(json["errorTaxonomyEntries"][0]
            .get("redaction_rule")
            .is_none());
        assert!(json["releaseGates"][0].get("gate_id").is_none());
    }
}
