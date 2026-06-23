use transport::dto::{
    BenchmarkRequiredCheckDto, BenchmarkRequirementStatusDto, BenchmarkSnapshotDto,
    BenchmarkSummaryDto, GovernanceRuntimeSignalsDto, ReleaseGateEntryDto, ReleaseGateStatusDto,
    ReleaseScoreBreakdownEntryDto, ReleaseScorecardDto, SecurityAuditSummaryDto,
    VerificationChainStatusDto, VerificationResultDto,
};

use crate::governance::fact_loader::*;

pub(crate) fn release_gates(
    verification_chains: &[VerificationChainStatusDto],
    support_matrix: &transport::dto::ParserSupportMatrixSummaryDto,
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

pub(crate) fn merge_runtime_gate_fact(
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

pub(crate) fn worse_gate_status(
    left: ReleaseGateStatusDto,
    right: ReleaseGateStatusDto,
) -> ReleaseGateStatusDto {
    if gate_status_rank(&left) >= gate_status_rank(&right) {
        left
    } else {
        right
    }
}

pub(crate) fn gate_status_rank(status: &ReleaseGateStatusDto) -> u8 {
    match status {
        ReleaseGateStatusDto::Passed => 0,
        ReleaseGateStatusDto::Warning => 1,
        ReleaseGateStatusDto::Blocked => 2,
    }
}

pub(crate) fn correlation_family_gate_status(
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

pub(crate) fn correlation_family_gate_detail(
    runtime_signals: &GovernanceRuntimeSignalsDto,
) -> String {
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

pub(crate) fn release_scorecard(
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

pub(crate) fn release_score_breakdown(
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

pub(crate) fn verification_contributions(
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

pub(crate) fn correlation_contributions(
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

pub(crate) fn performance_contributions(
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

pub(crate) fn security_contributions(
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

pub(crate) fn apply_score_policy(
    policy: &ScoreDimensionPolicyFile,
    contributions: &[ScoreContribution],
) -> u32 {
    let total_deduction = contributions.iter().map(|item| item.amount).sum::<u32>();
    policy.max_score.saturating_sub(total_deduction)
}

pub(crate) fn score_contributions_for(
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

pub(crate) fn score_trigger_matches(
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

pub(crate) fn core_fixture_gate(
    verification_chains: &[VerificationChainStatusDto],
    support_matrix: &transport::dto::ParserSupportMatrixSummaryDto,
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

pub(crate) fn benchmark_gate(
    benchmark: &BenchmarkSummaryDto,
) -> (ReleaseGateStatusDto, String, String) {
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

pub(crate) fn benchmark_required_checks(
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

pub(crate) fn security_gate(
    security: &SecurityAuditSummaryDto,
) -> (ReleaseGateStatusDto, String, String) {
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

pub(crate) fn gate_status(
    gates: &[ReleaseGateEntryDto],
    gate_id: &str,
) -> Option<ReleaseGateStatusDto> {
    gates
        .iter()
        .find(|gate| gate.gate_id == gate_id)
        .map(|gate| gate.status.clone())
}
