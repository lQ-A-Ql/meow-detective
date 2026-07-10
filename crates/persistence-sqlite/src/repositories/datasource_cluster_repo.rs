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
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = crate::connection::open_in_memory().unwrap();
        crate::migrations::runner::run_all(&conn).unwrap();
        conn.execute(
            "INSERT INTO cases (id, name, created_at, updated_at)
             VALUES ('case-1', 'case', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn cluster_record_round_trips_and_updates_state() {
        let conn = setup_db();
        let repo = DataSourceClusterRepo::new(&conn);
        let record = DataSourceClusterRecord {
            id: "cluster-1".to_string(),
            case_id: CaseId("case-1".to_string()),
            name: "pve".to_string(),
            root_path: "D:/cluster".to_string(),
            platform: "linux".to_string(),
            profile: Some("pve".to_string()),
            manifest_rel_path: "clusters/cluster-1/cluster-manifest.json".to_string(),
            import_state: "pending".to_string(),
            member_count: 2,
            ready_count: 0,
            failed_count: 0,
            last_error: None,
        };

        repo.insert_pending(&record).unwrap();
        repo.update_state("cluster-1", "ready", 2, 0, None).unwrap();

        let stored = repo.find_by_id("cluster-1").unwrap().unwrap();
        assert_eq!(stored.id, "cluster-1");
        assert_eq!(stored.import_state, "ready");
        assert_eq!(stored.ready_count, 2);
        assert_eq!(stored.member_count, 2);
    }

    #[test]
    fn cluster_state_update_requires_existing_cluster() {
        let conn = setup_db();
        let repo = DataSourceClusterRepo::new(&conn);

        let error = repo.update_state("missing-cluster", "failed", 0, 1, Some("failed"));

        assert!(error.is_err());
    }

    #[test]
    fn cluster_state_rejects_invalid_state() {
        let conn = setup_db();
        let repo = DataSourceClusterRepo::new(&conn);
        let record = DataSourceClusterRecord {
            id: "cluster-1".to_string(),
            case_id: CaseId("case-1".to_string()),
            name: "pve".to_string(),
            root_path: "D:/cluster".to_string(),
            platform: "linux".to_string(),
            profile: Some("pve".to_string()),
            manifest_rel_path: "clusters/cluster-1/cluster-manifest.json".to_string(),
            import_state: "pending".to_string(),
            member_count: 2,
            ready_count: 0,
            failed_count: 0,
            last_error: None,
        };
        repo.insert_pending(&record).unwrap();

        let error = repo.update_state("cluster-1", "unknown", 0, 0, None);

        assert!(error.is_err());
    }
}
