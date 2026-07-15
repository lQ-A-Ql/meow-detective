use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};

use crate::connection::{DbError, DbResult};

const RBD_MIN_OBJECT_ORDER: u8 = 12;
const RBD_MAX_OBJECT_ORDER: u8 = 25;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephRbdLineageRecord {
    pub derived_data_source_id: String,
    pub parent_cluster_id: String,
    pub image_name: String,
    pub image_id: String,
    pub object_prefix: String,
    pub image_size: u64,
    pub object_order: u8,
    pub features: u64,
    pub stripe_unit: u64,
    pub stripe_count: u64,
    pub data_pool_id: i64,
    pub scope_identity: String,
    pub operation_features: u64,
    pub has_parent: bool,
    pub snapshot_id: Option<u64>,
    pub encrypted: bool,
    pub expected_replica_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephRbdReplicaRecord {
    pub ordinal: u32,
    pub source_data_source_id: String,
    pub inventory_id: String,
    pub osd_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephRbdLineageAggregate {
    pub lineage: CephRbdLineageRecord,
    pub replicas: Vec<CephRbdReplicaRecord>,
}

pub struct CephRbdLineageRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CephRbdLineageRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert_aggregate(&self, aggregate: &CephRbdLineageAggregate) -> DbResult<()> {
        validate_aggregate(aggregate)?;
        let transaction = self.conn.unchecked_transaction()?;
        validate_ownership(&transaction, aggregate)?;
        insert_aggregate_on(&transaction, aggregate)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn replace_aggregate(&self, aggregate: &CephRbdLineageAggregate) -> DbResult<()> {
        validate_aggregate(aggregate)?;
        let transaction = self.conn.unchecked_transaction()?;
        validate_ownership(&transaction, aggregate)?;
        transaction.execute(
            "DELETE FROM ceph_rbd_derived_lineage WHERE derived_data_source_id = ?1",
            [&aggregate.lineage.derived_data_source_id],
        )?;
        insert_aggregate_on(&transaction, aggregate)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn find_by_data_source(
        &self,
        derived_data_source_id: &str,
    ) -> DbResult<Option<CephRbdLineageAggregate>> {
        validate_identifier("derived data-source ID", derived_data_source_id)?;
        let transaction = self.conn.unchecked_transaction()?;
        let Some(stored) = transaction
            .query_row(
                "SELECT derived_data_source_id, parent_cluster_id, image_name, image_id,
                        object_prefix, image_size, object_order, features, stripe_unit,
                        stripe_count, data_pool_id, scope_identity, operation_features,
                        has_parent, snapshot_id, encrypted, expected_replica_count
                 FROM ceph_rbd_derived_lineage
                 WHERE derived_data_source_id = ?1",
                [derived_data_source_id],
                read_stored_lineage,
            )
            .optional()?
        else {
            transaction.commit()?;
            return Ok(None);
        };

        let lineage = decode_lineage(stored)?;
        let replicas = find_replicas_on(&transaction, derived_data_source_id)?;
        let aggregate = CephRbdLineageAggregate { lineage, replicas };
        validate_aggregate(&aggregate)?;
        transaction.commit()?;
        Ok(Some(aggregate))
    }

    pub fn delete(&self, derived_data_source_id: &str) -> DbResult<bool> {
        validate_identifier("derived data-source ID", derived_data_source_id)?;
        let deleted = self.conn.execute(
            "DELETE FROM ceph_rbd_derived_lineage WHERE derived_data_source_id = ?1",
            [derived_data_source_id],
        )?;
        Ok(deleted == 1)
    }
}

pub fn insert_aggregate_in_transaction(
    conn: &Connection,
    aggregate: &CephRbdLineageAggregate,
) -> DbResult<()> {
    validate_aggregate(aggregate)?;
    validate_ownership(conn, aggregate)?;
    insert_aggregate_on(conn, aggregate)
}

fn find_replicas_on(
    conn: &Connection,
    derived_data_source_id: &str,
) -> DbResult<Vec<CephRbdReplicaRecord>> {
    let mut statement = conn.prepare(
        "SELECT ordinal, source_data_source_id, inventory_id, osd_id
         FROM ceph_rbd_derived_replicas
         WHERE derived_data_source_id = ?1
         ORDER BY ordinal",
    )?;
    let rows = statement.query_map([derived_data_source_id], |row| {
        Ok(StoredReplica {
            ordinal: row.get(0)?,
            source_data_source_id: row.get(1)?,
            inventory_id: row.get(2)?,
            osd_id: row.get(3)?,
        })
    })?;
    rows.map(|row| decode_replica(row?)).collect()
}

pub fn validate_aggregate(aggregate: &CephRbdLineageAggregate) -> DbResult<()> {
    let lineage = &aggregate.lineage;
    for (label, value) in [
        (
            "derived data-source ID",
            lineage.derived_data_source_id.as_str(),
        ),
        ("parent cluster ID", lineage.parent_cluster_id.as_str()),
        ("image name", lineage.image_name.as_str()),
        ("image ID", lineage.image_id.as_str()),
        ("object prefix", lineage.object_prefix.as_str()),
        ("scope identity", lineage.scope_identity.as_str()),
    ] {
        validate_identifier(label, value)?;
    }
    validate_sqlite_u64("image size", lineage.image_size, false)?;
    if !(RBD_MIN_OBJECT_ORDER..=RBD_MAX_OBJECT_ORDER).contains(&lineage.object_order) {
        return invalid("RBD object order is outside the supported range");
    }
    validate_striping(lineage)?;
    if lineage.data_pool_id < 0 {
        return invalid("RBD data-pool ID must not be negative");
    }
    if lineage.expected_replica_count == 0 {
        return invalid("RBD expected replica count must be positive");
    }
    if aggregate.replicas.len() != lineage.expected_replica_count as usize {
        return invalid("RBD replica count does not match the expected replica count");
    }

    let mut source_ids = HashSet::new();
    let mut inventory_ids = HashSet::new();
    let mut osd_ids = HashSet::new();
    for (index, replica) in aggregate.replicas.iter().enumerate() {
        if usize::try_from(replica.ordinal).ok() != Some(index) {
            return invalid("RBD replica ordinals must start at zero and be contiguous");
        }
        validate_identifier(
            "replica source data-source ID",
            &replica.source_data_source_id,
        )?;
        validate_identifier("replica inventory ID", &replica.inventory_id)?;
        if replica.source_data_source_id == lineage.derived_data_source_id {
            return invalid("RBD lineage cannot use its derived data source as a replica");
        }
        if !source_ids.insert(replica.source_data_source_id.as_str())
            || !inventory_ids.insert(replica.inventory_id.as_str())
            || !osd_ids.insert(replica.osd_id)
        {
            return invalid("RBD replica source, inventory, and OSD identities must be unique");
        }
    }
    Ok(())
}

fn validate_ownership(conn: &Connection, aggregate: &CephRbdLineageAggregate) -> DbResult<()> {
    let lineage = &aggregate.lineage;
    let derived_matches: bool = conn.query_row(
        "SELECT COUNT(*) = 1
         FROM data_sources AS derived
         JOIN data_source_clusters AS cluster
           ON cluster.id = ?2 AND cluster.case_id = derived.case_id
         WHERE derived.id = ?1 AND derived.kind = 'ceph_rbd'",
        params![lineage.derived_data_source_id, lineage.parent_cluster_id],
        |row| row.get(0),
    )?;
    if !derived_matches {
        return invalid("RBD derived source and parent cluster must belong to the same case");
    }
    for replica in &aggregate.replicas {
        let replica_matches: bool = conn.query_row(
            "SELECT COUNT(*) = 1
             FROM data_sources AS source
             JOIN data_source_clusters AS cluster
               ON cluster.id = ?2 AND cluster.case_id = source.case_id
             WHERE source.id = ?1 AND source.cluster_id = cluster.id",
            params![replica.source_data_source_id, lineage.parent_cluster_id],
            |row| row.get(0),
        )?;
        if !replica_matches {
            return invalid("RBD replica source is not a member of the parent cluster");
        }
    }
    Ok(())
}

fn validate_striping(lineage: &CephRbdLineageRecord) -> DbResult<()> {
    validate_sqlite_u64("stripe unit", lineage.stripe_unit, true)?;
    validate_sqlite_u64("stripe count", lineage.stripe_count, true)?;
    if (lineage.stripe_unit == 0) != (lineage.stripe_count == 0) {
        return invalid("RBD stripe unit and count must both be zero or both be nonzero");
    }
    if lineage.stripe_unit == 0 {
        return Ok(());
    }
    let object_size = 1_u64 << lineage.object_order;
    if lineage.stripe_unit > object_size || !object_size.is_multiple_of(lineage.stripe_unit) {
        return invalid("RBD stripe unit must divide the object size");
    }
    Ok(())
}

fn validate_identifier(label: &str, value: &str) -> DbResult<()> {
    if value.trim().is_empty() || value.contains('\0') {
        return invalid(format!("RBD lineage has an invalid {label}"));
    }
    Ok(())
}

fn validate_sqlite_u64(label: &str, value: u64, allow_zero: bool) -> DbResult<()> {
    if (!allow_zero && value == 0) || value > i64::MAX as u64 {
        return invalid(format!("RBD {label} is outside the SQLite integer range"));
    }
    Ok(())
}

fn insert_aggregate_on(conn: &Connection, aggregate: &CephRbdLineageAggregate) -> DbResult<()> {
    let lineage = &aggregate.lineage;
    conn.execute(
        "INSERT INTO ceph_rbd_derived_lineage (
            derived_data_source_id, parent_cluster_id, image_name, image_id,
            object_prefix, image_size, object_order, features, stripe_unit,
            stripe_count, data_pool_id, scope_identity, operation_features,
            has_parent, snapshot_id, encrypted, expected_replica_count
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16, ?17
         )",
        params![
            lineage.derived_data_source_id,
            lineage.parent_cluster_id,
            lineage.image_name,
            lineage.image_id,
            lineage.object_prefix,
            lineage.image_size,
            lineage.object_order,
            encode_u64(lineage.features),
            lineage.stripe_unit,
            lineage.stripe_count,
            lineage.data_pool_id,
            lineage.scope_identity,
            encode_u64(lineage.operation_features),
            lineage.has_parent,
            lineage.snapshot_id.map(encode_u64),
            lineage.encrypted,
            lineage.expected_replica_count,
        ],
    )?;

    let mut statement = conn.prepare_cached(
        "INSERT INTO ceph_rbd_derived_replicas (
            derived_data_source_id, ordinal, source_data_source_id, inventory_id, osd_id
         ) VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for replica in &aggregate.replicas {
        statement.execute(params![
            lineage.derived_data_source_id,
            replica.ordinal,
            replica.source_data_source_id,
            replica.inventory_id,
            replica.osd_id,
        ])?;
    }
    Ok(())
}

struct StoredLineage {
    derived_data_source_id: String,
    parent_cluster_id: String,
    image_name: String,
    image_id: String,
    object_prefix: String,
    image_size: i64,
    object_order: i64,
    features: String,
    stripe_unit: i64,
    stripe_count: i64,
    data_pool_id: i64,
    scope_identity: String,
    operation_features: String,
    has_parent: bool,
    snapshot_id: Option<String>,
    encrypted: bool,
    expected_replica_count: i64,
}

fn read_stored_lineage(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredLineage> {
    Ok(StoredLineage {
        derived_data_source_id: row.get(0)?,
        parent_cluster_id: row.get(1)?,
        image_name: row.get(2)?,
        image_id: row.get(3)?,
        object_prefix: row.get(4)?,
        image_size: row.get(5)?,
        object_order: row.get(6)?,
        features: row.get(7)?,
        stripe_unit: row.get(8)?,
        stripe_count: row.get(9)?,
        data_pool_id: row.get(10)?,
        scope_identity: row.get(11)?,
        operation_features: row.get(12)?,
        has_parent: row.get(13)?,
        snapshot_id: row.get(14)?,
        encrypted: row.get(15)?,
        expected_replica_count: row.get(16)?,
    })
}

fn decode_lineage(stored: StoredLineage) -> DbResult<CephRbdLineageRecord> {
    Ok(CephRbdLineageRecord {
        derived_data_source_id: stored.derived_data_source_id,
        parent_cluster_id: stored.parent_cluster_id,
        image_name: stored.image_name,
        image_id: stored.image_id,
        object_prefix: stored.object_prefix,
        image_size: decode_nonnegative("image size", stored.image_size)?,
        object_order: decode_u8("object order", stored.object_order)?,
        features: decode_u64("features", &stored.features)?,
        stripe_unit: decode_nonnegative("stripe unit", stored.stripe_unit)?,
        stripe_count: decode_nonnegative("stripe count", stored.stripe_count)?,
        data_pool_id: stored.data_pool_id,
        scope_identity: stored.scope_identity,
        operation_features: decode_u64("operation features", &stored.operation_features)?,
        has_parent: stored.has_parent,
        snapshot_id: stored
            .snapshot_id
            .as_deref()
            .map(|value| decode_u64("snapshot ID", value))
            .transpose()?,
        encrypted: stored.encrypted,
        expected_replica_count: decode_u32(
            "expected replica count",
            stored.expected_replica_count,
        )?,
    })
}

struct StoredReplica {
    ordinal: i64,
    source_data_source_id: String,
    inventory_id: String,
    osd_id: i64,
}

fn decode_replica(stored: StoredReplica) -> DbResult<CephRbdReplicaRecord> {
    Ok(CephRbdReplicaRecord {
        ordinal: decode_u32("replica ordinal", stored.ordinal)?,
        source_data_source_id: stored.source_data_source_id,
        inventory_id: stored.inventory_id,
        osd_id: decode_u32("OSD ID", stored.osd_id)?,
    })
}

fn encode_u64(value: u64) -> String {
    format!("{value:016x}")
}

fn decode_u64(label: &str, value: &str) -> DbResult<u64> {
    if value.len() != 16
        || value
            .bytes()
            .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
    {
        return invalid(format!("stored RBD {label} is not canonical hexadecimal"));
    }
    u64::from_str_radix(value, 16)
        .map_err(|_| DbError::System(format!("stored RBD {label} is invalid")))
}

fn decode_nonnegative(label: &str, value: i64) -> DbResult<u64> {
    u64::try_from(value).map_err(|_| DbError::System(format!("stored RBD {label} is negative")))
}

fn decode_u8(label: &str, value: i64) -> DbResult<u8> {
    u8::try_from(value).map_err(|_| DbError::System(format!("stored RBD {label} is out of range")))
}

fn decode_u32(label: &str, value: i64) -> DbResult<u32> {
    u32::try_from(value).map_err(|_| DbError::System(format!("stored RBD {label} is out of range")))
}

fn invalid<T>(message: impl Into<String>) -> DbResult<T> {
    Err(DbError::System(message.into()))
}
