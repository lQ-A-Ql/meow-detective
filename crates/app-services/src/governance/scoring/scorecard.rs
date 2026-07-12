use transport::dto::{
    GovernanceRuntimeSignalsDto, ReleaseGateEntryDto, ReleaseGateStatusDto,
    ReleaseScoreBreakdownEntryDto, ReleaseScorecardDto,
};

use crate::governance::fact_loader::release_policy;

use super::{
    contributions::{
        apply_score_policy, correlation_contributions, performance_contributions,
        security_contributions, verification_contributions,
    },
    gate_status::gate_status,
};

pub(crate) fn release_scorecard(
    gates: &[ReleaseGateEntryDto],
    runtime_signals: &GovernanceRuntimeSignalsDto,
) -> ReleaseScorecardDto {
    let release_policy = release_policy();
    let verification_score = apply_score_policy(
        &release_policy.score_policy.verification,
        &verification_contributions(gates, runtime_signals),
    );
    let correlation_score = apply_score_policy(
        &release_policy.score_policy.correlation,
        &correlation_contributions(gates, runtime_signals),
    );
    let performance_score = apply_score_policy(
        &release_policy.score_policy.performance,
        &performance_contributions(gates, runtime_signals),
    );
    let security_score = apply_score_policy(
        &release_policy.score_policy.security,
        &security_contributions(gates, runtime_signals),
    );
    let total_score = verification_score + correlation_score + performance_score + security_score;

    ReleaseScorecardDto {
        total_score,
        grade: grade(total_score).to_string(),
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
        blockers: blockers(gates, runtime_signals),
        residual_risks: residual_risks(gates, runtime_signals),
    }
}

fn grade(total_score: u32) -> &'static str {
    if total_score >= 90 {
        "A"
    } else if total_score >= 80 {
        "B"
    } else if total_score >= 70 {
        "C"
    } else {
        "D"
    }
}

fn blockers(
    gates: &[ReleaseGateEntryDto],
    runtime_signals: &GovernanceRuntimeSignalsDto,
) -> Vec<String> {
    let mut blockers = gates
        .iter()
        .filter(|gate| gate.status == ReleaseGateStatusDto::Blocked)
        .map(|gate| format!("{}：{}", gate.title, gate.detail))
        .collect::<Vec<_>>();
    if runtime_signals.pending_hash_data_source_count > 0 {
        blockers.push("仍有证据源哈希未完成，发布前需说明可信边界。".to_string());
    }
    blockers
}

fn residual_risks(
    gates: &[ReleaseGateEntryDto],
    runtime_signals: &GovernanceRuntimeSignalsDto,
) -> Vec<String> {
    let mut residual_risks = release_policy().baseline_residual_risks.clone();
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
    residual_risks
}

fn release_score_breakdown(
    gates: &[ReleaseGateEntryDto],
    runtime_signals: &GovernanceRuntimeSignalsDto,
    verification_score: u32,
    correlation_score: u32,
    performance_score: u32,
    security_score: u32,
) -> Vec<ReleaseScoreBreakdownEntryDto> {
    let release_policy = release_policy();
    let dimensions = [
        (
            "verification",
            release_policy.score_policy.verification.max_score,
            verification_score,
            verification_contributions(gates, runtime_signals),
        ),
        (
            "correlation",
            release_policy.score_policy.correlation.max_score,
            correlation_score,
            correlation_contributions(gates, runtime_signals),
        ),
        (
            "performance",
            release_policy.score_policy.performance.max_score,
            performance_score,
            performance_contributions(gates, runtime_signals),
        ),
        (
            "security",
            release_policy.score_policy.security.max_score,
            security_score,
            security_contributions(gates, runtime_signals),
        ),
    ];
    dimensions
        .into_iter()
        .map(
            |(dimension, max_score, actual_score, contributions)| ReleaseScoreBreakdownEntryDto {
                dimension: dimension.to_string(),
                max_score,
                actual_score,
                deductions: contributions.into_iter().map(|item| item.message).collect(),
            },
        )
        .collect()
}
