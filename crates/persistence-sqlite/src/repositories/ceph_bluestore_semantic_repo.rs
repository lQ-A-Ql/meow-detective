mod binding;
mod query;
mod rows;
pub(crate) mod validation;
mod write;

use rusqlite::Connection;

use crate::connection::DbResult;

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
