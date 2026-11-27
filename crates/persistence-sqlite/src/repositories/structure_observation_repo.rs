use crate::{DbError, DbResult};
use domain::structure::{
    NativeVersion, ObservationId, ObservationIndex, ObservationLocation, SourceRevisionRef,
};
use rusqlite::{params, Connection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObservationState {
    Pending,
    Published,
}

impl ObservationState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Published => "published",
        }
    }
}

pub struct StructureObservationRepo<'a> {
    conn: &'a Connection,
}

impl<'a> StructureObservationRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert(
        &self,
        case_id: &str,
        index: &ObservationIndex,
        state: ObservationState,
    ) -> DbResult<()> {
        validate_index(case_id, index)?;
        require_revision(self.conn, &index.source)?;
        let (location_kind, byte_offset, byte_length, gap_reason) = location_columns(index);
        let native_version = serde_json::to_string(&index.version)
            .map_err(|error| DbError::System(error.to_string()))?;
        if let Some(prior) = self.find(case_id, &index.id)? {
            if prior != *index {
                return Err(DbError::System("structure observation changed".into()));
            }
            if state == ObservationState::Published {
                self.mark_published(case_id, &index.id)?;
            }
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO structure_observations (
                id, case_id, source_id, source_revision, native_identity,
                native_version_json, location_kind, byte_offset, byte_length,
                gap_reason, parser_version, native_checksum_verified, detail_key,
                detail_sha256, attempt_id, generation, receipt, state
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                       ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                index.id.0,
                case_id,
                index.source.source_id,
                index.source.revision,
                index.native_identity,
                native_version,
                location_kind,
                byte_offset,
                byte_length,
                gap_reason,
                index.parser_version,
                index.native_checksum_verified,
                index.detail_key,
                index.detail_sha256,
                index.attempt_id,
                index.generation,
                index.receipt,
                state.as_str(),
            ],
        )?;
        Ok(())
    }

    pub fn mark_published(&self, case_id: &str, id: &ObservationId) -> DbResult<()> {
        let changed = self.conn.execute(
            "UPDATE structure_observations
             SET state='published'
             WHERE id=?1 AND case_id=?2 AND state='pending'",
            params![id.0, case_id],
        )?;
        if changed > 1 {
            return Err(DbError::System(
                "structure observation publication changed multiple rows".into(),
            ));
        }
        Ok(())
    }

    pub fn find(&self, case_id: &str, id: &ObservationId) -> DbResult<Option<ObservationIndex>> {
        self.query(
            "WHERE observation.id=?1 AND observation.case_id=?2",
            params![id.0, case_id],
        )
        .map(|mut values| values.pop())
    }

    pub fn list_published(&self, source: &SourceRevisionRef) -> DbResult<Vec<ObservationIndex>> {
        self.query(
            "WHERE observation.case_id=?1 AND observation.source_id=?2
               AND observation.source_revision=?3 AND observation.state='published'",
            params![source.case_id, source.source_id, source.revision],
        )
    }

    pub fn count_published(&self, source: &SourceRevisionRef) -> DbResult<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM structure_observations
             WHERE case_id=?1 AND source_id=?2 AND source_revision=?3
               AND state='published'",
            params![source.case_id, source.source_id, source.revision],
            |row| row.get(0),
        )?;
        u64::try_from(count)
            .map_err(|_| DbError::System("structure observation count is invalid".into()))
    }

    fn query(
        &self,
        filter: &str,
        params: impl rusqlite::Params,
    ) -> DbResult<Vec<ObservationIndex>> {
        let sql = format!(
            "SELECT observation.id, observation.case_id, observation.source_id,
                    observation.source_revision, observation.native_identity,
                    observation.native_version_json, observation.location_kind,
                    observation.byte_offset, observation.byte_length,
                    observation.gap_reason, observation.parser_version,
                    observation.native_checksum_verified, observation.detail_key,
                    observation.detail_sha256, observation.attempt_id,
                    observation.generation, observation.receipt
             FROM structure_observations AS observation
             {filter}
             ORDER BY observation.id"
        );
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(params, parse_index)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

fn validate_index(case_id: &str, index: &ObservationIndex) -> DbResult<()> {
    if index.source.case_id != case_id
        || index.native_identity.is_empty()
        || index.parser_version.is_empty()
        || index.detail_key.is_empty()
        || index.attempt_id.is_empty()
        || index.generation.is_empty()
        || !valid_digest(&index.detail_sha256)
        || !valid_digest(&index.receipt)
    {
        return Err(DbError::System(
            "structure observation identity is invalid".into(),
        ));
    }
    if let ObservationLocation::SourceWindow {
        byte_offset,
        byte_length,
    } = index.location
    {
        if byte_length == 0 || byte_offset.checked_add(byte_length).is_none() {
            return Err(DbError::System(
                "structure observation range is invalid".into(),
            ));
        }
    }
    Ok(())
}

fn require_revision(conn: &Connection, source: &SourceRevisionRef) -> DbResult<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(
             SELECT 1 FROM source_revisions
             WHERE source_id=?1 AND revision=?2
         )",
        params![source.source_id, source.revision],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(DbError::System(
            "structure observation source revision is missing".into(),
        ));
    }
    Ok(())
}

