use rusqlite::{params, Connection};

use crate::connection::{DbError, DbResult};

use super::{binding, query};

pub const MAX_OBJECT_INVENTORY_PAGE_SIZE: u32 = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluestoreObjectPageCursor(String);

impl CephBluestoreObjectPageCursor {
    pub fn new(value: impl Into<String>) -> DbResult<Self> {
        let value = value.into();
        if !canonical_sha256(&value) {
            return Err(DbError::System(
                "BlueStore object page cursor must be canonical lowercase SHA-256".to_string(),
            ));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluestoreObjectInventoryEntry {
    pub inventory_id: String,
    pub object_identity_sha256: String,
    pub decoded_pool: i64,
    pub object_namespace: Vec<u8>,
    pub object_key: Option<Vec<u8>>,
    pub object_name: Vec<u8>,
    pub snap_hex: String,
    pub generation_hex: String,
    pub size: u64,
    pub attribute_count: u64,
    pub attributes_sha256: String,
    pub decode_status: String,
    pub deferred_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephBluestoreObjectInventoryPage {
    pub inventory_id: String,
    pub semantic_sha256: String,
    pub objects: Vec<CephBluestoreObjectInventoryEntry>,
    pub next_cursor: Option<CephBluestoreObjectPageCursor>,
}

pub(super) fn list_objects_by_pool_after(
    conn: &Connection,
    inventory_id: &str,
    pool_id: i64,
    after: Option<&CephBluestoreObjectPageCursor>,
    limit: u32,
) -> DbResult<CephBluestoreObjectInventoryPage> {
    validate_request(inventory_id, pool_id, limit)?;
    let scan = query::find_scan(conn, inventory_id)?
        .ok_or_else(|| DbError::System("BlueStore object catalog is unavailable".to_string()))?;
    binding::validate_persisted_binding_for_read(conn, &scan)?;
    let fetch_limit = i64::from(limit) + 1;
    let mut objects = query_page(conn, inventory_id, pool_id, after, fetch_limit)?;
    let has_more = objects.len() > limit as usize;
    if has_more {
        objects.pop();
    }
    let next_cursor = has_more
        .then(|| objects.last())
        .flatten()
        .map(|object| CephBluestoreObjectPageCursor(object.object_identity_sha256.clone()));
    Ok(CephBluestoreObjectInventoryPage {
        inventory_id: inventory_id.to_string(),
        semantic_sha256: scan.semantic_sha256,
        objects,
        next_cursor,
    })
}

fn query_page(
    conn: &Connection,
    inventory_id: &str,
    pool_id: i64,
    after: Option<&CephBluestoreObjectPageCursor>,
    fetch_limit: i64,
) -> DbResult<Vec<CephBluestoreObjectInventoryEntry>> {
    const SELECT: &str = "SELECT inventory_id, object_identity_sha256, decoded_pool,
        object_namespace, object_key, object_name, snap_hex, generation_hex, size,
        attribute_count, attributes_sha256, decode_status, deferred_reason
        FROM ceph_bluestore_objects";
    let rows = match after {
        Some(after) => {
            let sql = format!(
                "{SELECT} WHERE inventory_id = ?1 AND decoded_pool = ?2
                 AND object_identity_sha256 > ?3
                 ORDER BY object_identity_sha256 LIMIT ?4"
            );
            let mut statement = conn.prepare_cached(&sql)?;
            let rows = statement.query_map(
                params![inventory_id, pool_id, after.as_str(), fetch_limit],
                map_entry,
            )?;
            rows.collect::<Result<Vec<_>, _>>()?
        }
        None => {
            let sql = format!(
                "{SELECT} WHERE inventory_id = ?1 AND decoded_pool = ?2
                 ORDER BY object_identity_sha256 LIMIT ?3"
            );
            let mut statement = conn.prepare_cached(&sql)?;
            let rows =
                statement.query_map(params![inventory_id, pool_id, fetch_limit], map_entry)?;
            rows.collect::<Result<Vec<_>, _>>()?
        }
    };
    Ok(rows)
}

fn map_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<CephBluestoreObjectInventoryEntry> {
    Ok(CephBluestoreObjectInventoryEntry {
        inventory_id: row.get(0)?,
        object_identity_sha256: row.get(1)?,
        decoded_pool: row.get(2)?,
        object_namespace: row.get(3)?,
        object_key: row.get(4)?,
        object_name: row.get(5)?,
        snap_hex: row.get(6)?,
        generation_hex: row.get(7)?,
        size: row.get(8)?,
        attribute_count: row.get(9)?,
        attributes_sha256: row.get(10)?,
        decode_status: row.get(11)?,
        deferred_reason: row.get(12)?,
    })
}

fn validate_request(inventory_id: &str, pool_id: i64, limit: u32) -> DbResult<()> {
    if inventory_id.trim().is_empty() || inventory_id.contains('\0') {
        return Err(DbError::System(
            "BlueStore inventory identity is invalid".to_string(),
        ));
    }
    if pool_id < 0 {
        return Err(DbError::System(
            "BlueStore pool identity cannot be negative".to_string(),
        ));
    }
    if !(1..=MAX_OBJECT_INVENTORY_PAGE_SIZE).contains(&limit) {
        return Err(DbError::System(format!(
            "BlueStore object page size must be between 1 and {MAX_OBJECT_INVENTORY_PAGE_SIZE}"
        )));
    }
    Ok(())
}

fn canonical_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
