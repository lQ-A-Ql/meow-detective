use std::collections::BTreeMap;

use persistence_fjall::PersistMode;
use rusqlite::Connection;
use serde_json::{Map, Value};

use crate::storage_runtime::ReadWriteSourceStorageHandle;
use transport::CommandError;

const BATCH_SIZE: usize = 512;

pub(super) fn mirror_namespace_locators(
    connection: &Connection,
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
) -> Result<(), CommandError> {
    let payloads = collect_namespace_locator_payloads(connection, source_id)?;
    let mut batch = Vec::with_capacity(BATCH_SIZE);
    for (entry_id, payload) in payloads {
        let payload = Value::Object(payload);
        batch.push(persistence_fjall::ParserRecord {
            id: format!("namespace-locator:{entry_id}"),
            data_source_id: source_id.to_owned(),
            family: "cephfs".to_owned(),
            kind: "ceph_fs_namespace_locator".to_owned(),
            status: "ready".to_owned(),
            summary_sha256: infrastructure::hashing::sha256_bytes(payload.to_string().as_bytes()),
            payload,
        });
        if batch.len() == BATCH_SIZE {
            source
                .parser_upsert_batch(&batch, PersistMode::Buffer)
                .map_err(|error| CommandError::internal(error.to_string()))?;
            batch.clear();
        }
    }
    if !batch.is_empty() {
        source
            .parser_upsert_batch(&batch, PersistMode::Buffer)
            .map_err(|error| CommandError::internal(error.to_string()))?;
    }
    Ok(())
}

fn collect_namespace_locator_payloads(
    connection: &Connection,
    source_id: &str,
) -> Result<BTreeMap<String, Map<String, Value>>, CommandError> {
    let mut statement = match connection.prepare(
        "SELECT manifest.filesystem_identity, manifest.filesystem_id, manifest.fsmap_epoch,
                manifest.projection_sha256, manifest.schema_version, manifest.decoder_profile,
                dentry.entry_id, dentry.child_inode, dentry.entry_kind, inode.size,
                layout.stripe_unit, layout.stripe_count, layout.object_size, layout.pool_id,
                layout.pool_namespace, layout.inline_data, sparse.offset, sparse.length,
                sparse.evidence_sha256, sparse.proof_sha256
         FROM ceph_fs_namespace_manifests AS manifest
         JOIN ceph_fs_dentries AS dentry
           ON dentry.filesystem_identity = manifest.filesystem_identity
          AND dentry.data_source_id = manifest.data_source_id
         JOIN ceph_fs_inodes AS inode
           ON inode.filesystem_identity = dentry.filesystem_identity
          AND inode.data_source_id = dentry.data_source_id
          AND inode.inode = dentry.child_inode
         JOIN ceph_fs_file_layouts AS layout
           ON layout.filesystem_identity = inode.filesystem_identity
          AND layout.data_source_id = inode.data_source_id
          AND layout.inode = inode.inode
         LEFT JOIN ceph_fs_sparse_extents AS sparse
           ON sparse.filesystem_identity = layout.filesystem_identity
          AND sparse.data_source_id = layout.data_source_id
          AND sparse.inode = layout.inode
         WHERE manifest.data_source_id = ?1
           AND manifest.completeness = 'closed'
           AND manifest.published = 1
         ORDER BY dentry.entry_id ASC, sparse.offset ASC",
    ) {
        Ok(statement) => statement,
        Err(error) if error.to_string().contains("no such table") => return Ok(BTreeMap::new()),
        Err(error) => return Err(CommandError::internal(error.to_string())),
    };
    let mut payloads = BTreeMap::<String, Map<String, Value>>::new();
    let rows = statement
        .query_map([source_id], |row| {
            let entry_id: String = row.get(6)?;
            let payload = serde_json::json!({
                "filesystem_identity": row.get::<_, String>(0)?,
                "filesystem_id": row.get::<_, i64>(1)?,
                "fsmap_epoch": row.get::<_, i64>(2)?,
                "projection_sha256": row.get::<_, String>(3)?,
                "schema_version": row.get::<_, i64>(4)?,
                "decoder_profile": row.get::<_, String>(5)?,
                "entry_id": entry_id,
                "inode": row.get::<_, i64>(7)?,
                "entry_kind": row.get::<_, String>(8)?,
                "size": row.get::<_, i64>(9)?,
                "stripe_unit": row.get::<_, i64>(10)?,
                "stripe_count": row.get::<_, i64>(11)?,
                "object_size": row.get::<_, i64>(12)?,
                "pool_id": row.get::<_, i64>(13)?,
                "pool_namespace": row.get::<_, String>(14)?,
                "inline_data_hex": row.get::<_, Option<Vec<u8>>>(15)?.map(hex::encode),
                "sparse_extents": Vec::<Value>::new(),
            });
            let sparse = match row.get::<_, Option<i64>>(16)? {
                Some(offset) => Some(serde_json::json!({
                    "offset": offset,
                    "length": row.get::<_, i64>(17)?,
                    "evidence_sha256": row.get::<_, String>(18)?,
                    "proof_sha256": row.get::<_, String>(19)?,
                })),
                None => None,
            };
            Ok((entry_id, payload, sparse))
        })
        .map_err(|error| CommandError::internal(error.to_string()))?;
    for row in rows {
        let (entry_id, payload, sparse) =
            row.map_err(|error| CommandError::internal(error.to_string()))?;
        let payload = payload
            .as_object()
            .cloned()
            .ok_or_else(|| CommandError::internal("CephFS locator payload is not an object"))?;
        let target = payloads.entry(entry_id).or_insert_with(|| payload);
        if let Some(sparse) = sparse {
            target
                .get_mut("sparse_extents")
                .and_then(Value::as_array_mut)
                .ok_or_else(|| CommandError::internal("CephFS sparse extent field is invalid"))?
                .push(sparse);
        }
    }
    Ok(payloads)
}
