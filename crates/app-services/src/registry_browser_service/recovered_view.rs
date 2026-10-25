use super::{
    companion_logs,
    io::read_snapshot,
    recovery_mapping,
    registry::{RegistryState, MAX_HIVE_BYTES, METADATA_BUDGET},
    RegistryBrowserError,
};
use crate::file_service::SourceReadContext;
use artifacts_windows::registry::log_replay::{prepare_log_replay, HiveLog, LogSlot};
use artifacts_windows::registry::{reader::HiveView, RegistrySnapshot};
use domain::FileEntry;
use sha2::{Digest, Sha256};
use transport::dto::{RegistryLogSlotDto, RegistryRecoveryDto};

const REPLAY_WORK_BYTES: usize = 24 * 1024 * 1024;
pub(super) fn load(
    state: &mut RegistryState,
    context: &mut SourceReadContext<'_>,
    entry: &FileEntry,
    size: usize,
    companions: Vec<FileEntry>,
) -> Result<(HiveView, RegistryRecoveryDto), RegistryBrowserError> {
    let mut logs = companion_logs::sources(companions, &entry.name);
    let log_bytes = companion_logs::sizes(context, &mut logs);
    state.reserve(reservation(size, log_bytes))?;
    state.log_reads += companion_logs::load(context, &mut logs)?;
    let prefix = crate::file_service::read_file_bytes_for_case(&mut *context, &entry.id, 0, 512)
        .map_err(|_| RegistryBrowserError::SourceUnavailable)?;
    let inputs: Vec<_> = logs
        .iter()
        .map(|log| HiveLog {
            slot: match log.metadata.slot {
                RegistryLogSlotDto::Log1 => LogSlot::Log1,
                RegistryLogSlotDto::Log2 => LogSlot::Log2,
            },
            bytes: log.bytes.as_deref(),
        })
        .collect();
    let plan = prepare_log_replay(&prefix, &inputs, MAX_HIVE_BYTES).map_err(|error| {
        RegistryBrowserError::Recovery(Box::new(recovery_mapping::summary(
            &error.report,
            None,
            None,
            logs.iter().map(|l| l.metadata.clone()).collect(),
        )))
    })?;
    let capacity = plan.required_capacity(size);
    state.reserve(reservation(capacity, log_bytes))?;
    let bytes = read_snapshot(context, &entry.id, size, capacity)?;
    state.reads += 1;
    let base_hash = hex::encode(Sha256::digest(&bytes));
    let recovered = plan.apply(bytes).map_err(|error| {
        RegistryBrowserError::Recovery(Box::new(recovery_mapping::summary(
            &error.report,
            Some(base_hash.clone()),
            None,
            logs.iter().map(|l| l.metadata.clone()).collect(),
        )))
    })?;
    let result_hash = hex::encode(Sha256::digest(&recovered.bytes));
    let summary = recovery_mapping::summary(
        &recovered.report,
        Some(base_hash),
        Some(result_hash),
        logs.into_iter().map(|l| l.metadata).collect(),
    );
    let hive = RegistrySnapshot::from_replayed(recovered).into_view()?;
    Ok((hive, summary))
}
fn reservation(snapshot: usize, log_bytes: usize) -> usize {
    snapshot + snapshot.div_ceil(64) + log_bytes + METADATA_BUDGET + REPLAY_WORK_BYTES
}
