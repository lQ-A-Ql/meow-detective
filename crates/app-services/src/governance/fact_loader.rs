use once_cell::sync::Lazy;
use serde::Deserialize;
use transport::dto::{
    BenchmarkSnapshotDto, ErrorTaxonomyEntryDto, GovernanceFactSourceDto,
    GovernanceRuntimeCheckDto, GovernanceRuntimeResultsDto, GovernanceRuntimeSubcheckDto,
    KnownLimitationDto, ParserSupportMatrixEntryDto, ParserSupportMatrixSummaryDto,
    ReleaseGateStatusDto, SupportMaturityDto, VerificationChainStatusDto,
    VerificationGuaranteeLevelDto, VerificationResultDto,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GovernanceChainCatalogFile {
    pub(crate) chains: Vec<GovernanceChainCatalogEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BenchmarkBaselineFile {
    pub(crate) host_profile: String,
    pub(crate) baseline_version: String,
    pub(crate) last_verified_at: String,
    pub(crate) scenarios: Vec<BenchmarkSnapshotDto>,
    pub(crate) required_checks: Vec<BenchmarkRequirementPolicyFile>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BenchmarkRequirementPolicyFile {
    pub(crate) dataset_level: String,
    pub(crate) scenario: String,
    pub(crate) threshold_p95_ms: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SecurityTaxonomyFile {
    pub(crate) security_defaults: SecurityDefaultsFile,
    pub(crate) error_taxonomy_entries: Vec<ErrorTaxonomyEntryDto>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SecurityDefaultsFile {
    pub(crate) export_overwrite_default: bool,
    pub(crate) export_path_guard_enabled: bool,
    pub(crate) stdio_command_whitelist_enforced: bool,
    pub(crate) sse_https_only: bool,
    pub(crate) embedded_credentials_blocked: bool,
    pub(crate) media_handle_scoped: bool,
    pub(crate) error_redaction_enabled: bool,
    pub(crate) notes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReleasePolicyFile {
    pub(crate) core_fixture_chains: Vec<String>,
    pub(crate) baseline_residual_risks: Vec<String>,
    pub(crate) score_policy: ReleaseScorePolicyFile,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KnownLimitationsFile {
    pub(crate) last_verified_at: String,
    pub(crate) documented_limit_count: u32,
    pub(crate) items: Vec<KnownLimitationDto>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReleaseScorePolicyFile {
    pub(crate) verification: ScoreDimensionPolicyFile,
    pub(crate) correlation: ScoreDimensionPolicyFile,
    pub(crate) performance: ScoreDimensionPolicyFile,
    pub(crate) security: ScoreDimensionPolicyFile,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScoreDimensionPolicyFile {
    pub(crate) max_score: u32,
    pub(crate) deductions: Vec<ScoreDeductionRuleFile>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScoreDeductionRuleFile {
    pub(crate) trigger: String,
    pub(crate) amount: u32,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GovernanceRuntimeResultsFile {
    pub(crate) checked_at: String,
    pub(crate) doc_drift: RuntimeGateFactFile,
    pub(crate) core_fixture_regression: RuntimeGateFactFile,
    pub(crate) benchmark_thresholds: RuntimeGateFactFile,
    pub(crate) security_baseline: RuntimeGateFactFile,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeGateFactFile {
    pub(crate) status: ReleaseGateStatusDto,
    pub(crate) evidence: String,
    pub(crate) detail: String,
    pub(crate) sub_checks: Vec<RuntimeGateSubcheckFile>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeGateSubcheckFile {
    pub(crate) check_id: String,
    pub(crate) title: String,
    pub(crate) status: ReleaseGateStatusDto,
    pub(crate) evidence: String,
    pub(crate) detail: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ScoreContribution {
    pub(crate) amount: u32,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GovernanceChainCatalogEntry {
    pub(crate) chain: String,
    pub(crate) platform: String,
    pub(crate) display_name: String,
    pub(crate) maturity: SupportMaturityDto,
    pub(crate) guarantee_level: VerificationGuaranteeLevelDto,
    pub(crate) fixture_tier: String,
    pub(crate) expected_json_version: String,
    pub(crate) verified_sample_count: u32,
    pub(crate) result: VerificationResultDto,
    pub(crate) notes: Vec<String>,
    pub(crate) verified_samples: Vec<String>,
    pub(crate) baseline: String,
    pub(crate) guarantee_summary: String,
    pub(crate) matrix_notes: Vec<String>,
}

static GOVERNANCE_CHAIN_CATALOG: Lazy<Vec<GovernanceChainCatalogEntry>> = Lazy::new(|| {
    let raw = include_str!("../../../../testdata/governance/v2-verification-catalog.json");
    serde_json::from_str::<GovernanceChainCatalogFile>(raw)
        .expect("parse V2 governance verification catalog")
        .chains
});

static BENCHMARK_BASELINE: Lazy<BenchmarkBaselineFile> = Lazy::new(|| {
    let raw = include_str!("../../../../testdata/governance/v2-benchmark-baseline.json");
    serde_json::from_str::<BenchmarkBaselineFile>(raw)
        .expect("parse V2 governance benchmark baseline")
});

static SECURITY_TAXONOMY: Lazy<SecurityTaxonomyFile> = Lazy::new(|| {
    let raw = include_str!("../../../../testdata/governance/v2-security-taxonomy.json");
    serde_json::from_str::<SecurityTaxonomyFile>(raw)
        .expect("parse V2 governance security taxonomy")
});

static RELEASE_POLICY: Lazy<ReleasePolicyFile> = Lazy::new(|| {
    let raw = include_str!("../../../../testdata/governance/v2-release-policy.json");
    serde_json::from_str::<ReleasePolicyFile>(raw).expect("parse V2 governance release policy")
});

static KNOWN_LIMITATIONS: Lazy<KnownLimitationsFile> = Lazy::new(|| {
    let raw = include_str!("../../../../testdata/governance/v2-known-limitations.json");
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
    let raw = include_str!("../../../../testdata/governance/v2-runtime-results.json");
    serde_json::from_str::<GovernanceRuntimeResultsFile>(raw)
        .expect("parse V2 governance runtime results")
});

pub(crate) fn governance_chain_catalog() -> &'static [GovernanceChainCatalogEntry] {
    &GOVERNANCE_CHAIN_CATALOG
}

pub(crate) fn benchmark_baseline() -> &'static BenchmarkBaselineFile {
    &BENCHMARK_BASELINE
}

pub(crate) fn security_taxonomy() -> &'static SecurityTaxonomyFile {
    &SECURITY_TAXONOMY
}

pub(crate) fn release_policy() -> &'static ReleasePolicyFile {
    &RELEASE_POLICY
}

pub(crate) fn known_limitations_file() -> &'static KnownLimitationsFile {
    &KNOWN_LIMITATIONS
}

pub(crate) fn governance_runtime_results() -> &'static GovernanceRuntimeResultsFile {
    &GOVERNANCE_RUNTIME_RESULTS
}

pub(crate) fn governance_fact_sources() -> Vec<GovernanceFactSourceDto> {
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

pub(crate) fn governance_runtime_results_dto() -> GovernanceRuntimeResultsDto {
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

pub(crate) fn runtime_subcheck_dto(item: &RuntimeGateSubcheckFile) -> GovernanceRuntimeSubcheckDto {
    GovernanceRuntimeSubcheckDto {
        check_id: item.check_id.clone(),
        title: item.title.clone(),
        status: item.status.clone(),
        evidence: item.evidence.clone(),
        detail: item.detail.clone(),
    }
}

pub(crate) fn verification_chains(
    catalog: &[GovernanceChainCatalogEntry],
) -> Vec<VerificationChainStatusDto> {
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

pub(crate) fn support_matrix_entries(
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

pub(crate) fn support_matrix_summary(
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

pub(crate) fn known_limitations() -> Vec<KnownLimitationDto> {
    known_limitations_file().items.clone()
}
