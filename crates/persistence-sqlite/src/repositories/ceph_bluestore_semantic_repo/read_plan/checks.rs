use rusqlite::Connection;

use crate::connection::{DbError, DbResult};

use super::super::{
    binding, query, validation, CephBluestoreBlobRecord, CephBluestoreChecksumChunkRecord,
    CephBluestoreLogicalExtentRecord, CephBluestoreObjectRecord, CephBluestoreOnodeShardRecord,
    CephBluestorePhysicalExtentRecord, CephBluestoreSemanticAggregate,
    CephBluestoreSharedBlobRecord, CephBluestoreSharedBlobRefRecord,
};
use super::ReadContext;

pub(super) struct ObjectRows {
    pub(super) object: CephBluestoreObjectRecord,
    pub(super) onode_shards: Vec<CephBluestoreOnodeShardRecord>,
    pub(super) blobs: Vec<CephBluestoreBlobRecord>,
    pub(super) logical_extents: Vec<CephBluestoreLogicalExtentRecord>,
    pub(super) physical_extents: Vec<CephBluestorePhysicalExtentRecord>,
    pub(super) checksum_chunks: Vec<CephBluestoreChecksumChunkRecord>,
    pub(super) shared_blobs: Vec<CephBluestoreSharedBlobRecord>,
    pub(super) shared_blob_refs: Vec<CephBluestoreSharedBlobRefRecord>,
}

pub(super) fn read_context(conn: &Connection, inventory_id: &str) -> DbResult<Option<ReadContext>> {
    let Some(scan) = query::find_scan(conn, inventory_id)? else {
        return Ok(None);
    };
    let super_record = query::find_super(conn, inventory_id)?;
    let device_size = binding::validate_persisted_binding_for_read(conn, &scan)?;
    validation::validate_super_for_read(inventory_id, &super_record)?;
    Ok(Some(ReadContext {
        scan,
        super_record,
        device_size,
    }))
}

pub(super) fn object_validation_aggregate(
    context: &ReadContext,
    rows: ObjectRows,
) -> CephBluestoreSemanticAggregate {
    CephBluestoreSemanticAggregate {
        scan: context.scan.clone(),
        super_record: context.super_record.clone(),
        collections: Vec::new(),
        objects: vec![rows.object],
        onode_shards: rows.onode_shards,
        blobs: rows.blobs,
        logical_extents: rows.logical_extents,
        physical_extents: rows.physical_extents,
        checksum_chunks: rows.checksum_chunks,
        shared_blobs: rows.shared_blobs,
        shared_blob_refs: rows.shared_blob_refs,
    }
}

pub(super) fn validate_plan_rows(
    context: &ReadContext,
    object_ordinal: u32,
    object: &CephBluestoreObjectRecord,
    aggregate: &mut CephBluestoreSemanticAggregate,
) -> DbResult<()> {
    validation::validate_object_for_read(&context.scan.inventory_id, object)?;
    if aggregate
        .checksum_chunks
        .iter()
        .any(|chunk| chunk.object_ordinal != object_ordinal)
    {
        return Err(DbError::System(
            "BlueStore checksum object ordinal is inconsistent".to_string(),
        ));
    }
    for chunk in &mut aggregate.checksum_chunks {
        chunk.object_ordinal = 0;
    }
    validation::validate_object_children_for_read(aggregate)?;
    validation::validate_device_bounds(aggregate, context.device_size)?;
    for chunk in &mut aggregate.checksum_chunks {
        chunk.object_ordinal = object_ordinal;
    }
    Ok(())
}
