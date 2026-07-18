use domain::DataSource;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    DerivedSourceError, DerivedSourceResult, MaterializedRbdSource, CATALOG_MATERIALIZER_VERSION,
};

const SOURCE_META_KEY: &str = "derived.catalog.manifest";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredCatalogManifest {
    materializer_version: u32,
    input_fingerprint: String,
    record_count: u64,
    directory_count: u64,
    total_size: u64,
    created_count: u64,
    modified_count: u64,
    accessed_count: u64,
    changed_count: u64,
    catalog_digest: String,
    #[serde(default)]
    partition_count: u64,
    #[serde(default)]
    partition_digest: String,
}

impl StoredCatalogManifest {
    fn from_summary(
        input_fingerprint: &str,
        summary: &MaterializedRbdSource,
        partitions: &PartitionSummary,
    ) -> Self {
        Self {
            materializer_version: CATALOG_MATERIALIZER_VERSION,
            input_fingerprint: input_fingerprint.to_string(),
            record_count: summary.file_count,
            directory_count: summary.directory_count,
            total_size: summary.total_size,
            created_count: summary.created_count,
            modified_count: summary.modified_count,
            accessed_count: summary.accessed_count,
            changed_count: summary.changed_count,
            catalog_digest: summary.catalog_digest.clone(),
            partition_count: partitions.record_count,
            partition_digest: partitions.digest.clone(),
        }
    }

    fn into_summary(self, data_source: DataSource) -> MaterializedRbdSource {
        MaterializedRbdSource {
            data_source,
            file_count: self.record_count,
            directory_count: self.directory_count,
            total_size: self.total_size,
            created_count: self.created_count,
            modified_count: self.modified_count,
            accessed_count: self.accessed_count,
            changed_count: self.changed_count,
            catalog_digest: self.catalog_digest,
        }
    }
}

struct CatalogRow {
    id: String,
    parent_id: Option<String>,
    path: String,
    name: String,
    entry_type: String,
    size: Option<u64>,
    deleted: i64,
    hidden: i64,
    system: i64,
    created_at: Option<String>,
    modified_at: Option<String>,
    accessed_at: Option<String>,
    changed_at: Option<String>,
    parent_state: i64,
    parent_path: Option<String>,
    parent_name: Option<String>,
    parent_entry_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PartitionSummary {
    record_count: u64,
    digest: String,
}

struct PartitionRow {
    id: String,
    partition_index: i64,
    name: String,
    kind_label: String,
    status: String,
    type_guid: Option<String>,
    offset: i64,
    length: i64,
    filesystem: Option<String>,
    unlock_hint: Option<String>,
    lvm_vg_uuid: Option<String>,
    lvm_vg_name: Option<String>,
    lvm_lv_uuid: Option<String>,
    lvm_lv_name: Option<String>,
    lvm_pv_offsets_json: Option<String>,
    lvm_pv_sources_json: Option<String>,
}

#[derive(Default)]
struct CatalogAccumulator {
    hasher: Sha256,
    record_count: u64,
    directory_count: u64,
    total_size: u64,
    created_count: u64,
    modified_count: u64,
    accessed_count: u64,
    changed_count: u64,
}

impl CatalogAccumulator {
    fn push(&mut self, row: CatalogRow) {
        self.record_count += 1;
        if row.entry_type == "directory" {
            self.directory_count += 1;
        } else {
            self.total_size = self.total_size.saturating_add(row.size.unwrap_or(0));
        }
        self.created_count += u64::from(row.created_at.is_some());
        self.modified_count += u64::from(row.modified_at.is_some());
        self.accessed_count += u64::from(row.accessed_at.is_some());
        self.changed_count += u64::from(row.changed_at.is_some());

        update_field(&mut self.hasher, row.id.as_bytes());
        update_optional_text(&mut self.hasher, row.parent_id.as_deref());
        update_field(&mut self.hasher, row.path.as_bytes());
        update_field(&mut self.hasher, row.name.as_bytes());
        update_field(&mut self.hasher, row.entry_type.as_bytes());
        update_optional_u64(&mut self.hasher, row.size);
        update_i64(&mut self.hasher, row.deleted);
        update_i64(&mut self.hasher, row.hidden);
        update_i64(&mut self.hasher, row.system);
        update_optional_text(&mut self.hasher, row.created_at.as_deref());
        update_optional_text(&mut self.hasher, row.modified_at.as_deref());
        update_optional_text(&mut self.hasher, row.accessed_at.as_deref());
        update_optional_text(&mut self.hasher, row.changed_at.as_deref());
        // partition_index is a derived routing hint. Keep the historical null
        // marker in the semantic digest so materializing the hint cannot make
        // an otherwise identical evidence catalog stale.
        update_optional_u64(&mut self.hasher, None);
        update_i64(&mut self.hasher, row.parent_state);
        update_optional_text(&mut self.hasher, row.parent_path.as_deref());
        update_optional_text(&mut self.hasher, row.parent_name.as_deref());
        update_optional_text(&mut self.hasher, row.parent_entry_type.as_deref());
        update_optional_u64(&mut self.hasher, None);
    }

