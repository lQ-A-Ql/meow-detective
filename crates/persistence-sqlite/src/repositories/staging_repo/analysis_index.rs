use super::StagingRepo;
use rusqlite::{params, Connection};

impl StagingRepo {
    pub fn read_analysis_index_docs_page(
        conn: &Connection,
        limit: i64,
        offset: i64,
    ) -> rusqlite::Result<Vec<(String, String, String, String)>> {
        let mut stmt = conn.prepare(
            "SELECT file_id, path, text, language FROM index_docs WHERE text <> '' ORDER BY file_id LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit, offset], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?;
        rows.collect()
    }
}
