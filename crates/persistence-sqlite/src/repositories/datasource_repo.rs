use crate::connection::DbResult;
use domain::{CaseId, DataSource, DataSourceId, DataSourceKind};
use rusqlite::{params, Connection};

type ProgressCallback<'a> = &'a dyn Fn(u32, &str);

pub struct DataSourceRepo<'a> {
    conn: &'a Connection,
}

impl<'a> DataSourceRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, case_id: &CaseId, ds: &DataSource) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO data_sources (id, case_id, name, kind, source_path) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![ds.id.0, case_id.0, ds.name, kind_to_str(&ds.kind), ds.source_path.display().to_string()],
        )?;
        Ok(())
    }

    pub fn find_by_case(&self, case_id: &CaseId) -> DbResult<Vec<DataSource>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, kind, source_path, imported_at FROM data_sources WHERE case_id = ?1 ORDER BY imported_at DESC, name ASC",
        )?;
        let rows = stmt.query_map(params![case_id.0], |row| {
            Ok(DataSource {
                id: DataSourceId(row.get(0)?),
                name: row.get(1)?,
                kind: str_to_kind(&row.get::<_, String>(2)?),
                source_path: std::path::PathBuf::from(row.get::<_, String>(3)?),
                imported_at: crate::util::parse_datetime(&row.get::<_, String>(4)?),
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn rename(&self, data_source_id: &DataSourceId, name: &str) -> DbResult<()> {
        self.conn.execute(
            "UPDATE data_sources SET name = ?1 WHERE id = ?2",
            params![name, data_source_id.0],
        )?;
        Ok(())
    }

    pub fn delete_cascade(&self, data_source_id: &DataSourceId) -> DbResult<()> {
        self.delete_cascade_with_progress(data_source_id, None::<ProgressCallback<'_>>)
    }

    /// Delete data source with cascade and progress callback.
    pub fn delete_cascade_with_progress(
        &self,
        data_source_id: &DataSourceId,
        progress: Option<ProgressCallback<'_>>,
    ) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;

        // Step 1: Delete artifacts (10%)
        if let Some(cb) = progress {
            cb(0, "Deleting artifacts...");
        }
        tx.execute(
            "DELETE FROM artifacts WHERE source_object_id IN (
                SELECT id FROM file_entries WHERE data_source_id = ?1
            )",
            params![data_source_id.0],
        )?;

        // Step 2: Delete timeline events (30%)
        if let Some(cb) = progress {
            cb(10, "Deleting timeline events...");
        }
        tx.execute(
            "DELETE FROM timeline_events WHERE source_object_id IN (
                SELECT id FROM file_entries WHERE data_source_id = ?1
            )",
            params![data_source_id.0],
        )?;

        // Step 3: Delete file entries (70%)
        if let Some(cb) = progress {
            cb(30, "Deleting file entries...");
        }
        tx.execute(
            "DELETE FROM file_entries WHERE data_source_id = ?1",
            params![data_source_id.0],
        )?;

        // Step 4: Delete partitions (90%)
        if let Some(cb) = progress {
            cb(70, "Deleting partitions...");
        }
        tx.execute(
            "DELETE FROM data_source_partitions WHERE data_source_id = ?1",
            params![data_source_id.0],
        )?;

        // Step 5: Delete data source (100%)
        if let Some(cb) = progress {
            cb(90, "Deleting data source...");
        }
        tx.execute(
            "DELETE FROM data_sources WHERE id = ?1",
            params![data_source_id.0],
        )?;

        tx.commit()?;
        if let Some(cb) = progress {
            cb(100, "Deletion complete");
        }
        Ok(())
    }
}

fn kind_to_str(kind: &DataSourceKind) -> &'static str {
    match kind {
        DataSourceKind::Raw => "raw",
        DataSourceKind::E01 => "e01",
        DataSourceKind::LogicalDirectory => "logical_directory",
    }
}

