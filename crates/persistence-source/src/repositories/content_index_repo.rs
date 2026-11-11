use persistence_fjall::{ArtifactRecordStore, ScanDirection};
use persistence_sqlite::DbResult;
use rusqlite::{params, Connection, OptionalExtension};
pub struct ContentIndexRepo<'a> {
    conn: &'a Connection,
}
impl<'a> ContentIndexRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
    pub fn checkpoint(&self, key: &str) -> DbResult<Option<(String, String, bool)>> {
        self.conn.query_row("SELECT after_id,summary_json,published FROM content_index_checkpoints WHERE fingerprint=?1",[key],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional().map_err(Into::into)
    }
    pub fn begin(&self, key: &str, source: &str, revision: &str, summary: &str) -> DbResult<()> {
        self.conn.execute("INSERT INTO content_index_checkpoints(fingerprint,data_source_id,source_revision,summary_json) VALUES(?1,?2,?3,?4) ON CONFLICT(fingerprint) DO NOTHING",params![key,source,revision,summary])?;
        Ok(())
    }
    pub fn save_page(
        &self,
        key: &str,
        after: &str,
        summary: &str,
        entries: &[(String, String)],
    ) -> DbResult<()> {
        let tx = self.conn.unchecked_transaction()?;
        for (file, status) in entries {
            tx.execute("INSERT INTO content_index_entries(fingerprint,file_id,status) VALUES(?1,?2,?3) ON CONFLICT(fingerprint,file_id) DO UPDATE SET status=excluded.status",params![key,file,status])?;
        }
        tx.execute(
            "UPDATE content_index_checkpoints SET after_id=?2,summary_json=?3 WHERE fingerprint=?1",
            params![key, after, summary],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn publish(&self, key: &str) -> DbResult<()> {
        self.conn.execute(
            "UPDATE content_index_checkpoints SET published=1 WHERE fingerprint=?1",
            [key],
        )?;
        Ok(())
    }
    pub fn artifact_text(&self, source: &str, local: &str, global: &str) -> DbResult<String> {
        if crate::detail::source_id(self.conn)?.is_some() {
            return self.native_artifact_text(source, local, global);
        }
        Err(persistence_sqlite::DbError::System(
            "source identity is missing".into(),
        ))
    }

    fn native_artifact_text(&self, source: &str, local: &str, global: &str) -> DbResult<String> {
        let owner = crate::detail::source_id(self.conn)?.ok_or_else(|| {
            persistence_sqlite::DbError::System("source identity is missing".into())
        })?;
        if owner != source {
            return Err(persistence_sqlite::DbError::System(
                "content index artifact source owner mismatch".into(),
            ));
        }
        let store = crate::detail::require_source_store(self.conn)?;
        let repository = ArtifactRecordStore::from_source_store((*store).clone());
        let snapshot = repository.read_snapshot();
        let mut result = String::new();
        let mut rows = 0usize;
        for object_id in [local, global] {
            let mut cursor = None;
            loop {
                persistence_sqlite::read_operation::ensure_active()?;
                let page = repository
                    .list_by_source_object(
                        &snapshot,
                        source,
                        object_id,
                        cursor.as_deref(),
                        ScanDirection::Forward,
                        128,
                    )
                    .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
                for record in page.records {
                    if rows == 128 {
                        return Ok(result);
                    }
                    let artifact = super::artifact_repo::decode_fjall_artifact(record)?;
                    let text = format!(
                        "{} {} {}",
                        artifact.title,
                        artifact.summary,
                        serde_json::to_string(&artifact.attrs).map_err(|error| {
                            persistence_sqlite::DbError::System(error.to_string())
                        })?
                    );
                    let text = text.chars().take(65_536).collect::<String>();
                    if result.len().saturating_add(text.len()) > 1024 * 1024 {
                        return Ok(result);
                    }
                    result.push_str(&text);
                    result.push('\n');
                    rows += 1;
                }
                cursor = page.next_cursor;
                if cursor.is_none() {
                    break;
                }
            }
        }
        Ok(result)
    }
}
