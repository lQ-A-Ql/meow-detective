use std::time::Instant;

use persistence_sqlite::repositories::ceph_osd_device_binding_repo::CephOsdDeviceBindingAggregate;
use persistence_sqlite::repositories::ceph_osd_repo::{
    CephOsdInventoryRecord, CephOsdLabelReplicaRecord, CephOsdRepo, CephRocksdbMetadataSnapshot,
};
use transport::CommandError;

use super::ceph_metadata_audit::CephMetadataAggregate;

pub(super) fn persist_probe_records(
    repo: &CephOsdRepo<'_>,
    data_source_id: &str,
    inventory: &CephOsdInventoryRecord,
    replicas: &[CephOsdLabelReplicaRecord],
    device_binding: &CephOsdDeviceBindingAggregate,
    metadata: Option<&CephMetadataAggregate>,
) -> Result<(), CommandError> {
    let started = Instant::now();
    let rss_before_mb = crate::import_analysis::current_rss_mb();
    let peak_rss_before_mb = crate::import_analysis::peak_rss_mb();
    let inventory = std::slice::from_ref(inventory);
    let bindings = std::slice::from_ref(device_binding);
    let result = match metadata {
        Some(records) => repo.replace_for_data_source_with_rocksdb_metadata_and_device_bindings(
            data_source_id,
            inventory,
            replicas,
            CephRocksdbMetadataSnapshot {
                bluefs: &records.bluefs,
                rocksdb: &records.rocksdb,
                ssts: &records.ssts,
                wals: &records.wals,
                latest_state: &records.latest_state,
                semantic: &records.semantic,
                omap: &records.omap,
            },
            bindings,
        ),
        None => repo.replace_for_data_source_with_device_bindings(
            data_source_id,
            inventory,
            replicas,
            bindings,
        ),
    };
    tracing::info!(
        data_source_id,
        inventory_id = inventory[0].id,
        semantic_object_rows = metadata.map_or(0, |records| records.semantic.objects.len()),
        semantic_checksum_rows =
            metadata.map_or(0, |records| records.semantic.checksum_chunks.len()),
        omap_scope_rows = metadata.map_or(0, |records| records.omap.scopes.len()),
        omap_directory_rows = metadata.map_or(0, |records| records.omap.directory_mappings.len()),
        omap_header_rows = metadata.map_or(0, |records| records.omap.rbd_headers.len()),
        success = result.is_ok(),
        elapsed_ms = started.elapsed().as_millis(),
        rss_before_mb,
        rss_after_mb = crate::import_analysis::current_rss_mb(),
        peak_rss_before_mb,
        peak_rss_after_mb = crate::import_analysis::peak_rss_mb(),
        "Persisted Ceph metadata and source-bound device identity"
    );
    result.map_err(CommandError::from_service_error)
}
