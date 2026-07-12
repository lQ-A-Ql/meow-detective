use transport::dto::{
    BenchmarkRequiredCheckDto, BenchmarkRequirementStatusDto, BenchmarkSnapshotDto,
    BenchmarkSummaryDto, ReleaseGateStatusDto,
};

use crate::governance::fact_loader::BenchmarkRequirementPolicyFile;

use super::gate_status::GateResult;

pub(super) fn benchmark_gate(benchmark: &BenchmarkSummaryDto) -> GateResult {
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
