use crate::connection::{DbError, DbResult};
use domain::DataSourceId;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogPublication {
    pub data_source_id: DataSourceId,
    pub attempt_id: String,
    pub input_fingerprint: String,
    pub source_db_rel_path: String,
    pub catalog_digest: String,
    pub seal: String,
    pub state: String,
}

pub struct CatalogPublicationRepo<'a> {
    conn: &'a Connection,
}

impl<'a> CatalogPublicationRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn prepare(
        &self,
        data_source_id: &DataSourceId,
        attempt_id: &str,
        input_fingerprint: &str,
        source_db_rel_path: &str,
        catalog_digest: &str,
    ) -> DbResult<CatalogPublication> {
        validate_hex("input fingerprint", input_fingerprint)?;
        validate_hex("catalog digest", catalog_digest)?;
        if attempt_id.trim().is_empty() || source_db_rel_path.trim().is_empty() {
            return Err(DbError::System(
                "Catalog publication identity must not be empty".to_string(),
            ));
        }
        let seal = seal_for(
            &data_source_id.0,
            attempt_id,
            input_fingerprint,
            source_db_rel_path,
            catalog_digest,
        );
        let affected = self.conn.execute(
            "INSERT INTO data_source_catalog_publications (
                data_source_id, attempt_id, input_fingerprint, source_db_rel_path,
                catalog_digest, seal, state
             )
             SELECT ?1, ?2, ?3, ?4, ?5, ?6, 'prepared'
             WHERE EXISTS (
                 SELECT 1
                 FROM data_source_processing_phases
                 WHERE data_source_id = ?1
                   AND phase = 'catalog'
                   AND state = 'running'
                   AND attempt_id = ?2
                   AND input_fingerprint = ?3
             )
             ON CONFLICT(data_source_id) DO UPDATE SET
                attempt_id = excluded.attempt_id,
                input_fingerprint = excluded.input_fingerprint,
                source_db_rel_path = excluded.source_db_rel_path,
                catalog_digest = excluded.catalog_digest,
                seal = excluded.seal,
                state = 'prepared',
                created_at = datetime('now'),
                published_at = NULL
             WHERE data_source_catalog_publications.state = 'prepared'",
            params![
                data_source_id.0,
                attempt_id,
                input_fingerprint,
                source_db_rel_path,
                catalog_digest,
                seal,
            ],
        )?;
        if affected != 1 {
            return Err(DbError::System(
                "Catalog publication claim is stale or no longer running".to_string(),
            ));
        }
        self.find(data_source_id)?
            .ok_or_else(|| DbError::System("Catalog publication was not persisted".to_string()))
    }

    pub fn mark_published(
        &self,
        data_source_id: &DataSourceId,
        attempt_id: &str,
        seal: &str,
    ) -> DbResult<CatalogPublication> {
        let affected = self.conn.execute(
            "UPDATE data_source_catalog_publications
             SET state = 'published', published_at = datetime('now')
             WHERE data_source_id = ?1
               AND attempt_id = ?2
               AND seal = ?3
               AND state = 'prepared'",
            params![data_source_id.0, attempt_id, seal],
        )?;
        if affected != 1 {
            return Err(DbError::System(
                "Catalog publication seal is stale or already finalized".to_string(),
            ));
        }
        self.find(data_source_id)?
            .ok_or_else(|| DbError::System("Published Catalog seal disappeared".to_string()))
    }

    pub fn find(&self, data_source_id: &DataSourceId) -> DbResult<Option<CatalogPublication>> {
        self.conn
            .query_row(
                "SELECT data_source_id, attempt_id, input_fingerprint,
                        source_db_rel_path, catalog_digest, seal, state
                 FROM data_source_catalog_publications
                 WHERE data_source_id = ?1",
                [data_source_id.0.as_str()],
                read_publication,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn is_published(
        &self,
        data_source_id: &DataSourceId,
        input_fingerprint: &str,
        source_db_rel_path: &str,
        catalog_digest: &str,
    ) -> DbResult<bool> {
        let Some(publication) = self.find(data_source_id)? else {
            return Ok(false);
        };
        Ok(publication.state == "published"
            && publication.input_fingerprint == input_fingerprint
            && publication.source_db_rel_path == source_db_rel_path
            && publication.catalog_digest == catalog_digest
            && publication.seal
                == seal_for(
                    &data_source_id.0,
                    &publication.attempt_id,
                    input_fingerprint,
                    source_db_rel_path,
                    catalog_digest,
                ))
    }
}

fn read_publication(row: &rusqlite::Row<'_>) -> rusqlite::Result<CatalogPublication> {
    Ok(CatalogPublication {
        data_source_id: DataSourceId(row.get(0)?),
        attempt_id: row.get(1)?,
        input_fingerprint: row.get(2)?,
        source_db_rel_path: row.get(3)?,
        catalog_digest: row.get(4)?,
        seal: row.get(5)?,
        state: row.get(6)?,
    })
}

pub fn seal_for(
    data_source_id: &str,
    attempt_id: &str,
    input_fingerprint: &str,
    source_db_rel_path: &str,
    catalog_digest: &str,
) -> String {
    let mut hasher = Sha256::new();
    for value in [
        b"meow-detective-catalog-publication-v1".as_slice(),
        data_source_id.as_bytes(),
        attempt_id.as_bytes(),
        input_fingerprint.as_bytes(),
        source_db_rel_path.as_bytes(),
        catalog_digest.as_bytes(),
    ] {
        hasher.update((value.len() as u64).to_le_bytes());
        hasher.update(value);
    }
    hex::encode(hasher.finalize())
}

fn validate_hex(label: &str, value: &str) -> DbResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(DbError::System(format!(
            "Catalog publication {label} must be a lowercase SHA-256 fingerprint"
        )));
    }
    Ok(())
}
