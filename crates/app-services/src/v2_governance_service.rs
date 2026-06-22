use chrono::Utc;
use domain::DataSourceHashStatus;
use once_cell::sync::Lazy;
use persistence_sqlite::repositories::audit_repo::AuditRepo;
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use rusqlite::Connection;
use serde::Deserialize;
use transport::dto::{
    BenchmarkRequiredCheckDto, BenchmarkRequirementStatusDto, BenchmarkSnapshotDto,
    BenchmarkSummaryDto, CorrelationCoverageStatusDto, CorrelationFamilyCoverageDto,
    ErrorTaxonomyEntryDto, GovernanceFactSourceDto, GovernanceRuntimeCheckDto,
    GovernanceRuntimeResultsDto, GovernanceRuntimeSignalsDto, GovernanceRuntimeSubcheckDto,
    KnownLimitationDto, ParserSupportMatrixEntryDto, ParserSupportMatrixSummaryDto,
    ReleaseGateEntryDto, ReleaseGateStatusDto, ReleaseScoreBreakdownEntryDto, ReleaseScorecardDto,
    SecurityAuditEntryDto, SecurityAuditSummaryDto, SupportMaturityDto, V2GovernanceSnapshotDto,
    VerificationChainStatusDto, VerificationGuaranteeLevelDto, VerificationResultDto,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GovernanceChainCatalogFile {
    chains: Vec<GovernanceChainCatalogEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BenchmarkBaselineFile {
    host_profile: String,
    baseline_version: String,
    last_verified_at: String,
    scenarios: Vec<BenchmarkSnapshotDto>,
    required_checks: Vec<BenchmarkRequirementPolicyFile>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BenchmarkRequirementPolicyFile {
    dataset_level: String,
    scenario: String,
    threshold_p95_ms: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SecurityTaxonomyFile {
    security_defaults: SecurityDefaultsFile,
    error_taxonomy_entries: Vec<ErrorTaxonomyEntryDto>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SecurityDefaultsFile {
    export_overwrite_default: bool,
    export_path_guard_enabled: bool,
    stdio_command_whitelist_enforced: bool,
    sse_https_only: bool,
    embedded_credentials_blocked: bool,
    media_handle_scoped: bool,
    error_redaction_enabled: bool,
    notes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleasePolicyFile {
    core_fixture_chains: Vec<String>,
    baseline_residual_risks: Vec<String>,
    score_policy: ReleaseScorePolicyFile,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KnownLimitationsFile {
    last_verified_at: String,
    documented_limit_count: u32,
    items: Vec<KnownLimitationDto>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseScorePolicyFile {
    verification: ScoreDimensionPolicyFile,
    correlation: ScoreDimensionPolicyFile,
    performance: ScoreDimensionPolicyFile,
    security: ScoreDimensionPolicyFile,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScoreDimensionPolicyFile {
    max_score: u32,
    deductions: Vec<ScoreDeductionRuleFile>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScoreDeductionRuleFile {
    trigger: String,
    amount: u32,
    message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GovernanceRuntimeResultsFile {
    checked_at: String,
    doc_drift: RuntimeGateFactFile,
    core_fixture_regression: RuntimeGateFactFile,
    benchmark_thresholds: RuntimeGateFactFile,
    security_baseline: RuntimeGateFactFile,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeGateFactFile {
    status: ReleaseGateStatusDto,
    evidence: String,
    detail: String,
    sub_checks: Vec<RuntimeGateSubcheckFile>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeGateSubcheckFile {
    check_id: String,
    title: String,
    status: ReleaseGateStatusDto,
    evidence: String,
    detail: String,
}

#[derive(Debug, Clone)]
struct ScoreContribution {
    amount: u32,
    message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GovernanceChainCatalogEntry {
    chain: String,
    platform: String,
    display_name: String,
    maturity: SupportMaturityDto,
    guarantee_level: VerificationGuaranteeLevelDto,
    fixture_tier: String,
    expected_json_version: String,
    verified_sample_count: u32,
    result: VerificationResultDto,
    notes: Vec<String>,
    verified_samples: Vec<String>,
    baseline: String,
    guarantee_summary: String,
    matrix_notes: Vec<String>,
}

static GOVERNANCE_CHAIN_CATALOG: Lazy<Vec<GovernanceChainCatalogEntry>> = Lazy::new(|| {
    let raw = include_str!("../../../testdata/governance/v2-verification-catalog.json");
    serde_json::from_str::<GovernanceChainCatalogFile>(raw)
        .expect("parse V2 governance verification catalog")
        .chains
});

static BENCHMARK_BASELINE: Lazy<BenchmarkBaselineFile> = Lazy::new(|| {
    let raw = include_str!("../../../testdata/governance/v2-benchmark-baseline.json");
    serde_json::from_str::<BenchmarkBaselineFile>(raw)
        .expect("parse V2 governance benchmark baseline")
});

static SECURITY_TAXONOMY: Lazy<SecurityTaxonomyFile> = Lazy::new(|| {
    let raw = include_str!("../../../testdata/governance/v2-security-taxonomy.json");
    serde_json::from_str::<SecurityTaxonomyFile>(raw)
        .expect("parse V2 governance security taxonomy")
});

static RELEASE_POLICY: Lazy<ReleasePolicyFile> = Lazy::new(|| {
    let raw = include_str!("../../../testdata/governance/v2-release-policy.json");
    serde_json::from_str::<ReleasePolicyFile>(raw).expect("parse V2 governance release policy")
});

static KNOWN_LIMITATIONS: Lazy<KnownLimitationsFile> = Lazy::new(|| {
    let raw = include_str!("../../../testdata/governance/v2-known-limitations.json");
    let file = serde_json::from_str::<KnownLimitationsFile>(raw)
        .expect("parse V2 governance known limitations");
    assert_eq!(
        file.documented_limit_count as usize,
        file.items.len(),
        "known limitations documentedLimitCount must match items length"
    );
    file
});

static GOVERNANCE_RUNTIME_RESULTS: Lazy<GovernanceRuntimeResultsFile> = Lazy::new(|| {
    let raw = include_str!("../../../testdata/governance/v2-runtime-results.json");
    serde_json::from_str::<GovernanceRuntimeResultsFile>(raw)
        .expect("parse V2 governance runtime results")
});

#[derive(Debug, Default, Clone)]
struct CorrelationRuntimeSnapshot {
    snapshot_available: bool,
    lead_count: u32,
    high_confidence_lead_count: u32,
    review_lead_count: u32,
    cluster_count: u32,
    rule_family_count: u32,
    covered_family_count: u32,
    high_confidence_family_count: u32,
    family_coverage: Vec<CorrelationFamilyCoverageDto>,
}

pub fn get_v2_governance_snapshot(
    conn: &Connection,
    case_id: &str,
) -> Result<V2GovernanceSnapshotDto, String> {
    let runtime_signals = build_runtime_signals(conn, case_id)?;
    let audit_snapshot = security_audit_snapshot(conn, case_id)?;
    let chain_catalog = governance_chain_catalog();
    let verification_chains = verification_chains(chain_catalog);
    let support_matrix_entries = support_matrix_entries(chain_catalog);
    let support_matrix = support_matrix_summary(&support_matrix_entries);
    let benchmark_file = benchmark_baseline();
    let security_file = security_taxonomy();
    let release_policy = release_policy();
    let benchmark_scenarios = benchmark_file.scenarios.clone();
    let benchmark_required_checks =
        benchmark_required_checks(&benchmark_file.required_checks, &benchmark_scenarios);
    let covered_required_count = benchmark_required_checks
        .iter()
        .filter(|item| item.status == BenchmarkRequirementStatusDto::Covered)
        .count() as u32;
    let missing_required_count = benchmark_required_checks
        .iter()
        .filter(|item| item.status == BenchmarkRequirementStatusDto::Missing)
        .count() as u32;
    let exceeded_required_count = benchmark_required_checks
        .iter()
        .filter(|item| item.status == BenchmarkRequirementStatusDto::Exceeded)
        .count() as u32;
    let benchmark = BenchmarkSummaryDto {
        host_profile: benchmark_file.host_profile.clone(),
        baseline_version: benchmark_file.baseline_version.clone(),
        last_verified_at: benchmark_file.last_verified_at.clone(),
        scenarios: benchmark_scenarios,
        required_checks: benchmark_required_checks,
        covered_required_count,
        missing_required_count,
        exceeded_required_count,
    };
    let security = SecurityAuditSummaryDto {
        export_overwrite_default: security_file.security_defaults.export_overwrite_default,
        export_path_guard_enabled: security_file.security_defaults.export_path_guard_enabled,
        stdio_command_whitelist_enforced: security_file
            .security_defaults
            .stdio_command_whitelist_enforced,
        sse_https_only: security_file.security_defaults.sse_https_only,
        embedded_credentials_blocked: security_file.security_defaults.embedded_credentials_blocked,
        media_handle_scoped: security_file.security_defaults.media_handle_scoped,
        error_redaction_enabled: security_file.security_defaults.error_redaction_enabled,
        audit_log_required: audit_snapshot.audit_log_required,
        audit_event_count: audit_snapshot.audit_event_count,
        sensitive_audit_event_count: audit_snapshot.sensitive_audit_event_count,
        recent_audit_entries: audit_snapshot.recent_audit_entries,
        notes: security_file.security_defaults.notes.clone(),
    };
    let release_gates = release_gates(
        &verification_chains,
        &support_matrix,
        &benchmark,
        &security,
        &runtime_signals,
        release_policy,
    );

    Ok(V2GovernanceSnapshotDto {
        generated_at: Utc::now().to_rfc3339(),
        fact_sources: governance_fact_sources(),
        runtime_results: governance_runtime_results_dto(),
        verification_chains,
        support_matrix,
        support_matrix_entries,
        known_limitations: known_limitations(),
        benchmark,
        security,
        error_taxonomy_entries: security_file.error_taxonomy_entries.clone(),
        release_gates: release_gates.clone(),
        release_scorecard: release_scorecard(&release_gates, &runtime_signals),
        runtime_signals,
    })
}

fn build_runtime_signals(
    conn: &Connection,
    case_id: &str,
) -> Result<GovernanceRuntimeSignalsDto, String> {
    let data_sources = DataSourceRepo::new(conn)
        .find_by_case(&domain::CaseId(case_id.to_string()))
        .map_err(|e| e.to_string())?;
    let jobs = crate::job_service::get_jobs_from_db(conn)?;
    let reports = crate::report::get_report_history(conn, case_id);

    let hashed_data_source_count = data_sources
        .iter()
        .filter(|source| matches!(source.provenance.hash_status, DataSourceHashStatus::Hashed))
        .count() as u32;
    let pending_hash_data_source_count = data_sources
        .iter()
        .filter(|source| {
            matches!(
                source.provenance.hash_status,
                DataSourceHashStatus::Pending | DataSourceHashStatus::Unknown
            )
        })
        .count() as u32;
    let warning_data_source_count = data_sources
        .iter()
        .filter(|source| !source.provenance.warnings.is_empty())
        .count() as u32;
    let running_job_count = jobs.iter().filter(|job| job.status == "running").count() as u32;
    let partial_job_count = jobs.iter().filter(|job| job.partial).count() as u32;
    let failed_job_count = jobs.iter().filter(|job| job.status == "failed").count() as u32;
    let correlation = correlation_runtime_snapshot(conn)?;

    Ok(GovernanceRuntimeSignalsDto {
        data_source_count: data_sources.len() as u32,
        hashed_data_source_count,
        pending_hash_data_source_count,
        warning_data_source_count,
        running_job_count,
        partial_job_count,
        failed_job_count,
        report_count: reports.len() as u32,
        correlation_snapshot_available: correlation.snapshot_available,
        correlation_lead_count: correlation.lead_count,
        correlation_high_confidence_lead_count: correlation.high_confidence_lead_count,
        correlation_review_lead_count: correlation.review_lead_count,
        correlation_cluster_count: correlation.cluster_count,
        correlation_rule_family_count: correlation.rule_family_count,
        correlation_covered_family_count: correlation.covered_family_count,
        correlation_high_confidence_family_count: correlation.high_confidence_family_count,
        correlation_family_coverage: correlation.family_coverage,
    })
}

fn correlation_runtime_snapshot(conn: &Connection) -> Result<CorrelationRuntimeSnapshot, String> {
    let snapshot = crate::correlation::get_correlation_snapshot(conn)?;
    let high_confidence_lead_count = snapshot
        .leads
        .iter()
        .filter(|lead| {
            matches!(
                lead.confidence,
                transport::dto::CorrelationConfidenceDto::Direct
                    | transport::dto::CorrelationConfidenceDto::Strong
            )
        })
        .count() as u32;
    let review_lead_count = snapshot
        .leads
        .iter()
        .filter(|lead| {
            !lead.caveats.is_empty()
                || matches!(
                    lead.confidence,
                    transport::dto::CorrelationConfidenceDto::Weak
                        | transport::dto::CorrelationConfidenceDto::Heuristic
                )
                || lead.provenance.iter().any(|item| {
                    matches!(
                        item.guarantee_level,
                        VerificationGuaranteeLevelDto::Experimental
                            | VerificationGuaranteeLevelDto::NotGuaranteed
                    )
                })
        })
        .count() as u32;
    let family_coverage = snapshot.family_coverage.clone();
    let covered_family_count = family_coverage
        .iter()
        .filter(|item| item.status == CorrelationCoverageStatusDto::Covered)
        .count() as u32;
    let high_confidence_family_count = family_coverage
        .iter()
        .filter(|item| item.high_confidence_lead_count > 0)
        .count() as u32;

    Ok(CorrelationRuntimeSnapshot {
        snapshot_available: true,
        lead_count: snapshot.lead_count,
        high_confidence_lead_count,
        review_lead_count,
        cluster_count: snapshot.cluster_count,
        rule_family_count: family_coverage.len() as u32,
        covered_family_count,
        high_confidence_family_count,
        family_coverage,
    })
}

struct SecurityAuditSnapshot {
    audit_log_required: bool,
    audit_event_count: u32,
    sensitive_audit_event_count: u32,
    recent_audit_entries: Vec<SecurityAuditEntryDto>,
}

fn security_audit_snapshot(
    conn: &Connection,
    case_id: &str,
) -> Result<SecurityAuditSnapshot, String> {
    let repo = AuditRepo::new(conn);
    let entries = repo
        .query(Some(case_id), None, 5, 0)
        .map_err(|err| err.to_string())?;
    let audit_event_count = repo.count(Some(case_id)).map_err(|err| err.to_string())? as u32;

    let recent_audit_entries = entries
        .iter()
        .map(|entry| SecurityAuditEntryDto {
            action: entry.action.clone(),
            resource_type: entry.resource_type.clone(),
            resource_id: entry.resource_id.clone(),
            created_at: entry.created_at.clone(),
            summary: audit_summary(entry),
            sensitive: is_sensitive_audit_entry(&entry.action),
        })
        .collect::<Vec<_>>();
    let sensitive_audit_event_count = entries
        .iter()
        .filter(|entry| is_sensitive_audit_entry(&entry.action))
        .count() as u32;

    Ok(SecurityAuditSnapshot {
        audit_log_required: true,
        audit_event_count,
        sensitive_audit_event_count,
        recent_audit_entries,
    })
}

fn is_sensitive_audit_entry(action: &str) -> bool {
    matches!(
        action,
        "file.extract"
            | "mcp.connect"
            | "mcp.disconnect"
            | "mcp.test"
            | "mcp.resource.list"
            | "mcp.resource.read"
            | "mcp.tool.list"
            | "mcp.tool.call"
            | "mcp.prompt.list"
            | "mcp.prompt.get"
            | "report.export"
    )
}

fn audit_summary(
    entry: &persistence_sqlite::repositories::audit_repo::AuditLogEntry,
) -> Option<String> {
    let details: serde_json::Value = serde_json::from_str(&entry.details).ok()?;
    if let Some(status) = details.get("status").and_then(|value| value.as_str()) {
        if let Some(tool_name) = details.get("toolName").and_then(|value| value.as_str()) {
            return Some(format!("status={status}; toolName={tool_name}"));
        }
        if let Some(file_name) = details
            .get("destinationFileName")
            .and_then(|value| value.as_str())
        {
            return Some(format!("status={status}; destinationFileName={file_name}"));
        }
        return Some(format!("status={status}"));
    }
    if let Some(prompt_name) = details.get("promptName").and_then(|value| value.as_str()) {
        return Some(format!("promptName={prompt_name}"));
    }
    if let Some(server_id) = details.get("serverId").and_then(|value| value.as_str()) {
        return Some(format!("serverId={server_id}"));
    }
    None
}

fn governance_chain_catalog() -> &'static [GovernanceChainCatalogEntry] {
    &GOVERNANCE_CHAIN_CATALOG
}

fn benchmark_baseline() -> &'static BenchmarkBaselineFile {
    &BENCHMARK_BASELINE
}

fn security_taxonomy() -> &'static SecurityTaxonomyFile {
    &SECURITY_TAXONOMY
}

fn release_policy() -> &'static ReleasePolicyFile {
    &RELEASE_POLICY
}

fn known_limitations_file() -> &'static KnownLimitationsFile {
    &KNOWN_LIMITATIONS
}

fn governance_runtime_results() -> &'static GovernanceRuntimeResultsFile {
    &GOVERNANCE_RUNTIME_RESULTS
}

fn governance_fact_sources() -> Vec<GovernanceFactSourceDto> {
    let runtime_results = governance_runtime_results();
    vec![
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
            area: "benchmark".to_string(),
            fact_file: "testdata/governance/v2-benchmark-baseline.json".to_string(),
            fact_kind: "baseline".to_string(),
            derived_outputs: vec![
                "benchmark.scenarios".to_string(),
                "benchmark.requiredChecks".to_string(),
                "benchmark.coveredRequiredCount".to_string(),
            ],
            last_verified_at: BENCHMARK_BASELINE.last_verified_at.clone(),
        },
        GovernanceFactSourceDto {
            area: "security".to_string(),
            fact_file: "testdata/governance/v2-security-taxonomy.json".to_string(),
            fact_kind: "taxonomy".to_string(),
            derived_outputs: vec!["security".to_string(), "errorTaxonomyEntries".to_string()],
            last_verified_at: "2026-06-13T00:00:00Z".to_string(),
        },
        GovernanceFactSourceDto {
            area: "releasePolicy".to_string(),
            fact_file: "testdata/governance/v2-release-policy.json".to_string(),
            fact_kind: "policy".to_string(),
            derived_outputs: vec![
                "releaseGates".to_string(),
                "releaseScorecard".to_string(),
                "runtimeSignals.correlationFamilyCoverage".to_string(),
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
            last_verified_at: known_limitations_file().last_verified_at.clone(),
        },
        GovernanceFactSourceDto {
            area: "runtimeResults".to_string(),
            fact_file: "testdata/governance/v2-runtime-results.json".to_string(),
            fact_kind: "runResult".to_string(),
            derived_outputs: vec![
                "releaseGates.docsDrift".to_string(),
                "releaseGates.coreFixtureRegression".to_string(),
                "releaseGates.benchmarkThresholds".to_string(),
                "releaseGates.securityBaseline".to_string(),
            ],
            last_verified_at: runtime_results.checked_at.clone(),
        },
    ]
}

fn governance_runtime_results_dto() -> GovernanceRuntimeResultsDto {
    let runtime_results = governance_runtime_results();
    GovernanceRuntimeResultsDto {
        checked_at: runtime_results.checked_at.clone(),
        checks: vec![
            GovernanceRuntimeCheckDto {
                check_id: "docs-drift".to_string(),
                title: "文档防漂移".to_string(),
                status: runtime_results.doc_drift.status.clone(),
                evidence: runtime_results.doc_drift.evidence.clone(),
                detail: runtime_results.doc_drift.detail.clone(),
                checked_at: runtime_results.checked_at.clone(),
                sub_checks: runtime_results
                    .doc_drift
                    .sub_checks
                    .iter()
                    .map(runtime_subcheck_dto)
                    .collect(),
            },
            GovernanceRuntimeCheckDto {
                check_id: "core-fixture-regression".to_string(),
                title: "核心 fixture 回归".to_string(),
                status: runtime_results.core_fixture_regression.status.clone(),
                evidence: runtime_results.core_fixture_regression.evidence.clone(),
                detail: runtime_results.core_fixture_regression.detail.clone(),
                checked_at: runtime_results.checked_at.clone(),
                sub_checks: runtime_results
                    .core_fixture_regression
                    .sub_checks
                    .iter()
                    .map(runtime_subcheck_dto)
                    .collect(),
            },
            GovernanceRuntimeCheckDto {
                check_id: "benchmark-thresholds".to_string(),
                title: "Benchmark 阈值".to_string(),
                status: runtime_results.benchmark_thresholds.status.clone(),
                evidence: runtime_results.benchmark_thresholds.evidence.clone(),
                detail: runtime_results.benchmark_thresholds.detail.clone(),
                checked_at: runtime_results.checked_at.clone(),
                sub_checks: runtime_results
                    .benchmark_thresholds
                    .sub_checks
                    .iter()
                    .map(runtime_subcheck_dto)
                    .collect(),
            },
            GovernanceRuntimeCheckDto {
                check_id: "security-baseline".to_string(),
                title: "安全基线".to_string(),
                status: runtime_results.security_baseline.status.clone(),
                evidence: runtime_results.security_baseline.evidence.clone(),
                detail: runtime_results.security_baseline.detail.clone(),
                checked_at: runtime_results.checked_at.clone(),
                sub_checks: runtime_results
                    .security_baseline
                    .sub_checks
                    .iter()
                    .map(runtime_subcheck_dto)
                    .collect(),
            },
        ],
    }
}

fn runtime_subcheck_dto(item: &RuntimeGateSubcheckFile) -> GovernanceRuntimeSubcheckDto {
    GovernanceRuntimeSubcheckDto {
        check_id: item.check_id.clone(),
        title: item.title.clone(),
        status: item.status.clone(),
        evidence: item.evidence.clone(),
        detail: item.detail.clone(),
    }
}

fn verification_chains(catalog: &[GovernanceChainCatalogEntry]) -> Vec<VerificationChainStatusDto> {
    catalog
        .iter()
        .map(|item| VerificationChainStatusDto {
            chain: item.chain.clone(),
            display_name: item.display_name.clone(),
            maturity: item.maturity.clone(),
            guarantee_level: item.guarantee_level.clone(),
            fixture_tier: item.fixture_tier.clone(),
            expected_json_version: item.expected_json_version.clone(),
            verified_sample_count: item.verified_sample_count,
            result: item.result.clone(),
            notes: item.notes.clone(),
        })
        .collect()
}

fn support_matrix_entries(
    catalog: &[GovernanceChainCatalogEntry],
) -> Vec<ParserSupportMatrixEntryDto> {
    catalog
        .iter()
        .map(|item| ParserSupportMatrixEntryDto {
            chain: item.chain.clone(),
            platform: item.platform.clone(),
            maturity: item.maturity.clone(),
            verified_samples: item.verified_samples.clone(),
            baseline: item.baseline.clone(),
            guarantee_summary: item.guarantee_summary.clone(),
            notes: item.matrix_notes.clone(),
        })
        .collect()
}

fn support_matrix_summary(
    entries: &[ParserSupportMatrixEntryDto],
) -> ParserSupportMatrixSummaryDto {
    let mut ga_count = 0;
    let mut beta_count = 0;
    let mut experimental_count = 0;
    let mut unsupported_count = 0;

    for entry in entries {
        match entry.maturity {
            SupportMaturityDto::Ga => ga_count += 1,
            SupportMaturityDto::Beta => beta_count += 1,
            SupportMaturityDto::Experimental => experimental_count += 1,
            SupportMaturityDto::Unsupported => unsupported_count += 1,
        }
    }

    ParserSupportMatrixSummaryDto {
        ga_count,
        beta_count,
        experimental_count,
        unsupported_count,
        documented_limit_count: known_limitations_file().documented_limit_count,
    }
}

fn known_limitations() -> Vec<KnownLimitationDto> {
    known_limitations_file().items.clone()
}

fn release_gates(
    verification_chains: &[VerificationChainStatusDto],
    support_matrix: &ParserSupportMatrixSummaryDto,
    benchmark: &BenchmarkSummaryDto,
    security: &SecurityAuditSummaryDto,
    runtime_signals: &GovernanceRuntimeSignalsDto,
    release_policy: &ReleasePolicyFile,
) -> Vec<ReleaseGateEntryDto> {
    let runtime_results = governance_runtime_results();
    let (core_fixture_status, core_fixture_evidence, core_fixture_detail) =
        core_fixture_gate(verification_chains, support_matrix, release_policy);
    let (benchmark_status, benchmark_evidence, benchmark_detail) = benchmark_gate(benchmark);
    let (security_status, security_evidence, security_detail) = security_gate(security);
    let (core_fixture_status, core_fixture_evidence, core_fixture_detail) = merge_runtime_gate_fact(
        core_fixture_status,
        core_fixture_evidence,
        core_fixture_detail,
        &runtime_results.core_fixture_regression,
        &runtime_results.checked_at,
    );
    let (benchmark_status, benchmark_evidence, benchmark_detail) = merge_runtime_gate_fact(
        benchmark_status,
        benchmark_evidence,
        benchmark_detail,
        &runtime_results.benchmark_thresholds,
        &runtime_results.checked_at,
    );
    let (security_status, security_evidence, security_detail) = merge_runtime_gate_fact(
        security_status,
        security_evidence,
        security_detail,
        &runtime_results.security_baseline,
        &runtime_results.checked_at,
    );
    let pending_hash = runtime_signals.pending_hash_data_source_count > 0;
    let failed_jobs = runtime_signals.failed_job_count > 0;
    let running_jobs = runtime_signals.running_job_count > 0;
    let partial_jobs = runtime_signals.partial_job_count > 0;
    let warning_sources = runtime_signals.warning_data_source_count > 0;

    vec![
        ReleaseGateEntryDto {
            gate_id: "core-fixture-regression".to_string(),
            title: "核心 fixture 回归".to_string(),
            status: core_fixture_status,
            evidence: core_fixture_evidence,
            detail: core_fixture_detail,
        },
        ReleaseGateEntryDto {
            gate_id: "docs-drift".to_string(),
            title: "文档防漂移".to_string(),
            status: runtime_results.doc_drift.status.clone(),
            evidence: format!(
                "checkedAt={}; {}",
                runtime_results.checked_at, runtime_results.doc_drift.evidence
            ),
            detail: runtime_results.doc_drift.detail.clone(),
        },
        ReleaseGateEntryDto {
            gate_id: "benchmark-thresholds".to_string(),
            title: "Benchmark 阈值".to_string(),
            status: benchmark_status,
            evidence: benchmark_evidence,
            detail: benchmark_detail,
        },
        ReleaseGateEntryDto {
            gate_id: "security-baseline".to_string(),
            title: "安全基线".to_string(),
            status: security_status,
            evidence: security_evidence,
            detail: security_detail,
        },
        ReleaseGateEntryDto {
            gate_id: "evidence-hash-completeness".to_string(),
            title: "证据哈希完整性".to_string(),
            status: if pending_hash {
                ReleaseGateStatusDto::Warning
            } else {
                ReleaseGateStatusDto::Passed
            },
            evidence: format!(
                "hashed={}, pending={}",
                runtime_signals.hashed_data_source_count,
                runtime_signals.pending_hash_data_source_count
            ),
            detail: if pending_hash {
                "仍有证据源哈希未完成，发布前必须说明可信边界".to_string()
            } else {
                "当前案件证据源哈希已完成或无待处理项".to_string()
            },
        },
        ReleaseGateEntryDto {
            gate_id: "runtime-failures".to_string(),
            title: "运行时失败任务".to_string(),
            status: if failed_jobs {
                ReleaseGateStatusDto::Blocked
            } else if running_jobs || partial_jobs || warning_sources {
                ReleaseGateStatusDto::Warning
            } else {
                ReleaseGateStatusDto::Passed
            },
            evidence: format!(
                "runningJobCount={}, partialJobCount={}, failedJobCount={}, warningDataSourceCount={}",
                runtime_signals.running_job_count,
                runtime_signals.partial_job_count,
                runtime_signals.failed_job_count,
                runtime_signals.warning_data_source_count
            ),
            detail: if failed_jobs {
                "存在 failed job，候选发布必须先清理真实链路失败".to_string()
            } else if running_jobs {
                "当前仍有运行中的任务，发布候选前需等待稳定收敛".to_string()
            } else if partial_jobs {
                "当前存在 partial job，需要 investigator 复核部分成功的可信边界".to_string()
            } else if warning_sources {
                "无 failed job，但存在 provenance warning，需要 investigator 明示".to_string()
            } else {
                "当前运行信号未发现阻断级失败".to_string()
            },
        },
        ReleaseGateEntryDto {
            gate_id: "correlation-family-coverage".to_string(),
            title: "关联规则家族覆盖".to_string(),
            status: correlation_family_gate_status(runtime_signals),
            evidence: format!(
                "snapshotAvailable={}, leadCount={}, coveredFamilies={}, ruleFamilies={}, highConfidenceFamilies={}",
                runtime_signals.correlation_snapshot_available,
                runtime_signals.correlation_lead_count,
                runtime_signals.correlation_covered_family_count,
                runtime_signals.correlation_rule_family_count,
                runtime_signals.correlation_high_confidence_family_count
            ),
            detail: correlation_family_gate_detail(runtime_signals),
        },
    ]
}

fn merge_runtime_gate_fact(
    derived_status: ReleaseGateStatusDto,
    derived_evidence: String,
    derived_detail: String,
    runtime_fact: &RuntimeGateFactFile,
    checked_at: &str,
) -> (ReleaseGateStatusDto, String, String) {
    let merged_status = worse_gate_status(derived_status, runtime_fact.status.clone());
    let merged_evidence = format!(
        "checkedAt={}; runtime={}; snapshot={}",
        checked_at, runtime_fact.evidence, derived_evidence
    );
    let merged_detail = if runtime_fact.detail == derived_detail {
        runtime_fact.detail.clone()
    } else {
        format!("{}；{}", runtime_fact.detail, derived_detail)
    };
    (merged_status, merged_evidence, merged_detail)
}

fn worse_gate_status(
    left: ReleaseGateStatusDto,
    right: ReleaseGateStatusDto,
) -> ReleaseGateStatusDto {
    if gate_status_rank(&left) >= gate_status_rank(&right) {
        left
    } else {
        right
    }
}

fn gate_status_rank(status: &ReleaseGateStatusDto) -> u8 {
    match status {
        ReleaseGateStatusDto::Passed => 0,
        ReleaseGateStatusDto::Warning => 1,
        ReleaseGateStatusDto::Blocked => 2,
    }
}

fn correlation_family_gate_status(
    runtime_signals: &GovernanceRuntimeSignalsDto,
) -> ReleaseGateStatusDto {
    if !runtime_signals.correlation_snapshot_available
        || runtime_signals.correlation_lead_count == 0
    {
        return ReleaseGateStatusDto::Blocked;
    }
    if runtime_signals.correlation_rule_family_count == 0 {
        return ReleaseGateStatusDto::Blocked;
    }
    if runtime_signals
        .correlation_covered_family_count
        .saturating_mul(2)
        < runtime_signals.correlation_rule_family_count
    {
        return ReleaseGateStatusDto::Warning;
    }
    if runtime_signals.correlation_high_confidence_family_count == 0 {
        return ReleaseGateStatusDto::Warning;
    }
    ReleaseGateStatusDto::Passed
}

fn correlation_family_gate_detail(runtime_signals: &GovernanceRuntimeSignalsDto) -> String {
    if !runtime_signals.correlation_snapshot_available {
        return "当前尚未生成关联分析快照，规则家族覆盖不可验证".to_string();
    }
    if runtime_signals.correlation_lead_count == 0 {
        return "当前关联分析快照没有 lead，规则家族覆盖尚未形成可用调查线索".to_string();
    }
    if runtime_signals.correlation_rule_family_count == 0 {
        return "当前未统计任何关联规则家族，发布前需补齐覆盖口径".to_string();
    }
    if runtime_signals
        .correlation_covered_family_count
        .saturating_mul(2)
        < runtime_signals.correlation_rule_family_count
    {
        return format!(
            "当前仅 {} / {} 个规则家族形成 covered 状态，Browser / Email / Registry 等链路仍需继续补齐",
            runtime_signals.correlation_covered_family_count,
            runtime_signals.correlation_rule_family_count
        );
    }
    if runtime_signals.correlation_high_confidence_family_count == 0 {
        return "当前规则家族虽有命中，但没有高置信家族，调查工作流仍需加强".to_string();
    }
    format!(
        "当前已形成 {} / {} 个 covered 家族，其中 {} 个具备高置信 lead",
        runtime_signals.correlation_covered_family_count,
        runtime_signals.correlation_rule_family_count,
        runtime_signals.correlation_high_confidence_family_count
    )
}

fn release_scorecard(
    gates: &[ReleaseGateEntryDto],
    runtime_signals: &GovernanceRuntimeSignalsDto,
) -> ReleaseScorecardDto {
    let release_policy = release_policy();
    let verification_contributions = verification_contributions(gates, runtime_signals);
    let correlation_contributions = correlation_contributions(gates, runtime_signals);
    let performance_contributions = performance_contributions(gates, runtime_signals);
    let security_contributions = security_contributions(gates, runtime_signals);
    let verification_score = apply_score_policy(
        &release_policy.score_policy.verification,
        &verification_contributions,
    );
    let correlation_score = apply_score_policy(
        &release_policy.score_policy.correlation,
        &correlation_contributions,
    );
    let performance_score = apply_score_policy(
        &release_policy.score_policy.performance,
        &performance_contributions,
    );
    let security_score = apply_score_policy(
        &release_policy.score_policy.security,
        &security_contributions,
    );
    let total_score = verification_score + correlation_score + performance_score + security_score;
    let grade = if total_score >= 90 {
        "A"
    } else if total_score >= 80 {
        "B"
    } else if total_score >= 70 {
        "C"
    } else {
        "D"
    };

    let mut blockers = Vec::new();
    for gate in gates
        .iter()
        .filter(|gate| gate.status == ReleaseGateStatusDto::Blocked)
    {
        blockers.push(format!("{}：{}", gate.title, gate.detail));
    }
    if runtime_signals.pending_hash_data_source_count > 0 {
        blockers.push("仍有证据源哈希未完成，发布前需说明可信边界。".to_string());
    }

    let mut residual_risks = release_policy.baseline_residual_risks.clone();
    if gate_status(gates, "benchmark-thresholds") == Some(ReleaseGateStatusDto::Warning) {
        residual_risks.push("benchmark 仍未覆盖全部 medium/large 阈值场景。".to_string());
    }
    if runtime_signals.warning_data_source_count > 0 {
        residual_risks
            .push("当前案件含 provenance warning，需在 investigator 视图显式提示。".to_string());
    }
    if !runtime_signals.correlation_snapshot_available {
        residual_risks.push("当前未生成关联分析快照，调查工作流仍缺少 lead 视图。".to_string());
    } else if runtime_signals.correlation_lead_count == 0 {
        residual_risks.push("当前关联分析快照没有 lead，需复核规则覆盖或样本充分性。".to_string());
    } else if runtime_signals.correlation_covered_family_count < 4 {
        residual_risks.push(
            "当前高质量关联规则家族覆盖仍不足，需继续补齐 Browser / Email / Registry 等主链路。"
                .to_string(),
        );
    }

    ReleaseScorecardDto {
        total_score,
        grade: grade.to_string(),
        verification_score,
        correlation_score,
        performance_score,
        security_score,
        breakdown: release_score_breakdown(
            gates,
            runtime_signals,
            verification_score,
            correlation_score,
            performance_score,
            security_score,
        ),
        blockers,
        residual_risks,
    }
}

fn release_score_breakdown(
    gates: &[ReleaseGateEntryDto],
    runtime_signals: &GovernanceRuntimeSignalsDto,
    verification_score: u32,
    correlation_score: u32,
    performance_score: u32,
    security_score: u32,
) -> Vec<ReleaseScoreBreakdownEntryDto> {
    let verification_contributions = verification_contributions(gates, runtime_signals);
    let correlation_contributions = correlation_contributions(gates, runtime_signals);
    let performance_contributions = performance_contributions(gates, runtime_signals);
    let security_contributions = security_contributions(gates, runtime_signals);

    vec![
        ReleaseScoreBreakdownEntryDto {
            dimension: "verification".to_string(),
            max_score: release_policy().score_policy.verification.max_score,
            actual_score: verification_score,
            deductions: verification_contributions
                .iter()
                .map(|item| item.message.clone())
                .collect(),
        },
        ReleaseScoreBreakdownEntryDto {
            dimension: "correlation".to_string(),
            max_score: release_policy().score_policy.correlation.max_score,
            actual_score: correlation_score,
            deductions: correlation_contributions
                .iter()
                .map(|item| item.message.clone())
                .collect(),
        },
        ReleaseScoreBreakdownEntryDto {
            dimension: "performance".to_string(),
            max_score: release_policy().score_policy.performance.max_score,
            actual_score: performance_score,
            deductions: performance_contributions
                .iter()
                .map(|item| item.message.clone())
                .collect(),
        },
        ReleaseScoreBreakdownEntryDto {
            dimension: "security".to_string(),
            max_score: release_policy().score_policy.security.max_score,
            actual_score: security_score,
            deductions: security_contributions
                .iter()
                .map(|item| item.message.clone())
                .collect(),
        },
    ]
}

fn verification_contributions(
    gates: &[ReleaseGateEntryDto],
    runtime_signals: &GovernanceRuntimeSignalsDto,
) -> Vec<ScoreContribution> {
    let blocked_count = gates
        .iter()
        .filter(|gate| gate.status == ReleaseGateStatusDto::Blocked)
        .count() as u32;
    let warning_count = gates
        .iter()
        .filter(|gate| gate.status == ReleaseGateStatusDto::Warning)
        .count() as u32;

    score_contributions_for(
        &release_policy().score_policy.verification,
        gates,
        runtime_signals,
        blocked_count,
        warning_count,
    )
}

fn correlation_contributions(
    gates: &[ReleaseGateEntryDto],
    runtime_signals: &GovernanceRuntimeSignalsDto,
) -> Vec<ScoreContribution> {
    let blocked_count = gates
        .iter()
        .filter(|gate| gate.status == ReleaseGateStatusDto::Blocked)
        .count() as u32;
    let warning_count = gates
        .iter()
        .filter(|gate| gate.status == ReleaseGateStatusDto::Warning)
        .count() as u32;

    score_contributions_for(
        &release_policy().score_policy.correlation,
        gates,
        runtime_signals,
        blocked_count,
        warning_count,
    )
}

fn performance_contributions(
    gates: &[ReleaseGateEntryDto],
    runtime_signals: &GovernanceRuntimeSignalsDto,
) -> Vec<ScoreContribution> {
    let blocked_count = gates
        .iter()
        .filter(|gate| gate.status == ReleaseGateStatusDto::Blocked)
        .count() as u32;
    let warning_count = gates
        .iter()
        .filter(|gate| gate.status == ReleaseGateStatusDto::Warning)
        .count() as u32;

    score_contributions_for(
        &release_policy().score_policy.performance,
        gates,
        runtime_signals,
        blocked_count,
        warning_count,
    )
}

fn security_contributions(
    gates: &[ReleaseGateEntryDto],
    runtime_signals: &GovernanceRuntimeSignalsDto,
) -> Vec<ScoreContribution> {
    let blocked_count = gates
        .iter()
        .filter(|gate| gate.status == ReleaseGateStatusDto::Blocked)
        .count() as u32;
    let warning_count = gates
        .iter()
        .filter(|gate| gate.status == ReleaseGateStatusDto::Warning)
        .count() as u32;

    score_contributions_for(
        &release_policy().score_policy.security,
        gates,
        runtime_signals,
        blocked_count,
        warning_count,
    )
}

fn apply_score_policy(
    policy: &ScoreDimensionPolicyFile,
    contributions: &[ScoreContribution],
) -> u32 {
    let total_deduction = contributions.iter().map(|item| item.amount).sum::<u32>();
    policy.max_score.saturating_sub(total_deduction)
}

fn score_contributions_for(
    policy: &ScoreDimensionPolicyFile,
    gates: &[ReleaseGateEntryDto],
    runtime_signals: &GovernanceRuntimeSignalsDto,
    blocked_count: u32,
    warning_count: u32,
) -> Vec<ScoreContribution> {
    policy
        .deductions
        .iter()
        .filter(|rule| {
            score_trigger_matches(
                rule.trigger.as_str(),
                gates,
                runtime_signals,
                blocked_count,
                warning_count,
            )
        })
        .map(|rule| ScoreContribution {
            amount: rule.amount,
            message: rule.message.clone(),
        })
        .collect()
}

fn score_trigger_matches(
    trigger: &str,
    gates: &[ReleaseGateEntryDto],
    runtime_signals: &GovernanceRuntimeSignalsDto,
    blocked_count: u32,
    warning_count: u32,
) -> bool {
    match trigger {
        "gate:core-fixture-regression=blocked" => {
            gate_status(gates, "core-fixture-regression") == Some(ReleaseGateStatusDto::Blocked)
        }
        "gate:core-fixture-regression=warning" => {
            gate_status(gates, "core-fixture-regression") == Some(ReleaseGateStatusDto::Warning)
        }
        "gate:evidence-hash-completeness=blocked" => {
            gate_status(gates, "evidence-hash-completeness") == Some(ReleaseGateStatusDto::Blocked)
        }
        "gate:evidence-hash-completeness=warning" => {
            gate_status(gates, "evidence-hash-completeness") == Some(ReleaseGateStatusDto::Warning)
        }
        "gate:docs-drift=blocked" => {
            gate_status(gates, "docs-drift") == Some(ReleaseGateStatusDto::Blocked)
        }
        "gate:correlation-family-coverage=blocked" => {
            gate_status(gates, "correlation-family-coverage") == Some(ReleaseGateStatusDto::Blocked)
        }
        "gate:correlation-family-coverage=warning" => {
            gate_status(gates, "correlation-family-coverage") == Some(ReleaseGateStatusDto::Warning)
        }
        "gate:runtime-failures=blocked" => {
            gate_status(gates, "runtime-failures") == Some(ReleaseGateStatusDto::Blocked)
        }
        "gate:runtime-failures=warning" => {
            gate_status(gates, "runtime-failures") == Some(ReleaseGateStatusDto::Warning)
        }
        "gate:benchmark-thresholds=blocked" => {
            gate_status(gates, "benchmark-thresholds") == Some(ReleaseGateStatusDto::Blocked)
        }
        "gate:benchmark-thresholds=warning" => {
            gate_status(gates, "benchmark-thresholds") == Some(ReleaseGateStatusDto::Warning)
        }
        "gate:security-baseline=blocked" => {
            gate_status(gates, "security-baseline") == Some(ReleaseGateStatusDto::Blocked)
        }
        "gate:security-baseline=warning" => {
            gate_status(gates, "security-baseline") == Some(ReleaseGateStatusDto::Warning)
        }
        "runtime:pending_hash_gt_1" => runtime_signals.pending_hash_data_source_count > 1,
        "runtime:warning_data_sources_gt_0" => runtime_signals.warning_data_source_count > 0,
        "runtime:correlation_snapshot_missing" => !runtime_signals.correlation_snapshot_available,
        "runtime:correlation_leads_eq_0" => {
            runtime_signals.correlation_snapshot_available
                && runtime_signals.correlation_lead_count == 0
        }
        "runtime:correlation_high_confidence_eq_0" => {
            runtime_signals.correlation_snapshot_available
                && runtime_signals.correlation_lead_count > 0
                && runtime_signals.correlation_high_confidence_lead_count == 0
        }
        "runtime:correlation_covered_lt_half" => {
            runtime_signals.correlation_rule_family_count > 0
                && runtime_signals
                    .correlation_covered_family_count
                    .saturating_mul(2)
                    < runtime_signals.correlation_rule_family_count
        }
        "runtime:partial_jobs_gt_0" => runtime_signals.partial_job_count > 0,
        "runtime:failed_jobs_gt_0" => runtime_signals.failed_job_count > 0,
        "meta:blocked_count_gt_0" => blocked_count > 0,
        "meta:blocked_count_gt_1" => blocked_count > 1,
        "meta:warning_count_gt_1" => warning_count > 1,
        "meta:warning_count_gt_3" => warning_count > 3,
        "meta:blocked_and_warning" => blocked_count > 0 && warning_count > 0,
        _ => false,
    }
}

fn core_fixture_gate(
    verification_chains: &[VerificationChainStatusDto],
    support_matrix: &ParserSupportMatrixSummaryDto,
    release_policy: &ReleasePolicyFile,
) -> (ReleaseGateStatusDto, String, String) {
    let relevant: Vec<&VerificationChainStatusDto> = verification_chains
        .iter()
        .filter(|chain| {
            release_policy
                .core_fixture_chains
                .iter()
                .any(|item| item.eq_ignore_ascii_case(&chain.chain))
        })
        .collect();
    let failed: Vec<&str> = relevant
        .iter()
        .filter(|chain| chain.result == VerificationResultDto::Failed)
        .map(|chain| chain.display_name.as_str())
        .collect();
    let not_full: Vec<&str> = relevant
        .iter()
        .filter(|chain| {
            matches!(
                chain.result,
                VerificationResultDto::Partial | VerificationResultDto::Pending
            )
        })
        .map(|chain| chain.display_name.as_str())
        .collect();
    let missing_count = release_policy
        .core_fixture_chains
        .len()
        .saturating_sub(relevant.len());

    let status = if !failed.is_empty() {
        ReleaseGateStatusDto::Blocked
    } else if !not_full.is_empty() || missing_count > 0 {
        ReleaseGateStatusDto::Warning
    } else {
        ReleaseGateStatusDto::Passed
    };

    let evidence = format!(
        "coreChains={}, passed={}, partialOrPending={}, failed={}, maturity[ga={}, beta={}, experimental={}, unsupported={}]",
        relevant.len(),
        relevant.len().saturating_sub(not_full.len() + failed.len()),
        not_full.len(),
        failed.len(),
        support_matrix.ga_count,
        support_matrix.beta_count,
        support_matrix.experimental_count,
        support_matrix.unsupported_count
    );

    let detail = if !failed.is_empty() {
        format!("核心链路存在失败项：{}", failed.join("、"))
    } else if !not_full.is_empty() || missing_count > 0 {
        let mut parts = Vec::new();
        if !not_full.is_empty() {
            parts.push(format!("仍有未全量通过链路：{}", not_full.join("、")));
        }
        if missing_count > 0 {
            parts.push(format!("缺少 {} 条核心链路快照", missing_count));
        }
        parts.join("；")
    } else {
        "E01/RAW/NTFS/Prefetch/LNK/Registry/RecycleBin 核心链路均已通过当前快照校验".to_string()
    };

    (status, evidence, detail)
}

fn benchmark_gate(benchmark: &BenchmarkSummaryDto) -> (ReleaseGateStatusDto, String, String) {
    let missing = benchmark
        .required_checks
        .iter()
        .filter(|item| item.status == BenchmarkRequirementStatusDto::Missing)
        .map(|item| format!("{} {}", item.dataset_level, item.scenario))
        .collect::<Vec<_>>();
    let exceeded = benchmark
        .required_checks
        .iter()
        .filter(|item| item.status == BenchmarkRequirementStatusDto::Exceeded)
        .map(|item| {
            format!(
                "{} {}={}ms>{}ms",
                item.dataset_level,
                item.scenario,
                item.measured_p95_ms.unwrap_or_default(),
                item.threshold_p95_ms
            )
        })
        .collect::<Vec<_>>();

    let status = if !exceeded.is_empty() {
        ReleaseGateStatusDto::Blocked
    } else if benchmark.scenarios.is_empty() || !missing.is_empty() {
        ReleaseGateStatusDto::Warning
    } else {
        ReleaseGateStatusDto::Passed
    };

    let evidence = format!(
        "baselineVersion={}, measuredScenarios={}, missingRequired={}, exceededRequired={}",
        benchmark.baseline_version,
        benchmark.scenarios.len(),
        benchmark.missing_required_count,
        benchmark.exceeded_required_count
    );

    let detail = if !exceeded.is_empty() {
        format!("存在超阈值场景：{}", exceeded.join("；"))
    } else if benchmark.scenarios.is_empty() {
        "尚未采集 benchmark 场景，候选发布前需补齐基线".to_string()
    } else if !missing.is_empty() {
        format!("仍缺少 benchmark 必需场景：{}", missing.join("、"))
    } else {
        "当前 benchmark 已覆盖必需场景且未发现超阈值项".to_string()
    };

    (status, evidence, detail)
}

fn benchmark_required_checks(
    requirements: &[BenchmarkRequirementPolicyFile],
    scenarios: &[BenchmarkSnapshotDto],
) -> Vec<BenchmarkRequiredCheckDto> {
    requirements
        .iter()
        .map(|requirement| {
            let matched = scenarios.iter().find(|item| {
                item.dataset_level
                    .eq_ignore_ascii_case(&requirement.dataset_level)
                    && item.scenario == requirement.scenario
            });
            let measured_p95_ms = matched.map(|item| item.p95_ms);
            let status = match matched {
                Some(item) if item.p95_ms > requirement.threshold_p95_ms => {
                    BenchmarkRequirementStatusDto::Exceeded
                }
                Some(_) => BenchmarkRequirementStatusDto::Covered,
                None => BenchmarkRequirementStatusDto::Missing,
            };

            BenchmarkRequiredCheckDto {
                dataset_level: requirement.dataset_level.clone(),
                scenario: requirement.scenario.clone(),
                threshold_p95_ms: requirement.threshold_p95_ms,
                measured_p95_ms,
                status,
            }
        })
        .collect()
}

fn security_gate(security: &SecurityAuditSummaryDto) -> (ReleaseGateStatusDto, String, String) {
    let mut failed_controls = Vec::new();

    if security.export_overwrite_default {
        failed_controls.push("exportOverwriteDefault");
    }
    if !security.export_path_guard_enabled {
        failed_controls.push("exportPathGuardEnabled");
    }
    if !security.stdio_command_whitelist_enforced {
        failed_controls.push("stdioCommandWhitelistEnforced");
    }
    if !security.sse_https_only {
        failed_controls.push("sseHttpsOnly");
    }
    if !security.embedded_credentials_blocked {
        failed_controls.push("embeddedCredentialsBlocked");
    }
    if !security.media_handle_scoped {
        failed_controls.push("mediaHandleScoped");
    }
    if !security.error_redaction_enabled {
        failed_controls.push("errorRedactionEnabled");
    }
    if !security.audit_log_required || security.audit_event_count == 0 {
        failed_controls.push("auditLogRequired");
    }

    let status = if failed_controls.is_empty() {
        ReleaseGateStatusDto::Passed
    } else {
        ReleaseGateStatusDto::Blocked
    };
    let evidence = format!(
        "pathGuard={}, stdioWhitelist={}, sseHttpsOnly={}, embeddedCredentialsBlocked={}, mediaHandleScoped={}, errorRedactionEnabled={}, auditLogRequired={}, overwriteDefault={}",
        security.export_path_guard_enabled,
        security.stdio_command_whitelist_enforced,
        security.sse_https_only,
        security.embedded_credentials_blocked,
        security.media_handle_scoped,
        security.error_redaction_enabled,
        security.audit_log_required,
        security.export_overwrite_default
    );
    let detail = if failed_controls.is_empty() {
        "导出路径、防覆盖、MCP、媒体句柄与错误脱敏基线均已开启".to_string()
    } else {
        format!("安全基线缺失控件：{}", failed_controls.join("、"))
    };

    (status, evidence, detail)
}

fn gate_status(gates: &[ReleaseGateEntryDto], gate_id: &str) -> Option<ReleaseGateStatusDto> {
    gates
        .iter()
        .find(|gate| gate.gate_id == gate_id)
        .map(|gate| gate.status.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use domain::{
        CaseId, DataSource, DataSourceKind, DataSourceProvenance, DataSourceProvenanceStatus,
    };
    use persistence_sqlite::repositories::{
        audit_repo::{AuditAction, AuditRepo},
        datasource_repo::DataSourceRepo,
        job_repo::JobRepo,
        report_repo::{ReportRecord, ReportRepo},
    };

    #[test]
    fn governance_snapshot_aggregates_runtime_signals() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        persistence_sqlite::runner::run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO cases (id, name, number, examiner) VALUES ('case-1', 'Case', '001', 'qa')",
            [],
        )
        .unwrap();

        DataSourceRepo::new(&conn)
            .insert(
                &CaseId("case-1".to_string()),
                &DataSource {
                    id: domain::DataSourceId("ds-1".to_string()),
                    name: "source-1".to_string(),
                    kind: DataSourceKind::Raw,
                    source_path: std::path::PathBuf::from("C:/evidence/source-1.raw"),
                    imported_at: Utc::now(),
                    provenance: DataSourceProvenance {
                        source_hash_sha256: Some("abc".to_string()),
                        hash_status: DataSourceHashStatus::Hashed,
                        canonical_source_path: None,
                        evidence_size: Some(1024),
                        reader_kind: Some("raw".to_string()),
                        provenance_status: DataSourceProvenanceStatus::Recorded,
                        warnings: Vec::new(),
                    },
                },
            )
            .unwrap();
        DataSourceRepo::new(&conn)
            .insert(
                &CaseId("case-1".to_string()),
                &DataSource {
                    id: domain::DataSourceId("ds-2".to_string()),
                    name: "source-2".to_string(),
                    kind: DataSourceKind::E01,
                    source_path: std::path::PathBuf::from("C:/evidence/source-2.E01"),
                    imported_at: Utc::now(),
                    provenance: DataSourceProvenance {
                        source_hash_sha256: None,
                        hash_status: DataSourceHashStatus::Pending,
                        canonical_source_path: None,
                        evidence_size: Some(2048),
                        reader_kind: Some("e01".to_string()),
                        provenance_status: DataSourceProvenanceStatus::Recorded,
                        warnings: vec!["pending".to_string()],
                    },
                },
            )
            .unwrap();

        let job_id = JobRepo::new(&conn).create("case-1", "Import").unwrap();
        JobRepo::new(&conn)
            .update_outcome_counts(&job_id, 1, 0, 0, true)
            .unwrap();
        JobRepo::new(&conn).complete(&job_id, "partial").unwrap();
        ReportRepo::new(&conn)
            .insert(&ReportRecord {
                id: "report-1".to_string(),
                case_id: "case-1".to_string(),
                template_id: "summary".to_string(),
                file_name: "report.json".to_string(),
                created_by: "qa".to_string(),
                status: "completed".to_string(),
                progress: Some(100),
                created_at: Utc::now().to_rfc3339(),
            })
            .unwrap();
        AuditRepo::new(&conn)
            .log(
                Some("case-1"),
                "system",
                &AuditAction::McpToolCall,
                Some("fixture-catalog"),
                r#"{"status":"ok","toolName":"query_fixture_catalog"}"#,
            )
            .unwrap();
        AuditRepo::new(&conn)
            .log(
                Some("case-1"),
                "system",
                &AuditAction::FileExtract,
                Some("file-cmd-exe"),
                r#"{"status":"ok","destinationFileName":"cmd.exe"}"#,
            )
            .unwrap();

        let snapshot = get_v2_governance_snapshot(&conn, "case-1").unwrap();

        assert_eq!(snapshot.runtime_signals.data_source_count, 2);
        assert_eq!(snapshot.runtime_signals.hashed_data_source_count, 1);
        assert_eq!(snapshot.runtime_signals.pending_hash_data_source_count, 1);
        assert_eq!(snapshot.runtime_signals.warning_data_source_count, 1);
        assert_eq!(snapshot.runtime_signals.partial_job_count, 1);
        assert_eq!(snapshot.runtime_signals.report_count, 1);
        assert!(snapshot.runtime_signals.correlation_snapshot_available);
        assert_eq!(snapshot.runtime_signals.correlation_lead_count, 0);
        assert_eq!(
            snapshot
                .runtime_signals
                .correlation_high_confidence_lead_count,
            0
        );
        assert_eq!(snapshot.runtime_signals.correlation_review_lead_count, 0);
        assert_eq!(snapshot.runtime_signals.correlation_cluster_count, 0);
        assert_eq!(snapshot.runtime_signals.correlation_rule_family_count, 8);
        assert_eq!(snapshot.runtime_signals.correlation_covered_family_count, 0);
        assert_eq!(
            snapshot
                .runtime_signals
                .correlation_high_confidence_family_count,
            0
        );
        assert_eq!(
            snapshot.runtime_signals.correlation_family_coverage.len(),
            8
        );
        assert!(snapshot
            .runtime_signals
            .correlation_family_coverage
            .iter()
            .all(|item| item.status == CorrelationCoverageStatusDto::Missing));
        assert_eq!(snapshot.benchmark.required_checks.len(), 18);
        assert_eq!(snapshot.benchmark.covered_required_count, 18);
        assert_eq!(snapshot.benchmark.missing_required_count, 0);
        assert_eq!(snapshot.benchmark.exceeded_required_count, 0);
        assert_eq!(
            snapshot.benchmark.required_checks[0].status,
            BenchmarkRequirementStatusDto::Covered
        );
        assert_eq!(
            snapshot.benchmark.required_checks[2].status,
            BenchmarkRequirementStatusDto::Covered
        );
        assert_eq!(snapshot.security.audit_event_count, 2);
        assert_eq!(snapshot.security.sensitive_audit_event_count, 2);
        assert_eq!(snapshot.security.recent_audit_entries.len(), 2);
        let audit_actions = snapshot
            .security
            .recent_audit_entries
            .iter()
            .map(|entry| entry.action.as_str())
            .collect::<Vec<_>>();
        assert!(audit_actions.contains(&"file.extract"));
        assert!(audit_actions.contains(&"mcp.tool.call"));
        assert!(!snapshot.verification_chains.is_empty());
        assert!(!snapshot.support_matrix_entries.is_empty());
        assert_eq!(snapshot.known_limitations.len(), 36);
        assert_eq!(
            snapshot.support_matrix.documented_limit_count,
            snapshot.known_limitations.len() as u32
        );
        assert!(snapshot
            .known_limitations
            .iter()
            .any(|item| item.category == "Recycle Bin"
                && item.affected_chains.contains(&"RecycleBin".to_string())));
        assert!(snapshot
            .known_limitations
            .iter()
            .any(|item| item.category == "Browser"
                && item.affected_chains.contains(&"ChromeHistory".to_string())));
        assert!(snapshot
            .fact_sources
            .iter()
            .any(|item| item.fact_file == "testdata/governance/v2-known-limitations.json"));
        assert!(!snapshot.error_taxonomy_entries.is_empty());
        assert_eq!(snapshot.release_gates.len(), 7);
        assert_eq!(
            snapshot
                .release_gates
                .iter()
                .find(|gate| gate.gate_id == "core-fixture-regression")
                .map(|gate| gate.status.clone()),
            Some(ReleaseGateStatusDto::Warning)
        );
        assert_eq!(
            snapshot
                .release_gates
                .iter()
                .find(|gate| gate.gate_id == "benchmark-thresholds")
                .map(|gate| gate.status.clone()),
            Some(ReleaseGateStatusDto::Warning)
        );
        assert_eq!(
            snapshot
                .release_gates
                .iter()
                .find(|gate| gate.gate_id == "security-baseline")
                .map(|gate| gate.status.clone()),
            Some(ReleaseGateStatusDto::Passed)
        );
        assert_eq!(
            snapshot
                .release_gates
                .iter()
                .find(|gate| gate.gate_id == "correlation-family-coverage")
                .map(|gate| gate.status.clone()),
            Some(ReleaseGateStatusDto::Blocked)
        );
        assert_eq!(snapshot.release_scorecard.total_score, 70);
        assert_eq!(snapshot.release_scorecard.grade, "C");
        assert!(snapshot
            .release_scorecard
            .residual_risks
            .iter()
            .any(|item| item.contains("关联分析快照没有 lead")));
        assert!(snapshot
            .release_scorecard
            .breakdown
            .iter()
            .find(|entry| entry.dimension == "correlation")
            .map(|entry| entry
                .deductions
                .iter()
                .any(|item| item.contains("关联快照无 lead")))
            .unwrap_or(false));
    }
}
