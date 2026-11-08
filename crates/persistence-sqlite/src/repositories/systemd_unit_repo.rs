use crate::DbResult;
use persistence_fjall::{CatalogEntryType, CatalogRecordStore};
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
        if crate::source_fjall::source_id(self.conn)?.is_some() {
            return self.native_catalog_page(source, after);
        }
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

    fn native_catalog_page(&self, source: &str, after: &str) -> DbResult<Vec<SystemdCatalogRow>> {
        let owner = crate::source_fjall::source_id(self.conn)?
            .ok_or_else(|| crate::DbError::System("source identity is missing".into()))?;
        if owner != source {
            return Err(crate::DbError::System(
                "systemd catalog source owner mismatch".into(),
            ));
        }
        let store = crate::source_fjall::require_source_store(self.conn)?;
        let catalog = CatalogRecordStore::from_source_store((*store).clone());
        let snapshot = catalog.read_snapshot();
        let mut cursor = (!after.is_empty()).then(|| after.to_owned());
        let mut rows = Vec::with_capacity(512);
        loop {
            crate::read_operation::ensure_active()?;
            let page = catalog
                .list_by_id(&snapshot, source, cursor.as_deref(), 512)
                .map_err(|error| crate::DbError::System(error.to_string()))?;
            if page.records.is_empty() {
                break;
            }
            for record in &page.records {
                cursor = Some(record.id.clone());
                if !systemd_catalog_record(record) {
                    continue;
                }
                rows.push(SystemdCatalogRow {
                    file_id: record.id.clone(),
                    source_id: record.data_source_id.clone(),
                    partition_index: record.partition_index,
                    path: record.path.clone(),
                    name: record.name.clone(),
                    size: record.size.unwrap_or_default(),
                    unix_mode: record.unix_mode,
                    encrypted: record.encrypted.unwrap_or(true),
                });
                if rows.len() == 512 {
                    return Ok(rows);
                }
            }
            if page.next_cursor.is_none() {
                break;
            }
        }
        Ok(rows)
    }
}

fn systemd_catalog_record(record: &persistence_fjall::FileEntryRecord) -> bool {
    let path = record.path.replace('\\', "/").to_ascii_lowercase();
    !record.data_source_id.is_empty()
        && !record.deleted
        && (path.contains("/systemd/")
            || matches!(
                record.name.to_ascii_lowercase().as_str(),
                "lib" | "usr" | "local" | "etc" | "run" | "systemd"
            ))
        && (record.entry_type == CatalogEntryType::File
            || record.entry_type == CatalogEntryType::Directory)
}
