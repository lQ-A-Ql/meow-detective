use transport::dto::{GovernanceRuntimeSignalsDto, ReleaseGateStatusDto};

pub(super) fn correlation_family_gate_status(
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

pub(super) fn correlation_family_gate_detail(
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
