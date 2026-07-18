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
}

impl StoredCatalogManifest {
    fn from_summary(input_fingerprint: &str, summary: &MaterializedRbdSource) -> Self {
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
    partition_index: Option<u64>,
    parent_state: i64,
    parent_path: Option<String>,
    parent_name: Option<String>,
    parent_entry_type: Option<String>,
    parent_partition_index: Option<u64>,
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
        update_optional_u64(&mut self.hasher, row.partition_index);
        update_i64(&mut self.hasher, row.parent_state);
        update_optional_text(&mut self.hasher, row.parent_path.as_deref());
        update_optional_text(&mut self.hasher, row.parent_name.as_deref());
        update_optional_text(&mut self.hasher, row.parent_entry_type.as_deref());
        update_optional_u64(&mut self.hasher, row.parent_partition_index);
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
            "SELECT child.path, child.name, child.entry_type, child.size,
                    child.deleted, child.hidden, child.system,
                    child.created_at, child.modified_at, child.accessed_at,
                    child.changed_at, child.partition_index,
                    CASE
                        WHEN child.parent_id IS NULL THEN 0
                        WHEN parent.id IS NULL THEN 2
                        ELSE 1
                    END AS parent_state,
                    parent.path, parent.name, parent.entry_type,
                    parent.partition_index
             FROM file_entries AS child
             LEFT JOIN file_entries AS parent
               ON parent.id = child.parent_id
              AND parent.data_source_id = child.data_source_id
             WHERE child.data_source_id = ?1
             ORDER BY COALESCE(child.partition_index, -1),
                      child.path COLLATE BINARY,
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
        path: row.get(0)?,
        name: row.get(1)?,
        entry_type: row.get(2)?,
        size: row.get(3)?,
        deleted: row.get(4)?,
        hidden: row.get(5)?,
        system: row.get(6)?,
        created_at: row.get(7)?,
        modified_at: row.get(8)?,
        accessed_at: row.get(9)?,
        changed_at: row.get(10)?,
        partition_index: row.get(11)?,
        parent_state: row.get(12)?,
        parent_path: row.get(13)?,
        parent_name: row.get(14)?,
        parent_entry_type: row.get(15)?,
        parent_partition_index: row.get(16)?,
    })
}

pub(super) fn persist_catalog_manifest(
    connection: &Connection,
    input_fingerprint: &str,
    summary: &MaterializedRbdSource,
) -> DerivedSourceResult<()> {
    let manifest = serde_json::to_string(&StoredCatalogManifest::from_summary(
        input_fingerprint,
        summary,
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
    let Some(stored_summary) =
        load_current_source_summary(connection, lineage_fingerprint, data_source.clone())?
    else {
        return Ok(false);
    };
    let computed_summary = summarize_source_connection(connection, data_source)?;
    Ok(stored_summary.file_count == computed_summary.file_count
        && stored_summary.directory_count == computed_summary.directory_count
        && stored_summary.total_size == computed_summary.total_size
        && stored_summary.created_count == computed_summary.created_count
        && stored_summary.modified_count == computed_summary.modified_count
        && stored_summary.accessed_count == computed_summary.accessed_count
        && stored_summary.changed_count == computed_summary.changed_count
        && stored_summary.catalog_digest == computed_summary.catalog_digest)
}

pub(super) fn persist_current_source_manifest(
    connection: &Connection,
    lineage_fingerprint: &str,
    summary: &MaterializedRbdSource,
) -> DerivedSourceResult<()> {
    let fingerprint = catalog_fingerprint_for_source(lineage_fingerprint);
    persist_catalog_manifest(connection, &fingerprint, summary)
}
