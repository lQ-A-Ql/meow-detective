mod digest;
mod query;
mod records;
mod validation;
mod write;

use rusqlite::Connection;

use crate::connection::DbResult;

pub use digest::cephfs_lineage_fingerprint;
pub use records::{
    CephFsDerivedLineageAggregate, CephFsDerivedLineageRecord, CephFsDerivedMapProvenanceRecord,
    CephFsDerivedPoolRecord, CephFsDerivedPoolSourceRecord,
};

pub struct CephFsDerivedLineageRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CephFsDerivedLineageRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, aggregate: &CephFsDerivedLineageAggregate) -> DbResult<()> {
        validation::validate_aggregate(aggregate)?;
        let transaction = self.conn.unchecked_transaction()?;
        validation::validate_ownership(&transaction, aggregate)?;
        write::insert(&transaction, aggregate)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn find_by_data_source(
        &self,
        data_source_id: &str,
    ) -> DbResult<Option<CephFsDerivedLineageAggregate>> {
        let aggregate = query::find(self.conn, data_source_id)?;
        if let Some(aggregate) = &aggregate {
            validation::validate_aggregate(aggregate)?;
        }
        Ok(aggregate)
    }
}

pub fn insert_cephfs_lineage_in_transaction(
    conn: &Connection,
    aggregate: &CephFsDerivedLineageAggregate,
) -> DbResult<()> {
    validation::validate_aggregate(aggregate)?;
    validation::validate_ownership(conn, aggregate)?;
    write::insert(conn, aggregate)
}

pub fn validate_cephfs_lineage(aggregate: &CephFsDerivedLineageAggregate) -> DbResult<()> {
    validation::validate_aggregate(aggregate)
}
