use domain::{Artifact, DataSourceId, EntryType, FileEntry, FileEntryId, TimelineEvent};
use persistence_fjall::{ArtifactRecord, CatalogEntryType, FileEntryRecord, TimelineRecord};

pub fn file_record(entry: &FileEntry) -> FileEntryRecord {
    FileEntryRecord {
        id: entry.id.0.clone(),
        parent_id: entry.parent_id.as_ref().map(|id| id.0.clone()),
        data_source_id: entry.data_source_id.0.clone(),
        path: entry.path.clone(),
        name: entry.name.clone(),
        entry_type: match entry.entry_type {
            EntryType::File => CatalogEntryType::File,
            EntryType::Directory => CatalogEntryType::Directory,
        },
        size: entry.size,
        ext: entry.ext.clone(),
        deleted: entry.deleted,
        hidden: entry.hidden,
        system: entry.system,
        read_only: entry.read_only,
        archive: entry.archive,
        encrypted: Some(entry.encrypted),
        created: entry.created_at,
        modified: entry.modified_at,
        accessed: entry.accessed_at,
        changed: entry.changed_at,
        hash_sha256: entry.hash_sha256.clone(),
        partition_index: None,
        unix_mode: entry.unix_mode,
    }
}

pub fn file_entry(record: FileEntryRecord) -> FileEntry {
    FileEntry {
        id: FileEntryId(record.id),
        parent_id: record.parent_id.map(FileEntryId),
        data_source_id: DataSourceId(record.data_source_id),
        path: record.path,
        name: record.name,
        entry_type: match record.entry_type {
            CatalogEntryType::File => EntryType::File,
            CatalogEntryType::Directory => EntryType::Directory,
        },
        size: record.size,
        ext: record.ext,
        deleted: record.deleted,
        hidden: record.hidden,
        system: record.system,
        encrypted: record.encrypted.unwrap_or(true),
        read_only: record.read_only,
        archive: record.archive,
        unix_mode: record.unix_mode,
        created_at: record.created,
        modified_at: record.modified,
        accessed_at: record.accessed,
        changed_at: record.changed,
        hash_sha256: record.hash_sha256,
    }
}

pub fn artifact_record(artifact: &Artifact, source_id: &str) -> ArtifactRecord {
    ArtifactRecord {
        id: artifact.id.0.clone(),
        data_source_id: source_id.to_owned(),
        artifact_type: artifact.family.clone(),
        source_object_id: artifact.source_object_id.as_ref().map(|id| id.0.clone()),
        content_sha256: None,
        attrs: serde_json::json!({
            "title": artifact.title,
            "summary": artifact.summary,
            "extractorId": artifact.extractor_id,
            "extractorVersion": artifact.extractor_version,
            "confidence": artifact.confidence,
            "sourceAttribution": artifact.source_attribution,
            "createdAt": artifact.created_at.to_rfc3339(),
            "attrs": artifact.attrs,
        }),
    }
}

pub fn timeline_record(event: &TimelineEvent, source_id: &str) -> TimelineRecord {
    TimelineRecord {
        id: event.id.0.clone(),
        data_source_id: source_id.to_owned(),
        source_object_id: event.source_object_id.clone(),
        event_type: event.event_type.clone(),
        timestamp: event.timestamp,
        title: event.title.clone(),
        description: event.description.clone(),
        parser_id: event.parser_id.clone(),
        parser_version: event.parser_version.clone(),
        confidence: event.confidence,
        source_attribution: event.source_attribution.clone(),
        attrs: event.attrs.clone(),
    }
}