    fn into_summary(self, data_source: DataSource) -> MaterializedRbdSource {
        MaterializedRbdSource {
            data_source,
            file_count: self.record_count,
            directory_count: self.directory_count,
            total_size: self.total_size,
            created_count: self.created_count,
            modified_count: self.modified_count,
            accessed_count: self.accessed_count,
            changed_count: self.changed_count,
            catalog_digest: hex::encode(self.hasher.finalize()),
        }
    }
}

pub(super) fn summarize_source_connection(
    connection: &Connection,
    data_source: DataSource,
) -> DerivedSourceResult<MaterializedRbdSource> {
    let mut statement = connection
        .prepare(
            "SELECT child.id, child.parent_id, child.path, child.name,
                    child.entry_type, child.size,
                    child.deleted, child.hidden, child.system,
                    child.created_at, child.modified_at, child.accessed_at,
                    child.changed_at,
                    CASE
                        WHEN child.parent_id IS NULL THEN 0
                        WHEN parent.id IS NULL THEN 2
                        ELSE 1
                    END AS parent_state,
                    parent.path, parent.name, parent.entry_type
             FROM file_entries AS child
             LEFT JOIN file_entries AS parent
               ON parent.id = child.parent_id
              AND parent.data_source_id = child.data_source_id
             WHERE child.data_source_id = ?1
             ORDER BY child.path COLLATE BINARY,
                      child.name COLLATE BINARY,
                      child.entry_type COLLATE BINARY,
                      COALESCE(child.size, -1)",
        )
        .map_err(persistence_sqlite::DbError::from)?;
    let mut rows = statement
        .query([&data_source.id.0])
        .map_err(persistence_sqlite::DbError::from)?;
    let mut accumulator = CatalogAccumulator::default();
    while let Some(row) = rows.next().map_err(persistence_sqlite::DbError::from)? {
        accumulator.push(read_catalog_row(row)?);
    }
    Ok(accumulator.into_summary(data_source))
}

fn read_catalog_row(row: &rusqlite::Row<'_>) -> Result<CatalogRow, persistence_sqlite::DbError> {
    Ok(CatalogRow {
        id: row.get(0)?,
        parent_id: row.get(1)?,
        path: row.get(2)?,
        name: row.get(3)?,
        entry_type: row.get(4)?,
        size: row.get(5)?,
        deleted: row.get(6)?,
        hidden: row.get(7)?,
        system: row.get(8)?,
        created_at: row.get(9)?,
        modified_at: row.get(10)?,
        accessed_at: row.get(11)?,
        changed_at: row.get(12)?,
        parent_state: row.get(13)?,
        parent_path: row.get(14)?,
        parent_name: row.get(15)?,
        parent_entry_type: row.get(16)?,
    })
}

fn persist_catalog_manifest(
    connection: &Connection,
    input_fingerprint: &str,
    summary: &MaterializedRbdSource,
    partitions: &PartitionSummary,
) -> DerivedSourceResult<()> {
    let manifest = serde_json::to_string(&StoredCatalogManifest::from_summary(
        input_fingerprint,
        summary,
        partitions,
    ))
    .map_err(|error| {
        DerivedSourceError::Database(persistence_sqlite::DbError::System(format!(
            "Serialize derived Catalog manifest: {error}"
        )))
    })?;
    connection
        .execute(
            "INSERT INTO source_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![SOURCE_META_KEY, manifest],
        )
        .map_err(persistence_sqlite::DbError::from)?;
    Ok(())
}

fn load_catalog_manifest(
    connection: &Connection,
) -> DerivedSourceResult<Option<StoredCatalogManifest>> {
    let stored = connection
        .query_row(
            "SELECT value FROM source_meta WHERE key = ?1",
            [SOURCE_META_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(persistence_sqlite::DbError::from)?;
    let Some(stored) = stored else {
        return Ok(None);
    };
    let stored: StoredCatalogManifest = serde_json::from_str(&stored).map_err(|error| {
        DerivedSourceError::Database(persistence_sqlite::DbError::System(format!(
            "Decode derived Catalog manifest: {error}"
        )))
    })?;
    Ok(Some(stored))
}

pub(super) fn load_current_source_summary(
    connection: &Connection,
    lineage_fingerprint: &str,
    data_source: DataSource,
) -> DerivedSourceResult<Option<MaterializedRbdSource>> {
    let expected_fingerprint = catalog_fingerprint_for_source(lineage_fingerprint);
    let Some(stored) = load_catalog_manifest(connection)? else {
        return Ok(None);
    };
    if stored.materializer_version != CATALOG_MATERIALIZER_VERSION
        || stored.input_fingerprint != expected_fingerprint
    {
        return Ok(None);
    }
    Ok(Some(stored.into_summary(data_source)))
}

pub(super) fn catalog_fingerprint_for_source(lineage_fingerprint: &str) -> String {
    super::super::derived_finalizer::phase_input_fingerprint_for_catalog(lineage_fingerprint)
}

pub(super) fn load_catalog_fingerprint(
    connection: &Connection,
) -> DerivedSourceResult<Option<String>> {
    Ok(load_catalog_manifest(connection)?
        .filter(|stored| stored.materializer_version == CATALOG_MATERIALIZER_VERSION)
        .map(|stored| stored.input_fingerprint))
}

fn update_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}

fn update_optional_text(hasher: &mut Sha256, value: Option<&str>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            update_field(hasher, value.as_bytes());
        }
        None => hasher.update([0]),
    }
}

