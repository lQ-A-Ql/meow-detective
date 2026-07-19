mod binding;
mod inventory_page;
mod mapping;
mod query;
mod read_plan;
mod rows;
pub(crate) mod validation;
mod write;

use rusqlite::Connection;

use crate::connection::DbResult;

pub use inventory_page::{
    CephBluestoreObjectInventoryEntry, CephBluestoreObjectInventoryPage,
    CephBluestoreObjectPageCursor, MAX_OBJECT_INVENTORY_PAGE_SIZE,
};
pub use read_plan::{
    CephBluestoreObjectCandidate, CephBluestoreObjectReadPlan, CephBluestoreReadPlanSession,
};
pub use rows::{
    CephBluestoreBlobRecord, CephBluestoreChecksumChunkRecord, CephBluestoreCollectionRecord,
    CephBluestoreLogicalExtentRecord, CephBluestoreObjectRecord, CephBluestoreOnodeShardRecord,
    CephBluestorePhysicalExtentRecord, CephBluestoreSemanticAggregate,
    CephBluestoreSemanticScanRecord, CephBluestoreSharedBlobRecord,
    CephBluestoreSharedBlobRefRecord, CephBluestoreSuperRecord,
};
pub use validation::{
    canonical_collection_identity, latest_state_set_sha256, object_identity_sha256,
    semantic_aggregate_sha256,
};

pub const BLUESTORE_SEMANTIC_SCHEMA_VERSION: u32 = 1;
pub const BLUESTORE_SEMANTIC_DECODE_PROFILE: &str = "scox-v1";

pub struct CephBluestoreSemanticRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CephBluestoreSemanticRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn find_aggregate(
        &self,
        inventory_id: &str,
    ) -> DbResult<Option<CephBluestoreSemanticAggregate>> {
        let transaction = self.conn.unchecked_transaction()?;
        let aggregate = query::find_aggregate(&transaction, inventory_id)?;
        if let Some(aggregate) = &aggregate {
            validation::validate_replacement(aggregate)?;
            binding::validate_persisted_binding(&transaction, aggregate)?;
        }
        transaction.commit()?;
        Ok(aggregate)
    }

    pub fn find_object_read_plan(
        &self,
        inventory_id: &str,
        object_identity_sha256: &str,
    ) -> DbResult<Option<CephBluestoreObjectReadPlan>> {
        read_plan::find_object_read_plan(self.conn, inventory_id, object_identity_sha256)
    }

    pub fn find_object_candidate(
        &self,
        inventory_id: &str,
        object_name: &[u8],
        pool: i64,
        namespace: &[u8],
        snap_hex: &str,
    ) -> DbResult<Option<CephBluestoreObjectCandidate>> {
        read_plan::find_object_candidate(
            self.conn,
            inventory_id,
            object_name,
            pool,
            namespace,
            snap_hex,
        )
    }

    pub fn ensure_object_catalog_complete(&self, inventory_id: &str) -> DbResult<()> {
        let Some(scan) = query::find_scan(self.conn, inventory_id)? else {
            return Err(crate::connection::DbError::System(
                "BlueStore object catalog is unavailable".to_string(),
            ));
        };
        validation::validate_scan_for_read(&scan)
    }

    pub fn list_objects_by_pool_after(
        &self,
        inventory_id: &str,
        pool_id: i64,
        after: Option<&CephBluestoreObjectPageCursor>,
        limit: u32,
    ) -> DbResult<CephBluestoreObjectInventoryPage> {
        inventory_page::list_objects_by_pool_after(self.conn, inventory_id, pool_id, after, limit)
    }

    pub fn replace_for_inventory(
        &self,
        aggregate: &CephBluestoreSemanticAggregate,
    ) -> DbResult<()> {
        validation::validate_replacement(aggregate)?;
        let transaction = self.conn.unchecked_transaction()?;
        binding::validate_persisted_binding(&transaction, aggregate)?;
        write::replace_for_inventory_on(&transaction, aggregate)?;
        transaction.commit()?;
        Ok(())
    }
}

pub fn validate_replacement(aggregate: &CephBluestoreSemanticAggregate) -> DbResult<()> {
    validation::validate_replacement(aggregate)
}

pub(crate) fn replace_validated_for_inventory_on(
    conn: &Connection,
    aggregate: &CephBluestoreSemanticAggregate,
) -> DbResult<()> {
    write::replace_for_inventory_on(conn, aggregate)
}

#[cfg(test)]
#[path = "../../tests/unit/repositories/ceph_bluestore_semantic_repo.rs"]
mod tests;
