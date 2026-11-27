use crate::{DbError, DbResult};
use domain::{
    structure::{SourceRevisionRef, StructureViewAttempt},
    DataSourceId,
};
use rusqlite::{params, Connection, OptionalExtension};

use super::{datasource_repo::DataSourceRepo, source_revision_repo::SourceRevisionRepo};

pub struct StructureViewAttemptRepo<'a> {
    conn: &'a Connection,
}

impl<'a> StructureViewAttemptRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn bind(&self, case_id: &str, attempt: &StructureViewAttempt) -> DbResult<()> {
        validate(attempt)?;
        let source_case = DataSourceRepo::new(self.conn)
            .case_id(&DataSourceId(attempt.derived_source_id.clone()))?;
        if source_case.0 != case_id {
            return Err(DbError::System(
                "structure view attempt belongs to another case".into(),
            ));
        }
        let revisions = SourceRevisionRepo::new(self.conn);
        let (revision, _) = revisions.current(case_id, &attempt.derived_source_id)?;
        revisions.require_current(&SourceRevisionRef {
            case_id: case_id.into(),
            source_id: attempt.derived_source_id.clone(),
            revision: revision.revision,
        })?;
        revisions.require_current_inputs(case_id, &attempt.derived_source_id)?;
        if let Some(prior) =
            self.find_for_attempt(case_id, &attempt.derived_source_id, &attempt.attempt_id)?
        {
            return if prior == *attempt {
                Ok(())
            } else {
                Err(DbError::System("structure view attempt changed".into()))
            };
        }
        self.conn.execute(
            "INSERT INTO structure_view_attempts (
                derived_source_id, input_fingerprint, attempt_id, owner_id,
                generation, receipt
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                attempt.derived_source_id,
                attempt.input_fingerprint,
                attempt.attempt_id,
                attempt.owner_id,
                attempt.generation,
                attempt.receipt,
            ],
        )?;
        Ok(())
    }

    pub fn find(
        &self,
        case_id: &str,
        derived_source_id: &str,
    ) -> DbResult<Option<StructureViewAttempt>> {
        let source_case =
            DataSourceRepo::new(self.conn).case_id(&DataSourceId(derived_source_id.to_owned()))?;
        if source_case.0 != case_id {
            return Err(DbError::System(
                "structure view attempt belongs to another case".into(),
            ));
        }
        self.conn
            .query_row(
                "SELECT input_fingerprint, attempt_id, owner_id, generation, receipt
                 FROM structure_view_attempts WHERE derived_source_id=?1
                 ORDER BY created_at DESC, rowid DESC LIMIT 1",
                [derived_source_id],
                |row| {
                    Ok(StructureViewAttempt {
                        derived_source_id: derived_source_id.to_owned(),
                        input_fingerprint: row.get(0)?,
                        attempt_id: row.get(1)?,
                        owner_id: row.get(2)?,
                        generation: row.get(3)?,
                        receipt: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn find_for_attempt(
        &self,
        case_id: &str,
        derived_source_id: &str,
        attempt_id: &str,
    ) -> DbResult<Option<StructureViewAttempt>> {
        let source_case =
            DataSourceRepo::new(self.conn).case_id(&DataSourceId(derived_source_id.to_owned()))?;
        if source_case.0 != case_id {
            return Err(DbError::System(
                "structure view attempt belongs to another case".into(),
            ));
        }
        self.conn
            .query_row(
                "SELECT input_fingerprint, attempt_id, owner_id, generation, receipt
                 FROM structure_view_attempts
                 WHERE derived_source_id=?1 AND attempt_id=?2",
                params![derived_source_id, attempt_id],
                |row| {
                    Ok(StructureViewAttempt {
                        derived_source_id: derived_source_id.to_owned(),
                        input_fingerprint: row.get(0)?,
                        attempt_id: row.get(1)?,
                        owner_id: row.get(2)?,
                        generation: row.get(3)?,
                        receipt: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }
}

fn validate(attempt: &StructureViewAttempt) -> DbResult<()> {
    if attempt.derived_source_id.is_empty()
        || attempt.attempt_id.is_empty()
        || attempt.owner_id.is_empty()
        || attempt.generation.is_empty()
        || !valid_digest(&attempt.input_fingerprint)
        || !valid_digest(&attempt.receipt)
    {
        return Err(DbError::System(
            "structure view attempt identity is invalid".into(),
        ));
    }
    Ok(())
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
