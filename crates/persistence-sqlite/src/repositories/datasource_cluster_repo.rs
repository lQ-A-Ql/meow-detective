use crate::connection::DbResult;
use domain::CaseId;
use rusqlite::{params, Connection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataSourceClusterRecord {
    pub id: String,
    pub case_id: CaseId,
    pub name: String,
    pub root_path: String,
    pub platform: String,
    pub profile: Option<String>,
    pub manifest_rel_path: String,
    pub import_state: String,
    pub member_count: u32,
    pub ready_count: u32,
    pub failed_count: u32,
    pub last_error: Option<String>,
}

pub struct DataSourceClusterRepo<'a> {
    conn: &'a Connection,
}

impl<'a> DataSourceClusterRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert_pending(&self, record: &DataSourceClusterRecord) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO data_source_clusters (
                id, case_id, name, root_path, platform, profile, manifest_rel_path,
                import_state, member_count, ready_count, failed_count, last_error
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                record.id,
                record.case_id.0,
                record.name,
                record.root_path,
                record.platform,
                record.profile,
                record.manifest_rel_path,
                record.import_state,
                record.member_count,
                record.ready_count,
                record.failed_count,
                record.last_error,
            ],
        )?;
        Ok(())
    }

    pub fn update_state(
        &self,
        cluster_id: &str,
        import_state: &str,
        ready_count: u32,
        failed_count: u32,
        last_error: Option<&str>,
    ) -> DbResult<()> {
        let affected = self.conn.execute(
            "UPDATE data_source_clusters
             SET import_state = ?1,
                 ready_count = ?2,
                 failed_count = ?3,
                 last_error = ?4,
                 updated_at = datetime('now')
             WHERE id = ?5",
            params![
                import_state,
                ready_count,
                failed_count,
                last_error,
                cluster_id
            ],
        )?;
        if affected != 1 {
            return Err(crate::connection::DbError::System(format!(
                "data source cluster not found: {cluster_id}"
            )));
        }
        Ok(())
    }

    pub fn find_by_id(&self, cluster_id: &str) -> DbResult<Option<DataSourceClusterRecord>> {
        let result = self.conn.query_row(
            "SELECT id, case_id, name, root_path, platform, profile, manifest_rel_path,
                    import_state, member_count, ready_count, failed_count, last_error
             FROM data_source_clusters WHERE id = ?1",
            params![cluster_id],
            |row| {
                Ok(DataSourceClusterRecord {
                    id: row.get(0)?,
                    case_id: CaseId(row.get(1)?),
                    name: row.get(2)?,
                    root_path: row.get(3)?,
                    platform: row.get(4)?,
                    profile: row.get(5)?,
                    manifest_rel_path: row.get(6)?,
                    import_state: row.get(7)?,
                    member_count: row.get::<_, i64>(8)?.max(0) as u32,
                    ready_count: row.get::<_, i64>(9)?.max(0) as u32,
                    failed_count: row.get::<_, i64>(10)?.max(0) as u32,
                    last_error: row.get(11)?,
                })
            },
        );
        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/repositories/datasource_cluster_repo.rs"]
mod tests;
