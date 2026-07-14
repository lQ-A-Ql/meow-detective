use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use domain::DataSourceId;
use persistence_sqlite::repositories::ceph_rocksdb_latest_state_repo::CephRocksdbLatestStateRecord;
use persistence_sqlite::repositories::ceph_rocksdb_repo::CephRocksdbAggregate;
use persistence_sqlite::repositories::ceph_rocksdb_sst_repo::CephRocksdbSstRecord;
use persistence_sqlite::repositories::ceph_rocksdb_wal_repo::CephRocksdbWalAggregate;
use transport::CommandError;

use super::ceph_bluefs_file_reader::BluefsExtentReader;
use super::ceph_bluefs_replay::BluefsReplaySnapshot;

pub(super) fn inventory_rocksdb_manifest(
    reader: &mut BluefsExtentReader<'_>,
    replay: &BluefsReplaySnapshot,
    case_root: &Path,
    data_source_id: &str,
    inventory_id: &str,
    cancel_token: &AtomicBool,
) -> Result<RocksdbInventoryAggregate, CommandError> {
    let inventory_started = Instant::now();
    let control = super::ceph_rocksdb_control_files::read_rocksdb_control_files(reader, replay)?;
    let snapshot = rocksdb_wire::decode_manifest(
        &control.manifest_bytes,
        rocksdb_wire::ManifestDecodeLimits::default(),
    )
    .map_err(map_manifest_error)?;
    let aggregate = super::ceph_rocksdb_records::build_rocksdb_aggregate(
        data_source_id,
        inventory_id,
        control,
        snapshot,
    )?;
    let sharding =
        super::ceph_rocksdb_sharding::read_rocksdb_sharding_definition(reader, replay, &aggregate)?;
    let mut spool = super::ceph_rocksdb_spool::RocksdbRecoverySpool::create(
        case_root,
        &DataSourceId(data_source_id.to_string()),
    )?;
    let located = super::ceph_rocksdb_sst_locator::locate_live_rocksdb_ssts(replay, &aggregate)?;
    let sst_started = Instant::now();
    let ssts = super::ceph_rocksdb_sst_inventory::inventory_live_rocksdb_ssts(
        reader,
        &sharding,
        &located,
        cancel_token,
        &mut spool,
    )?;
    let sst_elapsed_ms = sst_started.elapsed().as_millis();
    let wal_selection =
        super::ceph_rocksdb_wal_locator::locate_active_rocksdb_wals(replay, &aggregate)?;
    let wal_started = Instant::now();
    let wals = super::ceph_rocksdb_wal_inventory::inventory_active_rocksdb_wals(
        reader,
        &wal_selection,
        &aggregate,
        cancel_token,
        &mut spool,
    )?;
    let wal_elapsed_ms = wal_started.elapsed().as_millis();
    let seal_started = Instant::now();
    spool.seal()?;
    let seal_elapsed_ms = seal_started.elapsed().as_millis();
    let recovery_started = Instant::now();
    let latest_state =
        super::ceph_rocksdb_latest_state::recover_latest_state(&aggregate, &sharding, &spool)?;
    let recovery_elapsed_ms = recovery_started.elapsed().as_millis();
    tracing::info!(
        data_source_id,
        inventory_id,
        point_mutations = spool.point_count(),
        range_tombstones = spool.range_count(),
        spool_raw_bytes = spool.raw_bytes(),
        sst_elapsed_ms,
        wal_elapsed_ms,
        seal_elapsed_ms,
        recovery_elapsed_ms,
        total_elapsed_ms = inventory_started.elapsed().as_millis(),
        "Recovered bounded Ceph RocksDB latest-state summaries"
    );
    Ok(RocksdbInventoryAggregate {
        manifest: aggregate,
        ssts,
        wals,
        latest_state,
    })
}

pub(super) struct RocksdbInventoryAggregate {
    pub(super) manifest: CephRocksdbAggregate,
    pub(super) ssts: Vec<CephRocksdbSstRecord>,
    pub(super) wals: CephRocksdbWalAggregate,
    pub(super) latest_state: Vec<CephRocksdbLatestStateRecord>,
}

fn map_manifest_error(error: rocksdb_wire::RocksDbWireError) -> CommandError {
    let message = format!("RocksDB MANIFEST decode failed: {error}");
    match error {
        rocksdb_wire::RocksDbWireError::UnsupportedWalCompressionRecord { .. }
        | rocksdb_wire::RocksDbWireError::UnsupportedTrackedWalEdit { .. }
        | rocksdb_wire::RocksDbWireError::UnknownMandatoryTag { .. }
        | rocksdb_wire::RocksDbWireError::UnknownMandatoryCustomTag { .. } => {
            CommandError::unsupported(message)
        }
        _ => CommandError::parser(message),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/import_pipeline/ceph_rocksdb_inventory.rs"]
mod tests;
