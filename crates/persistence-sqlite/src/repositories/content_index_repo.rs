use crate::DbResult;
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
        let mut statement=self.conn.prepare("SELECT substr(title || ' ' || summary || ' ' || attrs,1,65536) FROM artifacts WHERE data_source_id=?1 AND source_object_id IN (?2,?3) ORDER BY id LIMIT 128")?;
        let rows = statement.query_map(params![source, local, global], |row| {
            row.get::<_, String>(0)
        })?;
        let mut result = String::new();
        for row in rows {
            let text = row?;
            if result.len() + text.len() > 1024 * 1024 {
                break;
            }
            result.push_str(&text);
            result.push('\n');
        }
        Ok(result)
    }
}