fn update_optional_u64(hasher: &mut Sha256, value: Option<u64>) {
    match value {
        Some(value) => {
            hasher.update([1]);
            hasher.update(value.to_le_bytes());
        }
        None => hasher.update([0]),
    }
}

fn update_i64(hasher: &mut Sha256, value: i64) {
    hasher.update(value.to_le_bytes());
}

pub(super) fn verify_current_source_manifest_deep(
    connection: &Connection,
    lineage_fingerprint: &str,
    data_source: DataSource,
) -> DerivedSourceResult<bool> {
    let expected_fingerprint = catalog_fingerprint_for_source(lineage_fingerprint);
    let Some(stored) = load_catalog_manifest(connection)? else {
        return Ok(false);
    };
    if stored.materializer_version != CATALOG_MATERIALIZER_VERSION
        || stored.input_fingerprint != expected_fingerprint
    {
        return Ok(false);
    }
    let computed_summary = summarize_source_connection(connection, data_source.clone())?;
    let computed_partitions = summarize_partitions(connection, &data_source.id.0)?;
    Ok(source_identity_matches(connection, &data_source)?
        && stored.record_count == computed_summary.file_count
        && stored.directory_count == computed_summary.directory_count
        && stored.total_size == computed_summary.total_size
        && stored.created_count == computed_summary.created_count
        && stored.modified_count == computed_summary.modified_count
        && stored.accessed_count == computed_summary.accessed_count
        && stored.changed_count == computed_summary.changed_count
        && stored.catalog_digest == computed_summary.catalog_digest
        && stored.partition_count == computed_partitions.record_count
        && stored.partition_digest == computed_partitions.digest)
}

