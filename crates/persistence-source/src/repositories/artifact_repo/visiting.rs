use super::{push_bounded, ArtifactRepo, MAX_ARTIFACT_QUERY_ROWS};
use domain::{Artifact, ArtifactId, FileEntryId};
use persistence_fjall::{ArtifactRecord, ArtifactRecordStore, ScanDirection};
use persistence_sqlite::{DbError, DbResult};
use serde::Deserialize;
use std::collections::BTreeMap;

pub(super) fn records(conn: &rusqlite::Connection) -> DbResult<Vec<ArtifactRecord>> {
    let source_id = crate::detail::source_id(conn)?
        .ok_or_else(|| DbError::System("source identity is missing".into()))?;
    let store = crate::detail::require_source_store(conn)?;
    let repository = ArtifactRecordStore::from_source_store((*store).clone());
    let snapshot = repository.read_snapshot();
    let mut cursor = None;
    let mut records = Vec::new();
    loop {
        persistence_sqlite::read_operation::ensure_active()?;
        let page = repository
            .list_by_source(
                &snapshot,
                &source_id,
                cursor.as_deref(),
                ScanDirection::Forward,
                512,
            )
            .map_err(|error| DbError::System(error.to_string()))?;
        for record in page.records {
            push_bounded(&mut records, record)?;
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(records);
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactPayload {
    title: String,
    summary: String,
    extractor_id: Option<String>,
    extractor_version: Option<String>,
    confidence: Option<f32>,
    source_attribution: Option<String>,
    created_at: chrono::DateTime<chrono::Utc>,
    attrs: BTreeMap<String, serde_json::Value>,
}

pub(crate) fn decode(record: ArtifactRecord) -> DbResult<Artifact> {
    let payload: ArtifactPayload = serde_json::from_value(record.attrs)
        .map_err(|error| DbError::System(format!("invalid artifact {}: {error}", record.id)))?;
    Ok(Artifact {
        id: ArtifactId(record.id),
        family: record.artifact_type,
        source_object_id: record.source_object_id.map(FileEntryId),
        title: payload.title,
        summary: payload.summary,
        extractor_id: payload.extractor_id,
        extractor_version: payload.extractor_version,
        confidence: payload.confidence,
        source_attribution: payload.source_attribution,
        created_at: payload.created_at,
        attrs: payload.attrs,
    })
}

impl ArtifactRepo<'_> {
    pub fn visit_by_source_object(
        &self,
        source_object_id: &str,
        mut visit: impl FnMut(Artifact) -> DbResult<()>,
    ) -> DbResult<()> {
        if let Some(store) = crate::detail::open_store(self.conn)? {
            let source_id = crate::detail::source_id(self.conn)?
                .ok_or_else(|| DbError::System("source identity is missing".into()))?;
            let repository = ArtifactRecordStore::from_source_store((*store).clone());
            let snapshot = repository.read_snapshot();
            let mut cursor = None;
            loop {
                persistence_sqlite::read_operation::ensure_active()?;
                let page = repository
                    .list_by_source_object(
                        &snapshot,
                        &source_id,
                        source_object_id,
                        cursor.as_deref(),
                        ScanDirection::Forward,
                        256,
                    )
                    .map_err(|error| DbError::System(error.to_string()))?;
                for record in page.records {
                    visit(decode(record)?)?;
                }
                cursor = page.next_cursor;
                if cursor.is_none() {
                    return Ok(());
                }
            }
        }
        Err(DbError::System("source detail storage is missing".into()))
    }

    pub fn count_by_source_object(&self, object: &str) -> DbResult<u64> {
        let mut count = 0u64;
        self.visit_by_source_object(object, |_| {
            count = count.saturating_add(1);
            Ok(())
        })?;
        Ok(count)
    }

    pub fn list_all_records(&self) -> DbResult<Vec<ArtifactRecord>> {
        self::records(self.conn)
    }

    pub fn visit_all_records(
        &self,
        mut visit: impl FnMut(ArtifactRecord) -> DbResult<()>,
    ) -> DbResult<()> {
        let source_id = crate::detail::source_id(self.conn)?
            .ok_or_else(|| DbError::System("source identity is missing".into()))?;
        let store = crate::detail::require_source_store(self.conn)?;
        let repository = ArtifactRecordStore::from_source_store((*store).clone());
        let snapshot = repository.read_snapshot();
        let mut cursor = None;
        loop {
            persistence_sqlite::read_operation::ensure_active()?;
            let page = repository
                .list_by_source(
                    &snapshot,
                    &source_id,
                    cursor.as_deref(),
                    ScanDirection::Forward,
                    512,
                )
                .map_err(|error| DbError::System(error.to_string()))?;
            for record in page.records {
                visit(record)?;
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(());
            }
        }
    }
    pub fn find_by_family_raw(&self, family: &str) -> DbResult<Vec<(String, String)>> {
        let mut records = Vec::new();
        self.visit_by_family(Some(family), |artifact| {
            let attrs = serde_json::to_string(&artifact.attrs)
                .map_err(|error| DbError::System(error.to_string()))?;
            push_bounded(&mut records, (artifact.id.0, attrs))
        })?;
        Ok(records)
    }

    pub fn find_extractor_versions(&self, parser: &str) -> DbResult<Vec<(String, Option<String>)>> {
        let mut versions = Vec::new();
        self.visit_by_family(None, |artifact| {
            if versions.is_empty() && artifact.extractor_version.is_some() {
                if let Some(id) = artifact
                    .extractor_id
                    .filter(|id| id.eq_ignore_ascii_case(parser))
                {
                    if versions.len() >= MAX_ARTIFACT_QUERY_ROWS {
                        return Err(DbError::System(
                            "artifact extractor query exceeds the bounded row limit".into(),
                        ));
                    }
                    versions.push((id, artifact.extractor_version));
                }
            }
            Ok(())
        })?;
        Ok(versions)
    }
    pub fn visit_by_family(
        &self,
        family: Option<&str>,
        mut visit: impl FnMut(Artifact) -> DbResult<()>,
    ) -> DbResult<()> {
        if let Some(store) = crate::detail::open_store(self.conn)? {
            let source_id = crate::detail::source_id(self.conn)?
                .ok_or_else(|| DbError::System("source identity is missing".into()))?;
            let repository = ArtifactRecordStore::from_source_store((*store).clone());
            let snapshot = repository.read_snapshot();
            let mut cursor = None;
            loop {
                persistence_sqlite::read_operation::ensure_active()?;
                let page = match family {
                    Some(family) => repository.list_by_type(
                        &snapshot,
                        &source_id,
                        family,
                        cursor.as_deref(),
                        ScanDirection::Forward,
                        256,
                    ),
                    None => repository.list_by_source(
                        &snapshot,
                        &source_id,
                        cursor.as_deref(),
                        ScanDirection::Forward,
                        256,
                    ),
                }
                .map_err(|error| DbError::System(error.to_string()))?;
                for record in page.records {
                    visit(decode(record)?)?;
                }
                cursor = page.next_cursor;
                if cursor.is_none() {
                    return Ok(());
                }
            }
        }
        Err(DbError::System("source detail storage is missing".into()))
    }
}
