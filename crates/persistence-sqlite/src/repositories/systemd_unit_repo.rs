use crate::DbResult;
use rusqlite::{params, Connection};
pub struct SystemdCatalogRow {
    pub file_id: String,
    pub source_id: String,
    pub partition_index: Option<u32>,
    pub path: String,
    pub name: String,
    pub size: u64,
    pub unix_mode: Option<u32>,
    pub encrypted: bool,
}
pub struct SystemdUnitRepo<'a> {
    conn: &'a Connection,
}
impl<'a> SystemdUnitRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
    pub fn catalog_page(&self, source: &str, after: &str) -> DbResult<Vec<SystemdCatalogRow>> {
        let mut statement=self.conn.prepare("SELECT id,data_source_id,partition_index,path,name,COALESCE(size,0),unix_mode,encrypted FROM file_entries WHERE data_source_id=?1 AND id>?2 AND deleted=0 AND (replace(path,char(92),'/') LIKE '%/systemd/%' OR name IN ('lib','usr','local','etc','run','systemd')) ORDER BY id LIMIT 512")?;
        let rows = statement.query_map(params![source, after], |row| {
            Ok(SystemdCatalogRow {
                file_id: row.get(0)?,
                source_id: row.get(1)?,
                partition_index: row.get(2)?,
                path: row.get(3)?,
                name: row.get(4)?,
                size: row.get(5)?,
                unix_mode: row.get(6)?,
                encrypted: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}
