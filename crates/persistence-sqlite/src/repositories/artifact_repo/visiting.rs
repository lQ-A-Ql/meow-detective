use super::{row_to_artifact, ArtifactRepo, ARTIFACT_SELECT_COLUMNS};
use crate::{DbError, DbResult};
use domain::{Artifact, ArtifactId, FileEntryId};
use persistence_fjall::{ArtifactRecord, ArtifactRecordStore, ScanDirection};
use serde::Deserialize;
use std::collections::BTreeMap;

pub(super) fn records(conn: &rusqlite::Connection) -> DbResult<Option<Vec<ArtifactRecord>>> {
    let Some(source_id) = crate::source_fjall::source_id(conn)? else {
        return Ok(None);
    };
    let Some(store) = crate::source_fjall::open_store(conn)? else {
        return Ok(None);
    };
    let repository = ArtifactRecordStore::from_source_store((*store).clone());
    let snapshot = repository.read_snapshot();
    let mut cursor = None;
    let mut records = Vec::new();
    loop {
        crate::read_operation::ensure_active()?;
        let page = repository
            .list_by_source(
                &snapshot,
                &source_id,
                cursor.as_deref(),
                ScanDirection::Forward,
                512,
            )
            .map_err(|error| DbError::System(error.to_string()))?;
        records.extend(page.records);
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(Some(records));
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
    pub fn count_by_source_object(&self, object: &str) -> DbResult<u64> {
        let mut count = 0;
        self.visit_by_family(None, |artifact| {
            if artifact
                .source_object_id
                .as_ref()
                .is_some_and(|id| id.0 == object)
            {
                count += 1;
            }
            Ok(())
        })?;
        Ok(count)
    }

    pub fn list_all_records(&self) -> DbResult<Vec<ArtifactRecord>> {
        if let Some(records) = self::records(self.conn)? {
            return Ok(records);
        }
        let mut records = Vec::new();
        self.visit_by_family(None, |artifact| {
            records.push(crate::source_fjall::artifact_record(&artifact, ""));
            Ok(())
        })?;
        Ok(records)
    }
    pub fn find_by_family_raw(&self, family: &str) -> DbResult<Vec<(String, String)>> {
        let mut records = Vec::new();
        self.visit_by_family(Some(family), |artifact| {
            let attrs = serde_json::to_string(&artifact.attrs)
                .map_err(|error| DbError::System(error.to_string()))?;
            records.push((artifact.id.0, attrs));
            Ok(())
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
        if let Some(store) = crate::source_fjall::open_store(self.conn)? {
            let source_id = crate::source_fjall::source_id(self.conn)?
                .ok_or_else(|| DbError::System("source identity is missing".into()))?;
            let repository = ArtifactRecordStore::from_source_store((*store).clone());
            let snapshot = repository.read_snapshot();
            let mut cursor = None;
            loop {
                crate::read_operation::ensure_active()?;
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
        let mut statement = self.conn.prepare(&format!(
            "SELECT {ARTIFACT_SELECT_COLUMNS} FROM artifacts
             WHERE ?1 IS NULL OR artifact_type = ?1 ORDER BY id"
        ))?;
        let rows = statement.query_map([family], row_to_artifact)?;
        for row in rows {
            crate::read_operation::ensure_active()?;
            visit(row?)?;
        }
        Ok(())
    }
}
