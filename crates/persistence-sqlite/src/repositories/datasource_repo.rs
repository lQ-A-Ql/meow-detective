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
