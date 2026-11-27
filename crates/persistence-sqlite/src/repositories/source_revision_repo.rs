use crate::{DbError, DbResult};
use domain::structure::{NativeObservation, SourceRevisionContent, SourceRevisionRef};
use rusqlite::{params, Connection, OptionalExtension};

pub struct SourceRevisionRepo<'a> {
    conn: &'a Connection,
}

impl<'a> SourceRevisionRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn bind(
        &self,
        case_id: &str,
        source_id: &str,
        content: &SourceRevisionContent,
        observations: &[NativeObservation],
        allow_new_revision: bool,
    ) -> DbResult<SourceRevisionRef> {
        validate(content, observations)?;
        let content = serde_json::to_string(content).map_err(json_error)?;
        let observations = serde_json::to_string(observations).map_err(json_error)?;
        let transaction = self
            .conn
            .is_autocommit()
            .then(|| self.conn.unchecked_transaction())
            .transpose()?;
        require_case(self.conn, case_id, source_id)?;
        let prior: Option<(u64, String, String)> = self
            .conn
            .query_row(
                "SELECT revision, content_json, observations_json FROM source_revisions
             WHERE source_id = ?1 ORDER BY revision DESC LIMIT 1",
                [source_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let revision = match prior {
            Some((revision, known_content, known_observations))
                if known_content == content && known_observations == observations =>
            {
                revision
            }
            Some(_) if !allow_new_revision => {
                return Err(DbError::System("source revision changed".into()))
            }
            prior => {
                let revision = prior
                    .map_or(0, |entry| entry.0)
                    .checked_add(1)
                    .filter(|value| *value <= i64::MAX as u64)
                    .ok_or_else(|| DbError::System("source revision overflow".into()))?;
                self.conn.execute(
                    "INSERT INTO source_revisions (source_id, revision, content_json, observations_json)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![source_id, revision, content, observations],
                )?;
                revision
            }
        };
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(SourceRevisionRef {
            case_id: case_id.into(),
            source_id: source_id.into(),
            revision,
        })
    }

    pub fn require_current(&self, reference: &SourceRevisionRef) -> DbResult<()> {
        require_case(self.conn, &reference.case_id, &reference.source_id)?;
        let current: Option<u64> = self.conn.query_row(
            "SELECT MAX(revision) FROM source_revisions WHERE source_id = ?1",
            [&reference.source_id],
            |row| row.get(0),
        )?;
        if current != Some(reference.revision) {
            return Err(DbError::System(
                "source revision is no longer current".into(),
            ));
        }
        Ok(())
    }

    pub fn current(
        &self,
        case_id: &str,
        source_id: &str,
    ) -> DbResult<(SourceRevisionRef, SourceRevisionContent)> {
        require_case(self.conn, case_id, source_id)?;
        let (revision, json): (u64, String) = self.conn.query_row(
            "SELECT revision, content_json FROM source_revisions WHERE source_id = ?1 ORDER BY revision DESC LIMIT 1",
            [source_id], |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        Ok((
            SourceRevisionRef {
                case_id: case_id.into(),
                source_id: source_id.into(),
                revision,
            },
            serde_json::from_str(&json).map_err(json_error)?,
        ))
    }

    pub fn observations(&self, reference: &SourceRevisionRef) -> DbResult<Vec<NativeObservation>> {
        require_case(self.conn, &reference.case_id, &reference.source_id)?;
        let json: String = self.conn.query_row(
            "SELECT observations_json FROM source_revisions WHERE source_id = ?1 AND revision = ?2",
            params![reference.source_id, reference.revision],
            |row| row.get(0),
        )?;
        serde_json::from_str(&json).map_err(json_error)
    }

    pub fn bind_inputs(
        &self,
        case_id: &str,
        derived_source_id: &str,
        inputs: &[SourceRevisionRef],
    ) -> DbResult<()> {
        if inputs.is_empty() {
            return Err(DbError::System("structure view inputs are missing".into()));
        }
        let transaction = self
            .conn
            .is_autocommit()
            .then(|| self.conn.unchecked_transaction())
            .transpose()?;
        require_case(self.conn, case_id, derived_source_id)?;
        let repo = SourceRevisionRepo::new(self.conn);
        let mut expected = inputs.to_vec();
        expected.sort_by(|left, right| left.source_id.cmp(&right.source_id));
        for (index, input) in expected.iter().enumerate() {
            if input.case_id != case_id
                || input.source_id == derived_source_id
                || index > 0 && expected[index - 1].source_id == input.source_id
            {
                return Err(DbError::System(
                    "invalid structure view parent identity".into(),
                ));
            }
            repo.require_current(input)?;
            let cycle: bool = self.conn.query_row(
                "WITH RECURSIVE parents(id) AS (
                 SELECT ?1 UNION SELECT parent_source_id FROM structure_view_inputs JOIN parents ON derived_source_id = parents.id)
                 SELECT EXISTS(SELECT 1 FROM parents WHERE id = ?2)",
                params![input.source_id, derived_source_id], |row| row.get(0),
            )?;
            if cycle {
                return Err(DbError::System("structure view dependency cycle".into()));
            }
        }
        let prior = repo.inputs(case_id, derived_source_id)?;
        if !prior.is_empty() && prior != expected {
            return Err(DbError::System("structure view inputs changed".into()));
        }
        if prior.is_empty() {
            for input in expected {
                self.conn.execute(
                    "INSERT INTO structure_view_inputs VALUES (?1, ?2, ?3)",
                    params![derived_source_id, input.source_id, input.revision],
                )?;
            }
        }
        if let Some(transaction) = transaction {
            transaction.commit()?;
        }
        Ok(())
    }

    pub fn inputs(
        &self,
        case_id: &str,
        derived_source_id: &str,
    ) -> DbResult<Vec<SourceRevisionRef>> {
        require_case(self.conn, case_id, derived_source_id)?;
        let mut statement = self.conn.prepare(
            "SELECT parent_source_id, parent_revision FROM structure_view_inputs
             WHERE derived_source_id = ?1 ORDER BY parent_source_id",
        )?;
        let inputs = statement
            .query_map([derived_source_id], |row| {
                Ok(SourceRevisionRef {
                    case_id: case_id.into(),
                    source_id: row.get(0)?,
                    revision: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for input in &inputs {
            require_case(self.conn, case_id, &input.source_id)?;
        }
        Ok(inputs)
    }

    pub fn require_current_inputs(&self, case_id: &str, derived_source_id: &str) -> DbResult<()> {
        let inputs = self.inputs(case_id, derived_source_id)?;
        if inputs.is_empty() {
            return Err(DbError::System("structure view inputs are missing".into()));
        }
        for input in inputs {
            self.require_current(&input)?;
        }
        Ok(())
    }
}

fn require_case(conn: &Connection, case_id: &str, source_id: &str) -> DbResult<()> {
    let belongs: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM data_sources WHERE id = ?1 AND case_id = ?2)",
        params![source_id, case_id],
        |row| row.get(0),
    )?;
    if !belongs {
        return Err(DbError::System(
            "source revision belongs to another case or missing source".into(),
        ));
    }
    Ok(())
}

fn json_error(error: serde_json::Error) -> DbError {
    DbError::System(format!("invalid source revision JSON: {error}"))
}

fn validate(content: &SourceRevisionContent, observations: &[NativeObservation]) -> DbResult<()> {
    if content.logical_sha256.as_ref().is_some_and(|digest| {
        digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }) {
        return Err(DbError::System("invalid logical SHA-256".into()));
    }
    for observation in observations {
        if observation.native_identity.is_empty() || observation.parser_version.is_empty() {
            return Err(DbError::System(
                "native observation identity is missing".into(),
            ));
        }
        if let domain::structure::ObservationLocation::SourceWindow {
            byte_offset,
            byte_length,
        } = observation.location
        {
            if byte_length == 0
                || byte_offset
                    .checked_add(byte_length)
                    .is_none_or(|end| end > content.logical_size)
            {
                return Err(DbError::System(
                    "native observation range exceeds source".into(),
                ));
            }
        }
    }
    Ok(())
}
