mod queries;
mod records;
mod replacement;
mod validation;

use rusqlite::Connection;

use crate::connection::DbResult;

use super::ceph_bluefs_repo::CephBluefsAggregate;
use super::ceph_osd_device_binding_repo::CephOsdDeviceBindingAggregate;
use replacement::CephAggregateReplacement;

pub use records::{CephOsdInventoryRecord, CephOsdLabelReplicaRecord, CephRocksdbMetadataSnapshot};

pub struct CephOsdRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CephOsdRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn replace_for_data_source(
        &self,
        data_source_id: &str,
        inventory: &[CephOsdInventoryRecord],
        replicas: &[CephOsdLabelReplicaRecord],
    ) -> DbResult<()> {
        replacement::replace_aggregate(
            self.conn,
            data_source_id,
            inventory,
            replicas,
            CephAggregateReplacement::default(),
        )
    }

    pub fn replace_for_data_source_with_bluefs(
        &self,
        data_source_id: &str,
        inventory: &[CephOsdInventoryRecord],
        replicas: &[CephOsdLabelReplicaRecord],
        bluefs: Option<&CephBluefsAggregate>,
    ) -> DbResult<()> {
        replacement::replace_aggregate(
            self.conn,
            data_source_id,
            inventory,
            replicas,
            CephAggregateReplacement::with_bluefs(bluefs),
        )
    }

    pub fn replace_for_data_source_with_rocksdb_metadata(
        &self,
        data_source_id: &str,
        inventory: &[CephOsdInventoryRecord],
        replicas: &[CephOsdLabelReplicaRecord],
        metadata: CephRocksdbMetadataSnapshot<'_>,
    ) -> DbResult<()> {
        replacement::replace_aggregate(
            self.conn,
            data_source_id,
            inventory,
            replicas,
            metadata.into(),
        )
    }

    pub fn replace_for_data_source_with_device_bindings(
        &self,
        data_source_id: &str,
        inventory: &[CephOsdInventoryRecord],
        replicas: &[CephOsdLabelReplicaRecord],
        device_bindings: &[CephOsdDeviceBindingAggregate],
    ) -> DbResult<()> {
        replacement::replace_aggregate(
            self.conn,
            data_source_id,
            inventory,
            replicas,
            CephAggregateReplacement::with_device_bindings(device_bindings),
        )
    }

    pub fn replace_for_data_source_with_rocksdb_metadata_and_device_bindings(
        &self,
        data_source_id: &str,
        inventory: &[CephOsdInventoryRecord],
        replicas: &[CephOsdLabelReplicaRecord],
        metadata: CephRocksdbMetadataSnapshot<'_>,
        device_bindings: &[CephOsdDeviceBindingAggregate],
    ) -> DbResult<()> {
        replacement::replace_aggregate(
            self.conn,
            data_source_id,
            inventory,
            replicas,
            CephAggregateReplacement::with_metadata_and_device_bindings(metadata, device_bindings),
        )
    }

    pub fn find_by_data_source(
        &self,
        data_source_id: &str,
    ) -> DbResult<Vec<CephOsdInventoryRecord>> {
        queries::find_by_data_source(self.conn, data_source_id)
    }

    pub fn find_label_replicas(
        &self,
        inventory_id: &str,
    ) -> DbResult<Vec<CephOsdLabelReplicaRecord>> {
        queries::find_label_replicas(self.conn, inventory_id)
    }
}
