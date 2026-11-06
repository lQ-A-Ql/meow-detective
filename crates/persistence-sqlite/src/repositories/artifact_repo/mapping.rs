use super::{ArtifactCursorRow, ArtifactSortKey};
use domain::{Artifact, ArtifactId};

pub(super) fn row_to_artifact(row: &rusqlite::Row<'_>) -> rusqlite::Result<Artifact> {
    let attrs_text: String = row.get(9)?;
    let attrs = serde_json::from_str(&attrs_text).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(9, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let timestamp: String = row.get(10)?;
    let created_at = chrono::DateTime::parse_from_rfc3339(&timestamp)
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                10,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?
        .with_timezone(&chrono::Utc);
    Ok(Artifact {
        id: ArtifactId(row.get(0)?),
        family: row.get(1)?,
        source_object_id: row.get::<_, Option<String>>(2)?.map(domain::FileEntryId),
        extractor_id: row.get(3)?,
        extractor_version: row.get(4)?,
        confidence: row.get(5)?,
        source_attribution: row.get(6)?,
        title: row.get(7)?,
        summary: row.get(8)?,
        created_at,
        attrs,
    })
}

pub(super) fn row_to_artifact_cursor_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ArtifactCursorRow> {
    let created_at = row.get::<_, String>(10)?;
    let artifact = row_to_artifact(row)?;
    let artifact_id = artifact.id.0.clone();
    Ok(ArtifactCursorRow {
        artifact,
        sort_key: ArtifactSortKey {
            created_at,
            artifact_id,
        },
    })
}