fn location_columns(
    index: &ObservationIndex,
) -> (&'static str, Option<i64>, Option<i64>, Option<&str>) {
    match &index.location {
        ObservationLocation::SourceWindow {
            byte_offset,
            byte_length,
        } => (
            "source_window",
            i64::try_from(*byte_offset).ok(),
            i64::try_from(*byte_length).ok(),
            None,
        ),
        ObservationLocation::Unavailable { reason } => {
            ("unavailable", None, None, Some(reason.as_str()))
        }
    }
}

fn parse_index(row: &rusqlite::Row<'_>) -> rusqlite::Result<ObservationIndex> {
    let native_version: String = row.get(5)?;
    let location_kind: String = row.get(6)?;
    let byte_offset: Option<i64> = row.get(7)?;
    let byte_length: Option<i64> = row.get(8)?;
    let gap_reason: Option<String> = row.get(9)?;
    let location = match location_kind.as_str() {
        "source_window" => ObservationLocation::SourceWindow {
            byte_offset: to_u64(byte_offset, 7)?,
            byte_length: to_u64(byte_length, 8)?,
        },
        "unavailable" => ObservationLocation::Unavailable {
            reason: gap_reason.ok_or_else(|| conversion_error(9, "missing location gap"))?,
        },
        _ => return Err(conversion_error(6, "invalid observation location")),
    };
    Ok(ObservationIndex {
        id: ObservationId(row.get(0)?),
        source: SourceRevisionRef {
            case_id: row.get(1)?,
            source_id: row.get(2)?,
            revision: to_u64(Some(row.get::<_, i64>(3)?), 3)?,
        },
        native_identity: row.get(4)?,
        version: serde_json::from_str::<NativeVersion>(&native_version)
            .map_err(|error| conversion_error(5, &format!("invalid native version: {error}")))?,
        location,
        parser_version: row.get(10)?,
        native_checksum_verified: row.get(11)?,
        detail_key: row.get(12)?,
        detail_sha256: row.get(13)?,
        attempt_id: row.get(14)?,
        generation: row.get(15)?,
        receipt: row.get(16)?,
    })
}

fn to_u64(value: impl Into<Option<i64>>, column: usize) -> rusqlite::Result<u64> {
    let value = value
        .into()
        .ok_or_else(|| conversion_error(column, "required integer is missing"))?;
    u64::try_from(value).map_err(|_| conversion_error(column, "integer is out of range"))
}

fn conversion_error(column: usize, message: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message.to_owned(),
        )),
    )
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
