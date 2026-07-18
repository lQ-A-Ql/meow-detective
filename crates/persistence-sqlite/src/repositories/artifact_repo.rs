use crate::connection::DbResult;
use domain::{Artifact, ArtifactId};
use rusqlite::{params, Connection};

pub struct ArtifactRepo<'a> {
    conn: &'a Connection,
}

impl<'a> ArtifactRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert_batch(
        &self,
        artifacts: &[Artifact],
        case_id: &str,
        data_source_id: &str,
    ) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        ArtifactRepo::new(&tx).insert_batch_in_transaction(artifacts, case_id, data_source_id)?;
        tx.commit()?;
        Ok(())
    }

    pub fn insert_batch_in_transaction(
        &self,
        artifacts: &[Artifact],
        case_id: &str,
        data_source_id: &str,
    ) -> DbResult<()> {
        let mut stmt = self.conn.prepare_cached(
            "INSERT INTO artifacts (id, case_id, data_source_id, artifact_type, source_object_id, extractor_id, extractor_version, confidence, source_attribution, title, summary, attrs, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )?;
        for artifact in artifacts {
            stmt.execute(params![
                artifact.id.0,
                case_id,
                data_source_id,
                artifact.family,
                artifact.source_object_id.as_ref().map(|id| &id.0),
                artifact.extractor_id,
                artifact.extractor_version,
                artifact.confidence,
                artifact.source_attribution,
                artifact.title,
                artifact.summary,
                serde_json::to_string(&artifact.attrs).unwrap_or_else(|_| "{}".to_string()),
                artifact.created_at.to_rfc3339(),
            ])?;
        }
        Ok(())
    }

    pub fn delete_analysis_outputs_in_transaction(
        &self,
        source_object_id: &str,
        producer_prefix: &str,
    ) -> DbResult<usize> {
        self.conn
            .execute(
                "DELETE FROM artifacts
                 WHERE source_object_id = ?1
                   AND extractor_id LIKE ?2",
                params![source_object_id, format!("{producer_prefix}%")],
            )
            .map_err(Into::into)
    }

    pub fn list_analysis_outputs(
        &self,
        source_object_id: &str,
        producer_prefix: &str,
    ) -> DbResult<Vec<Artifact>> {
        let mut statement = self.conn.prepare(
            "SELECT id, artifact_type, source_object_id, extractor_id, extractor_version,
                    confidence, source_attribution, title, summary, attrs, created_at
             FROM artifacts
             WHERE source_object_id = ?1
               AND extractor_id LIKE ?2
             ORDER BY rowid ASC",
        )?;
        let rows = statement.query_map(
            params![source_object_id, format!("{producer_prefix}%")],
            row_to_artifact,
        )?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn count_analysis_outputs(
        &self,
        source_object_id: &str,
        producer_prefix: &str,
    ) -> DbResult<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*)
             FROM artifacts
             WHERE source_object_id = ?1
               AND extractor_id LIKE ?2",
            params![source_object_id, format!("{producer_prefix}%")],
            |row| row.get(0),
        )?;
        Ok(count.max(0) as u64)
    }

    pub fn list_analysis_outputs_for_prefixes(
        &self,
        producer_prefixes: &[&str],
    ) -> DbResult<Vec<Artifact>> {
        if producer_prefixes.is_empty() {
            return Ok(Vec::new());
        }
        let predicates = (1..=producer_prefixes.len())
            .map(|index| format!("extractor_id LIKE ?{index}"))
            .collect::<Vec<_>>()
            .join(" OR ");
        let sql = format!(
            "SELECT id, artifact_type, source_object_id, extractor_id, extractor_version,
                    confidence, source_attribution, title, summary, attrs, created_at
             FROM artifacts
             WHERE source_object_id IS NOT NULL
               AND extractor_id IS NOT NULL
               AND ({predicates})
             ORDER BY rowid ASC"
        );
        let patterns = producer_prefixes
            .iter()
            .map(|prefix| format!("{prefix}%"))
            .collect::<Vec<_>>();
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(rusqlite::params_from_iter(patterns), row_to_artifact)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_analysis_outputs_for_sources(
        &self,
        source_object_ids: &[&str],
        producer_prefix: &str,
    ) -> DbResult<Vec<Artifact>> {
        if source_object_ids.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = (1..=source_object_ids.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let prefix_parameter = source_object_ids.len() + 1;
        let sql = format!(
            "SELECT id, artifact_type, source_object_id, extractor_id, extractor_version,
                    confidence, source_attribution, title, summary, attrs, created_at
             FROM artifacts
             WHERE source_object_id IN ({placeholders})
               AND extractor_id LIKE ?{prefix_parameter}
             ORDER BY source_object_id ASC, rowid ASC"
        );
        let mut parameters = source_object_ids
            .iter()
            .map(|source_id| (*source_id).to_string())
            .collect::<Vec<_>>();
        parameters.push(format!("{producer_prefix}%"));
        let mut statement = self.conn.prepare(&sql)?;
        let rows = statement.query_map(rusqlite::params_from_iter(parameters), row_to_artifact)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn list_by_family(&self, family: Option<&str>) -> DbResult<Vec<Artifact>> {
        let mut stmt = match family {
            Some(_) => self.conn.prepare(
                "SELECT id, artifact_type, source_object_id, extractor_id, extractor_version, confidence, source_attribution, title, summary, attrs, created_at
                 FROM artifacts WHERE artifact_type = ?1 ORDER BY created_at DESC LIMIT 1000",
            )?,
            None => self.conn.prepare(
                "SELECT id, artifact_type, source_object_id, extractor_id, extractor_version, confidence, source_attribution, title, summary, attrs, created_at
                 FROM artifacts ORDER BY created_at DESC LIMIT 1000",
            )?,
        };
        let rows = match family {
            Some(f) => stmt.query_map(params![f], row_to_artifact)?,
            None => stmt.query_map([], row_to_artifact)?,
        };
        let mut artifacts = Vec::new();
        for row in rows {
            artifacts.push(row?);
        }
        Ok(artifacts)
    }

    pub fn families(&self) -> DbResult<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT artifact_type FROM artifacts ORDER BY artifact_type")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut families = Vec::new();
        for row in rows {
            families.push(row?);
        }
        Ok(families)
    }

    pub fn count(&self) -> DbResult<u64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM artifacts", [], |row| row.get(0))?;
        Ok(count as u64)
    }

    pub fn count_by_family(&self) -> DbResult<Vec<(String, u64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT artifact_type, COUNT(*) FROM artifacts GROUP BY artifact_type ORDER BY artifact_type",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u64))
        })?;
        let mut counts = Vec::new();
        for row in rows {
            counts.push(row?);
        }
        Ok(counts)
    }

    pub fn find_by_id(&self, artifact_id: &str) -> DbResult<Option<Artifact>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, artifact_type, source_object_id, extractor_id, extractor_version, confidence, source_attribution, title, summary, attrs, created_at
             FROM artifacts WHERE id = ?1",
        )?;
        let result = stmt.query_row(params![artifact_id], row_to_artifact);
        match result {
            Ok(artifact) => Ok(Some(artifact)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Load artifacts by family type, returning (id, attrs) pairs.
    /// Used by rule pack engine to find source artifacts for matching.
    pub fn find_by_family_raw(&self, family: &str) -> DbResult<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, attrs FROM artifacts WHERE artifact_type = ?1")?;
        let rows = stmt.query_map(params![family], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    /// Find distinct extractor version info for families matching a given parser name.
    /// Returns (extractor_id, extractor_version) pairs.
    pub fn find_extractor_versions(&self, parser: &str) -> DbResult<Vec<(String, Option<String>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT extractor_id, extractor_version FROM artifacts
             WHERE LOWER(extractor_id) = LOWER(?1) AND extractor_version IS NOT NULL
             LIMIT 1",
        )?;
        let rows = stmt.query_map(params![parser], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }
}

fn row_to_artifact(row: &rusqlite::Row) -> rusqlite::Result<Artifact> {
    let attrs_str: String = row.get(9)?;
    let attrs: std::collections::BTreeMap<String, serde_json::Value> =
        serde_json::from_str(&attrs_str).unwrap_or_default();
    Ok(Artifact {
        id: ArtifactId(row.get::<_, String>(0)?),
        family: row.get(1)?,
        source_object_id: row.get::<_, Option<String>>(2)?.map(domain::FileEntryId),
        extractor_id: row.get(3)?,
        extractor_version: row.get(4)?,
        confidence: row.get(5)?,
        source_attribution: row.get(6)?,
        title: row.get(7)?,
        summary: row.get(8)?,
        created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(10)?)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|e| {
                tracing::warn!("Invalid artifact timestamp, falling back to epoch: {}", e);
                chrono::DateTime::default()
            }),
        attrs,
    })
}

#[cfg(test)]
#[path = "../../tests/unit/repositories/artifact_repo.rs"]
mod tests;
