use crate::connection::DbResult;
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxImportSetRecord {
    pub id: String,
    pub case_id: String,
    pub name: String,
    pub root_path: String,
    pub import_state: String,
    pub member_count: u32,
    pub ready_count: u32,
    pub failed_count: u32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinuxImportSetMemberRecord {
    pub import_set_id: String,
    pub member_index: u32,
    pub source_path: String,
    pub source_kind: String,
    pub data_source_id: Option<String>,
    pub import_state: String,
    pub last_error: Option<String>,
}

pub struct LinuxImportSetRepo<'a> {
    conn: &'a Connection,
}

impl<'a> LinuxImportSetRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, record: &LinuxImportSetRecord) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO linux_import_sets (
                id, case_id, name, root_path, import_state, member_count,
                ready_count, failed_count, last_error
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                record.id,
                record.case_id,
                record.name,
                record.root_path,
                record.import_state,
                record.member_count,
                record.ready_count,
                record.failed_count,
                record.last_error,
            ],
        )?;
        Ok(())
    }

    pub fn insert_member(&self, record: &LinuxImportSetMemberRecord) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO linux_import_set_members (
                import_set_id, member_index, source_path, source_kind,
                data_source_id, import_state, last_error
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                record.import_set_id,
                record.member_index,
                record.source_path,
                record.source_kind,
                record.data_source_id,
                record.import_state,
                record.last_error,
            ],
        )?;
        Ok(())
    }

    pub fn bind_member_source(
        &self,
        import_set_id: &str,
        member_index: u32,
        data_source_id: &str,
    ) -> DbResult<()> {
        let affected = self.conn.execute(
            "UPDATE linux_import_set_members
             SET data_source_id = ?1, import_state = 'importing'
             WHERE import_set_id = ?2 AND member_index = ?3",
            params![data_source_id, import_set_id, member_index],
        )?;
        if affected != 1 {
            return Err(crate::connection::DbError::System(format!(
                "linux import-set member not found: {import_set_id}/{member_index}"
            )));
        }
        Ok(())
    }

    pub fn update_state(
        &self,
        import_set_id: &str,
        import_state: &str,
        ready_count: u32,
        failed_count: u32,
        last_error: Option<&str>,
    ) -> DbResult<()> {
        let affected = self.conn.execute(
            "UPDATE linux_import_sets
             SET import_state = ?1, ready_count = ?2, failed_count = ?3,
                 last_error = ?4, updated_at = datetime('now')
             WHERE id = ?5",
            params![
                import_state,
                ready_count,
                failed_count,
                last_error,
                import_set_id
            ],
        )?;
        if affected != 1 {
            return Err(crate::connection::DbError::System(format!(
                "linux import set not found: {import_set_id}"
            )));
        }
        Ok(())
    }

    pub fn update_member_state_by_source(
        &self,
        data_source_id: &str,
        import_state: &str,
        last_error: Option<&str>,
    ) -> DbResult<()> {
        let affected = self.conn.execute(
            "UPDATE linux_import_set_members
             SET import_state = ?1, last_error = ?2
             WHERE data_source_id = ?3",
            params![import_state, last_error, data_source_id],
        )?;
        if affected != 1 {
            return Err(crate::connection::DbError::System(format!(
                "linux import-set member is not bound to data source: {data_source_id}"
            )));
        }
        Ok(())
    }

    pub fn has_member_source(&self, data_source_id: &str) -> DbResult<bool> {
        Ok(self.conn.query_row(
            "SELECT EXISTS(
                 SELECT 1 FROM linux_import_set_members
                 WHERE data_source_id = ?1
             )",
            [data_source_id],
            |row| row.get(0),
        )?)
    }

    pub fn find_member_by_source(
        &self,
        data_source_id: &str,
    ) -> DbResult<Option<LinuxImportSetMemberRecord>> {
        self.conn
            .query_row(
                "SELECT import_set_id, member_index, source_path, source_kind,
                        data_source_id, import_state, last_error
                 FROM linux_import_set_members
                 WHERE data_source_id = ?1",
                [data_source_id],
                |row| {
                    Ok(LinuxImportSetMemberRecord {
                        import_set_id: row.get(0)?,
                        member_index: row.get::<_, i64>(1)?.max(0) as u32,
                        source_path: row.get(2)?,
                        source_kind: row.get(3)?,
                        data_source_id: row.get(4)?,
                        import_state: row.get(5)?,
                        last_error: row.get(6)?,
                    })
                },
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn find_members(&self, import_set_id: &str) -> DbResult<Vec<LinuxImportSetMemberRecord>> {
        let mut statement = self.conn.prepare(
            "SELECT import_set_id, member_index, source_path, source_kind,
                    data_source_id, import_state, last_error
             FROM linux_import_set_members
             WHERE import_set_id = ?1
             ORDER BY member_index ASC",
        )?;
        let rows = statement.query_map([import_set_id], |row| {
            Ok(LinuxImportSetMemberRecord {
                import_set_id: row.get(0)?,
                member_index: row.get::<_, i64>(1)?.max(0) as u32,
                source_path: row.get(2)?,
                source_kind: row.get(3)?,
                data_source_id: row.get(4)?,
                import_state: row.get(5)?,
                last_error: row.get(6)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }
}
