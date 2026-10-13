use artifacts_windows::registry::log_replay::{
    LogDisposition, LogFormat, LogReplayReport, LogSlot, RecoveryState,
};
use transport::dto::*;

pub(super) fn summary(
    report: &LogReplayReport,
    base_sha256: Option<String>,
    result_sha256: Option<String>,
    mut logs: Vec<RegistryLogProvenanceDto>,
) -> RegistryRecoveryDto {
    for item in &report.logs {
        let slot = match item.slot {
            LogSlot::Log1 => RegistryLogSlotDto::Log1,
            LogSlot::Log2 => RegistryLogSlotDto::Log2,
        };
        let Some(log) = logs.iter_mut().find(|l| l.slot == slot) else {
            continue;
        };
        if matches!(
            log.status,
            RegistryLogStatusDto::Missing
                | RegistryLogStatusDto::Ambiguous
                | RegistryLogStatusDto::Locked
                | RegistryLogStatusDto::TooLarge
                | RegistryLogStatusDto::Unavailable
        ) {
            continue;
        }
        log.status = match item.disposition {
            LogDisposition::Applied => RegistryLogStatusDto::Applied,
            LogDisposition::NotNeeded => RegistryLogStatusDto::NotNeeded,
            LogDisposition::Stale => RegistryLogStatusDto::Stale,
            LogDisposition::Invalid => RegistryLogStatusDto::Invalid,
            LogDisposition::Unavailable => RegistryLogStatusDto::Unavailable,
            LogDisposition::Duplicate => RegistryLogStatusDto::Duplicate,
            LogDisposition::NotSelected => RegistryLogStatusDto::NotSelected,
        };
        log.format = item.format.map(|f| match f {
            LogFormat::Hvle => RegistryLogFormatDto::Hvle,
            LogFormat::Dirt => RegistryLogFormatDto::Dirt,
        });
        log.first_sequence = item.first_sequence;
        log.last_sequence = item.last_sequence;
        log.applied_entries = item.applied_entries;
        log.applied_pages = item.applied_pages;
        log.diagnostic_code = item.diagnostic.map(|e| e.code().into());
    }
    let limited = logs.iter().any(|l| {
        matches!(
            l.status,
            RegistryLogStatusDto::Ambiguous
                | RegistryLogStatusDto::Locked
                | RegistryLogStatusDto::TooLarge
                | RegistryLogStatusDto::Unavailable
        )
    });
    RegistryRecoveryDto {
        status: match report.state {
            RecoveryState::NotNeeded => RegistryRecoveryStatusDto::NotNeeded,
            RecoveryState::Recovered if limited => RegistryRecoveryStatusDto::Partial,
            RecoveryState::Recovered => RegistryRecoveryStatusDto::Recovered,
            RecoveryState::Partial => RegistryRecoveryStatusDto::Partial,
            RecoveryState::Failed => RegistryRecoveryStatusDto::Failed,
        },
        base_sha256,
        result_sha256,
        base_primary_sequence: report.base_primary,
        base_secondary_sequence: report.base_secondary,
        base_checksum_valid: report.base_checksum_valid,
        result_sequence: report.result_sequence,
        applied_entries: report.applied_entries,
        applied_pages: report.applied_pages,
        logs,
        diagnostic_codes: report.diagnostics.iter().map(|e| e.code().into()).collect(),
    }
}
