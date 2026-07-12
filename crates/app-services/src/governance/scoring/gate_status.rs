use transport::dto::{ReleaseGateEntryDto, ReleaseGateStatusDto};

use crate::governance::fact_loader::RuntimeGateFactFile;

pub(super) type GateResult = (ReleaseGateStatusDto, String, String);

pub(super) fn merge_runtime_gate_fact(
    derived_status: ReleaseGateStatusDto,
    derived_evidence: String,
    derived_detail: String,
    runtime_fact: &RuntimeGateFactFile,
    checked_at: &str,
) -> GateResult {
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

pub(super) fn worse_gate_status(
    left: ReleaseGateStatusDto,
    right: ReleaseGateStatusDto,
) -> ReleaseGateStatusDto {
    if gate_status_rank(&left) >= gate_status_rank(&right) {
        left
    } else {
        right
    }
}

pub(super) fn gate_status_rank(status: &ReleaseGateStatusDto) -> u8 {
    match status {
        ReleaseGateStatusDto::Passed => 0,
        ReleaseGateStatusDto::Warning => 1,
        ReleaseGateStatusDto::Blocked => 2,
    }
}

pub(super) fn gate_status(
    gates: &[ReleaseGateEntryDto],
    gate_id: &str,
) -> Option<ReleaseGateStatusDto> {
    gates
        .iter()
        .find(|gate| gate.gate_id == gate_id)
        .map(|gate| gate.status.clone())
}
