use transport::dto::{GovernanceRuntimeSignalsDto, ReleaseGateEntryDto, ReleaseGateStatusDto};

use crate::governance::fact_loader::{release_policy, ScoreContribution, ScoreDimensionPolicyFile};

use super::gate_status::gate_status;

pub(super) fn verification_contributions(
    gates: &[ReleaseGateEntryDto],
    runtime_signals: &GovernanceRuntimeSignalsDto,
) -> Vec<ScoreContribution> {
    dimension_contributions(
        &release_policy().score_policy.verification,
        gates,
        runtime_signals,
    )
}

pub(super) fn correlation_contributions(
    gates: &[ReleaseGateEntryDto],
    runtime_signals: &GovernanceRuntimeSignalsDto,
) -> Vec<ScoreContribution> {
    dimension_contributions(
        &release_policy().score_policy.correlation,
        gates,
        runtime_signals,
    )
}

pub(super) fn performance_contributions(
    gates: &[ReleaseGateEntryDto],
    runtime_signals: &GovernanceRuntimeSignalsDto,
) -> Vec<ScoreContribution> {
    dimension_contributions(
        &release_policy().score_policy.performance,
        gates,
        runtime_signals,
    )
}

pub(super) fn security_contributions(
    gates: &[ReleaseGateEntryDto],
    runtime_signals: &GovernanceRuntimeSignalsDto,
) -> Vec<ScoreContribution> {
    dimension_contributions(
        &release_policy().score_policy.security,
        gates,
        runtime_signals,
    )
}

fn dimension_contributions(
    policy: &ScoreDimensionPolicyFile,
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
    score_contributions_for(policy, gates, runtime_signals, blocked_count, warning_count)
}

pub(super) fn apply_score_policy(
    policy: &ScoreDimensionPolicyFile,
    contributions: &[ScoreContribution],
) -> u32 {
    let total_deduction = contributions.iter().map(|item| item.amount).sum::<u32>();
    policy.max_score.saturating_sub(total_deduction)
}

pub(super) fn score_contributions_for(
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

pub(super) fn score_trigger_matches(
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
