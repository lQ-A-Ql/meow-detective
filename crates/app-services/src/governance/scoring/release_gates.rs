use transport::dto::{
    BenchmarkSummaryDto, GovernanceRuntimeSignalsDto, ParserSupportMatrixSummaryDto,
    ReleaseGateEntryDto, ReleaseGateStatusDto, SecurityAuditSummaryDto, VerificationChainStatusDto,
};

use crate::governance::fact_loader::{
    governance_runtime_results, ReleasePolicyFile, RuntimeGateFactFile,
};

use super::{
    benchmark_gate::benchmark_gate,
    correlation_gate::{correlation_family_gate_detail, correlation_family_gate_status},
    fixture_gate::core_fixture_gate,
    gate_status::{merge_runtime_gate_fact, GateResult},
    security_gate::security_gate,
};

pub(crate) fn release_gates(
    verification_chains: &[VerificationChainStatusDto],
    support_matrix: &ParserSupportMatrixSummaryDto,
    benchmark: &BenchmarkSummaryDto,
    security: &SecurityAuditSummaryDto,
    runtime_signals: &GovernanceRuntimeSignalsDto,
    release_policy: &ReleasePolicyFile,
) -> Vec<ReleaseGateEntryDto> {
    let runtime_results = governance_runtime_results();
    let core_fixture = merge_gate(
        core_fixture_gate(verification_chains, support_matrix, release_policy),
        &runtime_results.core_fixture_regression,
        &runtime_results.checked_at,
    );
    let benchmark = merge_gate(
        benchmark_gate(benchmark),
        &runtime_results.benchmark_thresholds,
        &runtime_results.checked_at,
    );
    let security = merge_gate(
        security_gate(security),
        &runtime_results.security_baseline,
        &runtime_results.checked_at,
    );

    vec![
        gate_entry("core-fixture-regression", "核心 fixture 回归", core_fixture),
        docs_drift_gate(runtime_results),
        gate_entry("benchmark-thresholds", "Benchmark 阈值", benchmark),
        gate_entry("security-baseline", "安全基线", security),
        evidence_hash_gate(runtime_signals),
        runtime_failure_gate(runtime_signals),
        correlation_family_gate(runtime_signals),
    ]
}

fn merge_gate(
    (status, evidence, detail): GateResult,
    runtime_fact: &RuntimeGateFactFile,
    checked_at: &str,
) -> GateResult {
    merge_runtime_gate_fact(status, evidence, detail, runtime_fact, checked_at)
}

fn gate_entry(
    gate_id: &str,
    title: &str,
    (status, evidence, detail): GateResult,
) -> ReleaseGateEntryDto {
    ReleaseGateEntryDto {
        gate_id: gate_id.to_string(),
        title: title.to_string(),
        status,
        evidence,
        detail,
    }
}

fn docs_drift_gate(
    runtime_results: &crate::governance::fact_loader::GovernanceRuntimeResultsFile,
) -> ReleaseGateEntryDto {
    gate_entry(
        "docs-drift",
        "文档防漂移",
        (
            runtime_results.doc_drift.status.clone(),
            format!(
                "checkedAt={}; {}",
                runtime_results.checked_at, runtime_results.doc_drift.evidence
            ),
            runtime_results.doc_drift.detail.clone(),
        ),
    )
}

fn evidence_hash_gate(runtime_signals: &GovernanceRuntimeSignalsDto) -> ReleaseGateEntryDto {
    let pending_hash = runtime_signals.pending_hash_data_source_count > 0;
    gate_entry(
        "evidence-hash-completeness",
        "证据哈希完整性",
        (
            if pending_hash {
                ReleaseGateStatusDto::Warning
            } else {
                ReleaseGateStatusDto::Passed
            },
            format!(
                "hashed={}, pending={}",
                runtime_signals.hashed_data_source_count,
                runtime_signals.pending_hash_data_source_count
            ),
            if pending_hash {
                "仍有证据源哈希未完成，发布前必须说明可信边界".to_string()
            } else {
                "当前案件证据源哈希已完成或无待处理项".to_string()
            },
        ),
    )
}

fn runtime_failure_gate(runtime_signals: &GovernanceRuntimeSignalsDto) -> ReleaseGateEntryDto {
    let failed_jobs = runtime_signals.failed_job_count > 0;
    let running_jobs = runtime_signals.running_job_count > 0;
    let partial_jobs = runtime_signals.partial_job_count > 0;
    let warning_sources = runtime_signals.warning_data_source_count > 0;
    let status = if failed_jobs {
        ReleaseGateStatusDto::Blocked
    } else if running_jobs || partial_jobs || warning_sources {
        ReleaseGateStatusDto::Warning
    } else {
        ReleaseGateStatusDto::Passed
    };
    let detail = if failed_jobs {
        "存在 failed job，候选发布必须先清理真实链路失败".to_string()
    } else if running_jobs {
        "当前仍有运行中的任务，发布候选前需等待稳定收敛".to_string()
    } else if partial_jobs {
        "当前存在 partial job，需要 investigator 复核部分成功的可信边界".to_string()
    } else if warning_sources {
        "无 failed job，但存在 provenance warning，需要 investigator 明示".to_string()
    } else {
        "当前运行信号未发现阻断级失败".to_string()
    };
    gate_entry(
        "runtime-failures",
        "运行时失败任务",
        (
            status,
            format!(
                "runningJobCount={}, partialJobCount={}, failedJobCount={}, warningDataSourceCount={}",
                runtime_signals.running_job_count,
                runtime_signals.partial_job_count,
                runtime_signals.failed_job_count,
                runtime_signals.warning_data_source_count
            ),
            detail,
        ),
    )
}

fn correlation_family_gate(runtime_signals: &GovernanceRuntimeSignalsDto) -> ReleaseGateEntryDto {
    gate_entry(
        "correlation-family-coverage",
        "关联规则家族覆盖",
        (
            correlation_family_gate_status(runtime_signals),
            format!(
                "snapshotAvailable={}, leadCount={}, coveredFamilies={}, ruleFamilies={}, highConfidenceFamilies={}",
                runtime_signals.correlation_snapshot_available,
                runtime_signals.correlation_lead_count,
                runtime_signals.correlation_covered_family_count,
                runtime_signals.correlation_rule_family_count,
                runtime_signals.correlation_high_confidence_family_count
            ),
            correlation_family_gate_detail(runtime_signals),
        ),
    )
}