fn str_to_kind(s: &str) -> DataSourceKind {
    match s {
        "e01" => DataSourceKind::E01,
        "logical_directory" => DataSourceKind::LogicalDirectory,
        _ => DataSourceKind::Raw,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> rusqlite::Connection {
        let conn = crate::connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE cases (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                number TEXT,
                examiner TEXT,
                notes TEXT,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE data_sources (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL REFERENCES cases(id),
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                source_path TEXT NOT NULL,
                imported_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE file_entries (
                id TEXT PRIMARY KEY NOT NULL,
                parent_id TEXT,
                data_source_id TEXT NOT NULL,
                path TEXT NOT NULL,
                name TEXT NOT NULL,
                entry_type TEXT NOT NULL,
                size INTEGER,
                ext TEXT,
                deleted INTEGER NOT NULL DEFAULT 0,
                created_at TEXT,
                modified_at TEXT,
                accessed_at TEXT,
                changed_at TEXT,
                hash_sha256 TEXT
            );
            CREATE TABLE artifacts (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL DEFAULT '',
                data_source_id TEXT NOT NULL DEFAULT '',
                artifact_type TEXT NOT NULL,
                source_object_id TEXT,
                title TEXT NOT NULL,
                summary TEXT NOT NULL DEFAULT '',
                attrs TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE timeline_events (
                id TEXT PRIMARY KEY NOT NULL,
                case_id TEXT NOT NULL DEFAULT '',
                source_object_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                ts TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                attrs TEXT NOT NULL DEFAULT '{}'
            );
            CREATE TABLE data_source_partitions (
                id TEXT PRIMARY KEY,
                data_source_id TEXT NOT NULL,
                partition_index INTEGER NOT NULL,
                name TEXT NOT NULL,
                kind_label TEXT NOT NULL,
                status TEXT NOT NULL,
                type_guid TEXT,
                offset INTEGER NOT NULL,
                length INTEGER NOT NULL,
                filesystem TEXT,
                unlock_hint TEXT
            );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cases (id, name, created_at, updated_at) VALUES (?1, ?2, datetime('now'), datetime('now'))",
            params!["case-1", "Test Case"],
        ).unwrap();
        conn
    }

    fn make_ds(id: &str, name: &str) -> DataSource {
        DataSource {
            id: DataSourceId(id.to_string()),
            name: name.to_string(),
            kind: DataSourceKind::Raw,
            source_path: std::path::PathBuf::from("/evidence/image.E01"),
            imported_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn insert_then_find_by_case_returns_it() {
        let conn = setup_db();
        let repo = DataSourceRepo::new(&conn);
        let ds = make_ds("ds-1", "Disk Image");
        repo.insert(&CaseId("case-1".to_string()), &ds).unwrap();

        let results = repo.find_by_case(&CaseId("case-1".to_string())).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Disk Image");
        assert_eq!(results[0].kind, DataSourceKind::Raw);
    }

    #[test]
    fn rename_changes_the_name() {
        let conn = setup_db();
        let repo = DataSourceRepo::new(&conn);
        let ds = make_ds("ds-1", "Old Name");
        repo.insert(&CaseId("case-1".to_string()), &ds).unwrap();

        repo.rename(&DataSourceId("ds-1".to_string()), "New Name")
            .unwrap();

        let results = repo.find_by_case(&CaseId("case-1".to_string())).unwrap();
        assert_eq!(results[0].name, "New Name");
    }

    #[test]
    fn delete_cascade_removes_the_record() {
        let conn = setup_db();
        let repo = DataSourceRepo::new(&conn);
        let ds = make_ds("ds-1", "Disk Image");
        repo.insert(&CaseId("case-1".to_string()), &ds).unwrap();

        repo.delete_cascade(&DataSourceId("ds-1".to_string()))
            .unwrap();

        let results = repo.find_by_case(&CaseId("case-1".to_string())).unwrap();
        assert!(results.is_empty());
    }
}
