mod binding;
mod digest;
mod query;
mod records;
mod validation;
mod write;

use rusqlite::Connection;

use crate::connection::DbResult;

pub use digest::omap_aggregate_sha256;
pub use records::{
    CephBluestoreOmapAggregate, CephBluestoreOmapScanRecord, CephBluestoreOmapScopeRecord,
    CephBluestoreRbdDirectoryRecord, CephBluestoreRbdHeaderRecord,
};
pub use validation::canonical_scope_identity;

pub const BLUESTORE_OMAP_SCHEMA_VERSION: u32 = 1;
pub const BLUESTORE_OMAP_DECODE_PROFILE: &str = "omap-rbd-v1";

pub struct CephBluestoreOmapRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CephBluestoreOmapRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn find_aggregate(
        &self,
        inventory_id: &str,
    ) -> DbResult<Option<CephBluestoreOmapAggregate>> {
        let transaction = self.conn.unchecked_transaction()?;
        let aggregate = query::find_aggregate(&transaction, inventory_id)?;
        if let Some(aggregate) = &aggregate {
            validation::validate_replacement(aggregate)?;
            binding::validate_persisted_binding(&transaction, aggregate)?;
        }
        transaction.commit()?;
        Ok(aggregate)
    }

    pub fn find_scopes_by_family(
        &self,
        inventory_id: &str,
        key_family: &str,
    ) -> DbResult<Vec<CephBluestoreOmapScopeRecord>> {
        query::find_scopes_by_family(self.conn, inventory_id, key_family)
    }

    pub fn find_scopes_by_owner(
        &self,
        inventory_id: &str,
        owner_nid_hex: &str,
    ) -> DbResult<Vec<CephBluestoreOmapScopeRecord>> {
        query::find_scopes_by_owner(self.conn, inventory_id, owner_nid_hex)
    }

    pub fn find_rbd_header(
        &self,
        inventory_id: &str,
        image_id: &str,
    ) -> DbResult<Option<CephBluestoreRbdHeaderRecord>> {
        query::find_rbd_header(self.conn, inventory_id, image_id)
    }

    pub fn replace_for_inventory(&self, aggregate: &CephBluestoreOmapAggregate) -> DbResult<()> {
        validation::validate_replacement(aggregate)?;
        let transaction = self.conn.unchecked_transaction()?;
        binding::validate_persisted_binding(&transaction, aggregate)?;
        write::replace_for_inventory_on(&transaction, aggregate)?;
        transaction.commit()?;
        Ok(())
    }
}

pub fn validate_replacement(aggregate: &CephBluestoreOmapAggregate) -> DbResult<()> {
    validation::validate_replacement(aggregate)
}

pub(crate) use binding::validate_recovery_binding;

pub(crate) fn replace_validated_for_inventory_on(
    conn: &Connection,
    aggregate: &CephBluestoreOmapAggregate,
) -> DbResult<()> {
    write::replace_for_inventory_on(conn, aggregate)
}
