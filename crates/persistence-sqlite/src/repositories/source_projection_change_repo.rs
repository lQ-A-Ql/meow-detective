use crate::connection::{DbError, DbResult};
use rusqlite::{params, Connection};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceProjectionEntity {
    Artifact,
    Timeline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceProjectionChangeOperation {
    Upsert,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceProjectionChange {
    pub entity: SourceProjectionEntity,
    pub entity_id: String,
    pub source_id: String,
    pub operation: SourceProjectionChangeOperation,
    pub revision: u64,
}

pub struct SourceProjectionChangeRepo<'a> {
    connection: &'a Connection,
}

impl<'a> SourceProjectionChangeRepo<'a> {
    pub fn new(connection: &'a Connection) -> Self {
        Self { connection }
    }

    pub fn high_water(&self) -> DbResult<u64> {
        let revision = self.connection.query_row(
            "SELECT revision FROM source_projection_change_clock WHERE singleton = 1",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        u64::try_from(revision)
            .map_err(|_| DbError::Migration("source projection revision is negative".to_owned()))
    }

    pub fn acquire_projection_writer_barrier(&self) -> DbResult<()> {
        self.connection.execute(
            "UPDATE source_projection_change_clock
             SET revision = revision
             WHERE singleton = 1",
            [],
        )?;
        Ok(())
    }

    pub fn list_after(
        &self,
        after_revision: u64,
        through_revision: u64,
        limit: usize,
    ) -> DbResult<Vec<SourceProjectionChange>> {
        if limit == 0 || after_revision >= through_revision {
            return Ok(Vec::new());
        }
        let after_revision = to_sql_revision(after_revision)?;
        let through_revision = to_sql_revision(through_revision)?;
        let limit = i64::try_from(limit).map_err(|_| {
            DbError::Migration("source projection page size is too large".to_owned())
        })?;
        let mut statement = self.connection.prepare(
            "SELECT entity, entity_id, source_id, operation, revision
             FROM source_projection_changes
             WHERE revision > ?1 AND revision <= ?2
             ORDER BY revision ASC
             LIMIT ?3",
        )?;
        let rows =
            statement.query_map(params![after_revision, through_revision, limit], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })?;
        rows.map(|row| {
            let (entity, entity_id, source_id, operation, revision) = row?;
            Ok(SourceProjectionChange {
                entity: match entity.as_str() {
                    "artifact" => SourceProjectionEntity::Artifact,
                    "timeline" => SourceProjectionEntity::Timeline,
                    _ => {
                        return Err(rusqlite::Error::InvalidColumnType(
                            0,
                            "entity".to_owned(),
                            rusqlite::types::Type::Text,
                        ));
                    }
                },
                entity_id,
                source_id,
                operation: match operation.as_str() {
                    "upsert" => SourceProjectionChangeOperation::Upsert,
                    "delete" => SourceProjectionChangeOperation::Delete,
                    _ => {
                        return Err(rusqlite::Error::InvalidColumnType(
                            3,
                            "operation".to_owned(),
                            rusqlite::types::Type::Text,
                        ));
                    }
                },
                revision: u64::try_from(revision)
                    .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(4, revision))?,
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
    }

    pub fn prune_through(&self, revision: u64) -> DbResult<usize> {
        self.connection
            .execute(
                "DELETE FROM source_projection_changes WHERE revision <= ?1",
                [to_sql_revision(revision)?],
            )
            .map_err(Into::into)
    }
}

fn to_sql_revision(revision: u64) -> DbResult<i64> {
    i64::try_from(revision)
        .map_err(|_| DbError::Migration("source projection revision overflowed".to_owned()))
}

#[cfg(test)]
#[path = "../../tests/unit/repositories/source_projection_change_repo.rs"]
mod tests;
