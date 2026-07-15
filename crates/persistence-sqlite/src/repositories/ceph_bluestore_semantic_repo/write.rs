mod batch;
mod children;

use std::time::Instant;

use rusqlite::{params, Connection};

use crate::connection::{DbError, DbResult};

use super::{
    CephBluestoreSemanticAggregate, CephBluestoreSemanticScanRecord, CephBluestoreSuperRecord,
};

macro_rules! timed_insert {
    ($operation:expr) => {{
        let started = Instant::now();
        $operation?;
        started.elapsed().as_millis()
    }};
}

pub(super) fn replace_for_inventory_on(
    conn: &Connection,
    aggregate: &CephBluestoreSemanticAggregate,
) -> DbResult<()> {
    let total_started = Instant::now();
    ensure_inventory_parent(conn, &aggregate.scan.inventory_id)?;
    conn.execute(
        "DELETE FROM ceph_bluestore_semantic_scans WHERE inventory_id = ?1",
        params![aggregate.scan.inventory_id],
    )?;
    let header_ms = timed_insert!({
        insert_scan(conn, &aggregate.scan)?;
        insert_super(conn, &aggregate.super_record)
    });
    let collections_ms = timed_insert!(children::insert_collections(conn, &aggregate.collections));
    let shared_blobs_ms =
        timed_insert!(children::insert_shared_blobs(conn, &aggregate.shared_blobs));
    let objects_ms = timed_insert!(children::insert_objects(conn, &aggregate.objects));
    let shards_ms = timed_insert!(children::insert_onode_shards(conn, &aggregate.onode_shards));
    let blobs_ms = timed_insert!(children::insert_blobs(conn, &aggregate.blobs));
    let checksum_ms = timed_insert!(children::insert_checksum_chunks(
        conn,
        &aggregate.scan.inventory_id,
        &aggregate.objects,
        &aggregate.checksum_chunks
    ));
    let logical_ms = timed_insert!(children::insert_logical_extents(
        conn,
        &aggregate.logical_extents
    ));
    let physical_ms = timed_insert!(children::insert_physical_extents(
        conn,
        &aggregate.physical_extents
    ));
    let shared_refs_ms = timed_insert!(children::insert_shared_blob_refs(
        conn,
        &aggregate.shared_blob_refs
    ));
    tracing::info!(
        inventory_id = aggregate.scan.inventory_id,
        object_rows = aggregate.objects.len(),
        blob_rows = aggregate.blobs.len(),
        checksum_rows = aggregate.checksum_chunks.len(),
        header_ms,
        collections_ms,
        shared_blobs_ms,
        objects_ms,
        shards_ms,
        blobs_ms,
        checksum_ms,
        logical_ms,
        physical_ms,
        shared_refs_ms,
        total_ms = total_started.elapsed().as_millis(),
        "Persisted bounded BlueStore semantic snapshot"
    );
    Ok(())
}

fn ensure_inventory_parent(conn: &Connection, inventory_id: &str) -> DbResult<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM ceph_rocksdb_manifests WHERE inventory_id = ?1
         )",
        params![inventory_id],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(DbError::System(
            "BlueStore semantic scan references an unknown RocksDB inventory".to_string(),
        ));
    }
    Ok(())
}

fn insert_scan(conn: &Connection, record: &CephBluestoreSemanticScanRecord) -> DbResult<()> {
    conn.execute(
        "INSERT INTO ceph_bluestore_semantic_scans (
            inventory_id, schema_version, decode_profile, sharding_sha256,
            latest_state_sha256, semantic_sha256,
            s_latest_count, s_decoded_count, s_deferred_count,
            c_latest_count, c_decoded_count, c_deferred_count,
            o_latest_count, o_decoded_count, o_deferred_count,
            x_latest_count, x_decoded_count, x_deferred_count,
            collection_count, object_count, blob_count, onode_shard_count,
            logical_extent_count, physical_extent_count, checksum_chunk_count,
            shared_blob_count, shared_ref_extent_count, profile_complete
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26,
            ?27, ?28
         )",
        params![
            record.inventory_id,
            record.schema_version,
            record.decode_profile,
            record.sharding_sha256,
            record.latest_state_sha256,
            record.semantic_sha256,
            record.s_latest_count,
            record.s_decoded_count,
            record.s_deferred_count,
            record.c_latest_count,
            record.c_decoded_count,
            record.c_deferred_count,
            record.o_latest_count,
            record.o_decoded_count,
            record.o_deferred_count,
            record.x_latest_count,
            record.x_decoded_count,
            record.x_deferred_count,
            record.collection_count,
            record.object_count,
            record.blob_count,
            record.onode_shard_count,
            record.logical_extent_count,
            record.physical_extent_count,
            record.checksum_chunk_count,
            record.shared_blob_count,
            record.shared_ref_extent_count,
            record.profile_complete,
        ],
    )?;
    Ok(())
}

fn insert_super(conn: &Connection, record: &CephBluestoreSuperRecord) -> DbResult<()> {
    conn.execute(
        "INSERT INTO ceph_bluestore_super (
            inventory_id, nid_max, blobid_max, min_alloc_size, ondisk_format,
            min_compat_ondisk_format, per_pool_omap, freelist_type,
            observed_count, deferred_count
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            record.inventory_id,
            record.nid_max,
            record.blobid_max,
            record.min_alloc_size,
            record.ondisk_format,
            record.min_compat_ondisk_format,
            record.per_pool_omap,
            record.freelist_type,
            record.observed_count,
            record.deferred_count,
        ],
    )?;
    Ok(())
}
