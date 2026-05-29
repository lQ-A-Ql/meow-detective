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
        {
            let mut stmt = tx.prepare_cached(
                "INSERT INTO artifacts (id, case_id, data_source_id, artifact_type, source_object_id, title, summary, attrs, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for artifact in artifacts {
                stmt.execute(params![
                    artifact.id.0,
                    case_id,
                    data_source_id,
                    artifact.family,
                    artifact.source_object_id.as_ref().map(|id| &id.0),
                    artifact.title,
                    artifact.summary,
                    serde_json::to_string(&artifact.attrs).unwrap_or_else(|_| "{}".to_string()),
                    artifact.created_at.to_rfc3339(),
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_by_family(&self, family: Option<&str>) -> DbResult<Vec<Artifact>> {
        let mut stmt = match family {
            Some(_) => self.conn.prepare(
                "SELECT id, artifact_type, source_object_id, title, summary, attrs, created_at
                 FROM artifacts WHERE artifact_type = ?1 ORDER BY created_at DESC LIMIT 1000",
            )?,
            None => self.conn.prepare(
                "SELECT id, artifact_type, source_object_id, title, summary, attrs, created_at
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
}

fn row_to_artifact(row: &rusqlite::Row) -> rusqlite::Result<Artifact> {
    let attrs_str: String = row.get(5)?;
    let attrs: std::collections::BTreeMap<String, serde_json::Value> =
        serde_json::from_str(&attrs_str).unwrap_or_default();
    Ok(Artifact {
        id: ArtifactId(row.get::<_, String>(0)?),
        family: row.get(1)?,
        source_object_id: row.get::<_, Option<String>>(2)?.map(domain::FileEntryId),
        title: row.get(3)?,
        summary: row.get(4)?,
        created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>(6)?)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|e| {
                tracing::warn!("Invalid artifact timestamp, falling back to epoch: {}", e);
                chrono::DateTime::default()
            }),
        attrs,
    })
}
