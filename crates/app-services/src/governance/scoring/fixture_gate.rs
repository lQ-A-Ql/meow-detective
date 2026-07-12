use transport::dto::{
    ParserSupportMatrixSummaryDto, ReleaseGateStatusDto, VerificationChainStatusDto,
    VerificationResultDto,
};

use crate::governance::fact_loader::ReleasePolicyFile;

use super::gate_status::GateResult;

pub(super) fn core_fixture_gate(
    verification_chains: &[VerificationChainStatusDto],
    support_matrix: &ParserSupportMatrixSummaryDto,
    release_policy: &ReleasePolicyFile,
) -> GateResult {
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
