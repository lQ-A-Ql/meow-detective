use super::{ArtifactReferenceRecord, StorageControlRepo};
use crate::DbResult;
use rusqlite::{params, OptionalExtension};

impl StorageControlRepo<'_> {
    pub fn insert_artifact_reference(&self, record: &ArtifactReferenceRecord) -> DbResult<()> {
        self.connection.execute(
            "INSERT INTO artifact_references (
                reference_id, case_id, object_id, source_id, owner_kind, owner_id, role
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                record.reference_id,
                record.case_id,
                record.object_id,
                record.source_id,
                record.owner_kind,
                record.owner_id,
                record.role,
            ],
        )?;
        Ok(())
    }

    pub fn find_active_artifact_reference(
        &self,
        case_id: &str,
        object_id: &str,
        source_id: Option<&str>,
        owner_kind: &str,
        owner_id: &str,
        role: &str,
    ) -> DbResult<Option<ArtifactReferenceRecord>> {
        self.connection
            .query_row(
                "SELECT reference_id, case_id, object_id, source_id, owner_kind,
                        owner_id, role, released_at
                 FROM artifact_references
                 WHERE case_id = ?1 AND object_id = ?2
                   AND source_id IS ?3 AND owner_kind = ?4
                   AND owner_id = ?5 AND role = ?6
                   AND released_at IS NULL",
                params![case_id, object_id, source_id, owner_kind, owner_id, role],
                map_artifact_reference,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn release_artifact_reference(&self, case_id: &str, reference_id: &str) -> DbResult<bool> {
        let changed = self.connection.execute(
            "UPDATE artifact_references
             SET released_at = datetime('now')
             WHERE reference_id = ?1 AND case_id = ?2 AND released_at IS NULL",
            params![reference_id, case_id],
        )?;
        Ok(changed == 1)
    }

    pub fn release_artifact_owner_references(
        &self,
        case_id: &str,
        source_id: &str,
        owner_kind: &str,
        role: &str,
        owner_ids: &[String],
    ) -> DbResult<usize> {
        let mut statement = self.connection.prepare_cached(
            "UPDATE artifact_references
             SET released_at = datetime('now')
             WHERE case_id = ?1 AND source_id = ?2 AND owner_kind = ?3
               AND role = ?4 AND owner_id = ?5 AND released_at IS NULL",
        )?;
        let mut released = 0usize;
        for owner_id in owner_ids {
            released = released.saturating_add(
                statement.execute(params![case_id, source_id, owner_kind, role, owner_id])?,
            );
        }
        Ok(released)
    }
}

fn map_artifact_reference(row: &rusqlite::Row<'_>) -> rusqlite::Result<ArtifactReferenceRecord> {
    Ok(ArtifactReferenceRecord {
        reference_id: row.get(0)?,
        case_id: row.get(1)?,
        object_id: row.get(2)?,
        source_id: row.get(3)?,
        owner_kind: row.get(4)?,
        owner_id: row.get(5)?,
        role: row.get(6)?,
        released_at: row.get(7)?,
    })
}
