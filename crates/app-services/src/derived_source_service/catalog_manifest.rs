use domain::DataSource;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{DerivedSourceError, DerivedSourceResult, MaterializedRbdSource};
use crate::derived_source_catalog::CATALOG_MATERIALIZER_VERSION;

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
    let entries = persistence_source::repositories::file_repo::FileRepo::new(connection)
        .find_by_data_source(&data_source.id)
        .map_err(|error| DerivedSourceError::InconsistentState(error.to_string()))?;
    let by_id = entries
        .iter()
        .map(|entry| (entry.id.0.clone(), entry.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    let mut entries = entries;
    entries.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.is_directory().cmp(&right.is_directory()))
            .then_with(|| left.size.cmp(&right.size))
    });
    let mut accumulator = CatalogAccumulator::default();
    for entry in entries {
        let is_directory = entry.is_directory();
        let parent_id = entry.parent_id.clone();
        let parent = parent_id.as_ref().and_then(|id| by_id.get(&id.0));
        accumulator.push(CatalogRow {
            id: entry.id.0,
            parent_id: parent_id.as_ref().map(|id| id.0.clone()),
            path: entry.path,
            name: entry.name,
            entry_type: if is_directory {
                "directory".into()
            } else {
                "file".into()
            },
            size: entry.size,
            deleted: i64::from(entry.deleted),
            hidden: i64::from(entry.hidden),
            system: i64::from(entry.system),
            created_at: entry.created_at.map(|value| value.to_rfc3339()),
            modified_at: entry.modified_at.map(|value| value.to_rfc3339()),
            accessed_at: entry.accessed_at.map(|value| value.to_rfc3339()),
            changed_at: entry.changed_at.map(|value| value.to_rfc3339()),
            parent_state: if parent_id.is_none() {
                0
            } else if parent.is_some() {
                1
            } else {
                2
            },
            parent_path: parent.map(|value| value.path.clone()),
            parent_name: parent.map(|value| value.name.clone()),
            parent_entry_type: parent.map(|value| {
                if value.is_directory() {
                    "directory".into()
                } else {
                    "file".into()
                }
            }),
        });
    }
    Ok(accumulator.into_summary(data_source))
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
    crate::derived_source_catalog::catalog_fingerprint(lineage_fingerprint)
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

fn update_u64(hasher: &mut Sha256, value: u64) {
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
    source: &crate::storage_runtime::ReadWriteSourceStorageHandle,
    lineage_fingerprint: &str,
    summary: &MaterializedRbdSource,
) -> DerivedSourceResult<()> {
    let fingerprint = catalog_fingerprint_for_source(lineage_fingerprint);
    let partitions =
        summarize_partition_records(source_partition_records(source, &summary.data_source.id.0)?);
    persist_catalog_manifest(connection, &fingerprint, summary, &partitions)
}

fn summarize_partitions(
    connection: &Connection,
    data_source_id: &str,
) -> DerivedSourceResult<PartitionSummary> {
    let records = crate::partition_query_service::PartitionQueryRepo::new(connection)
        .source_records(data_source_id)?;
    Ok(summarize_partition_records(records))
}

fn source_partition_records(
    source: &crate::storage_runtime::ReadWriteSourceStorageHandle,
    source_id: &str,
) -> DerivedSourceResult<Vec<crate::partition_query_service::PartitionRecord>> {
    let snapshot = source.snapshot();
    let mut cursor = None;
    let mut records = Vec::new();
    loop {
        let page = source
            .partition_list_by_source(
                &snapshot,
                source_id,
                cursor.as_deref(),
                persistence_source::fjall::ScanDirection::Forward,
                256,
            )
            .map_err(|error| {
                DerivedSourceError::Database(persistence_sqlite::DbError::System(error.to_string()))
            })?;
        records.extend(page.records);
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(records);
        }
    }
}

fn summarize_partition_records(
    records: Vec<crate::partition_query_service::PartitionRecord>,
) -> PartitionSummary {
    let mut hasher = Sha256::new();
    if let Some(guid) = records
        .iter()
        .find_map(|record| record.gpt_disk_guid.as_deref())
    {
        update_field(&mut hasher, b"gpt-disk-guid-v1");
        update_field(&mut hasher, guid.as_bytes());
    }
    for record in &records {
        update_field(&mut hasher, record.id.as_bytes());
        update_i64(&mut hasher, i64::from(record.partition_index));
        update_field(&mut hasher, record.name.as_bytes());
        update_field(&mut hasher, record.kind_label.as_bytes());
        update_field(&mut hasher, record.status.as_bytes());
        update_optional_text(&mut hasher, record.type_guid.as_deref());
        if let Some(uuid) = record.gpt_partition_uuid.as_deref() {
            update_field(&mut hasher, b"gpt-partition-uuid-v1");
            update_field(&mut hasher, uuid.as_bytes());
        }
        if let Some(uuid) = record.filesystem_uuid.as_deref() {
            update_field(&mut hasher, b"filesystem-uuid-v1");
            update_field(&mut hasher, uuid.as_bytes());
        }
        update_u64(&mut hasher, record.offset);
        update_u64(&mut hasher, record.length);
        for value in [
            record.filesystem.as_deref(),
            record.unlock_hint.as_deref(),
            record.lvm_vg_uuid.as_deref(),
            record.lvm_vg_name.as_deref(),
            record.lvm_lv_uuid.as_deref(),
            record.lvm_lv_name.as_deref(),
            record.lvm_pv_offsets_json.as_deref(),
            record.lvm_pv_sources_json.as_deref(),
        ] {
            update_optional_text(&mut hasher, value);
        }
    }
    PartitionSummary {
        record_count: records.len() as u64,
        digest: hex::encode(hasher.finalize()),
    }
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