pub(super) fn persist_current_source_manifest(
    connection: &Connection,
    lineage_fingerprint: &str,
    summary: &MaterializedRbdSource,
) -> DerivedSourceResult<()> {
    let fingerprint = catalog_fingerprint_for_source(lineage_fingerprint);
    let partitions = summarize_partitions(connection, &summary.data_source.id.0)?;
    persist_catalog_manifest(connection, &fingerprint, summary, &partitions)
}

fn summarize_partitions(
    connection: &Connection,
    data_source_id: &str,
) -> DerivedSourceResult<PartitionSummary> {
    let mut statement = connection
        .prepare(
            "SELECT id, partition_index, name, kind_label, status, type_guid,
                    offset, length, filesystem, unlock_hint, lvm_vg_uuid,
                    lvm_vg_name, lvm_lv_uuid, lvm_lv_name,
                    lvm_pv_offsets_json, lvm_pv_sources_json
             FROM data_source_partitions
             WHERE data_source_id = ?1
             ORDER BY partition_index, id COLLATE BINARY",
        )
        .map_err(persistence_sqlite::DbError::from)?;
    let rows = statement
        .query_map([data_source_id], read_partition_row)
        .map_err(persistence_sqlite::DbError::from)?;
    let mut hasher = Sha256::new();
    let mut record_count = 0u64;
    for row in rows {
        let row = row.map_err(persistence_sqlite::DbError::from)?;
        record_count += 1;
        update_field(&mut hasher, row.id.as_bytes());
        update_i64(&mut hasher, row.partition_index);
        update_field(&mut hasher, row.name.as_bytes());
        update_field(&mut hasher, row.kind_label.as_bytes());
        update_field(&mut hasher, row.status.as_bytes());
        update_optional_text(&mut hasher, row.type_guid.as_deref());
        update_i64(&mut hasher, row.offset);
        update_i64(&mut hasher, row.length);
        for value in [
            row.filesystem,
            row.unlock_hint,
            row.lvm_vg_uuid,
            row.lvm_vg_name,
            row.lvm_lv_uuid,
            row.lvm_lv_name,
            row.lvm_pv_offsets_json,
            row.lvm_pv_sources_json,
        ] {
            update_optional_text(&mut hasher, value.as_deref());
        }
    }
    Ok(PartitionSummary {
        record_count,
        digest: hex::encode(hasher.finalize()),
    })
}

fn read_partition_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PartitionRow> {
    Ok(PartitionRow {
        id: row.get(0)?,
        partition_index: row.get(1)?,
        name: row.get(2)?,
        kind_label: row.get(3)?,
        status: row.get(4)?,
        type_guid: row.get(5)?,
        offset: row.get(6)?,
        length: row.get(7)?,
        filesystem: row.get(8)?,
        unlock_hint: row.get(9)?,
        lvm_vg_uuid: row.get(10)?,
        lvm_vg_name: row.get(11)?,
        lvm_lv_uuid: row.get(12)?,
        lvm_lv_name: row.get(13)?,
        lvm_pv_offsets_json: row.get(14)?,
        lvm_pv_sources_json: row.get(15)?,
    })
}

fn source_identity_matches(
    connection: &Connection,
    data_source: &DataSource,
) -> DerivedSourceResult<bool> {
    let matches = connection
        .query_row(
            "SELECT COUNT(*)
             FROM data_sources
             WHERE id = ?1 AND name = ?2 AND kind = 'ceph_rbd' AND source_path = ?3",
            params![
                data_source.id.0,
                data_source.name,
                data_source.source_path.display().to_string()
            ],
            |row| row.get::<_, u64>(0),
        )
        .map_err(persistence_sqlite::DbError::from)?;
    Ok(matches == 1)
}
