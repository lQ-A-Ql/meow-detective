use crate::{DbError, DbResult};
use rusqlite::{params, Connection, OptionalExtension};

pub struct McpTaskBindingRepo<'a> {
    conn: &'a Connection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpTaskBinding {
    pub case_id: String,
    pub case_session_id: String,
    pub principal_id: String,
    pub operation: String,
    pub idempotency_key: String,
    pub arguments_sha256: String,
    pub task_id: String,
}

impl<'a> McpTaskBindingRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn find(
        &self,
        case_id: &str,
        principal_id: &str,
        operation: &str,
        idempotency_key: &str,
    ) -> DbResult<Option<McpTaskBinding>> {
        self.conn
            .query_row(
                "SELECT case_id, case_session_id, principal_id, operation,
                        idempotency_key, arguments_sha256, task_id
                 FROM mcp_task_bindings
                 WHERE case_id=?1 AND principal_id=?2 AND operation=?3 AND idempotency_key=?4",
                params![case_id, principal_id, operation, idempotency_key],
                read,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn find_owned_task(
        &self,
        case_id: &str,
        case_session_id: &str,
        principal_id: &str,
        task_id: &str,
    ) -> DbResult<bool> {
        self.conn
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM mcp_task_bindings
                    WHERE case_id=?1 AND case_session_id=?2
                      AND principal_id=?3 AND task_id=?4
                )",
                params![case_id, case_session_id, principal_id, task_id],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }

    pub fn insert(&self, binding: &McpTaskBinding) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO mcp_task_bindings(
                case_id, case_session_id, principal_id, operation,
                idempotency_key, arguments_sha256, task_id, created_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,datetime('now'))",
            params![
                binding.case_id,
                binding.case_session_id,
                binding.principal_id,
                binding.operation,
                binding.idempotency_key,
                binding.arguments_sha256,
                binding.task_id,
            ],
        )?;
        Ok(())
    }

    pub fn insert_or_get(&self, binding: &McpTaskBinding) -> DbResult<McpTaskBinding> {
        match self.insert(binding) {
            Ok(()) => Ok(binding.clone()),
            Err(DbError::Sqlite(rusqlite::Error::SqliteFailure(_, _))) => self
                .find(
                    &binding.case_id,
                    &binding.principal_id,
                    &binding.operation,
                    &binding.idempotency_key,
                )?
                .ok_or_else(|| DbError::System("MCP task binding conflict".into())),
            Err(error) => Err(error),
        }
    }
}

fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<McpTaskBinding> {
    Ok(McpTaskBinding {
        case_id: row.get(0)?,
        case_session_id: row.get(1)?,
        principal_id: row.get(2)?,
        operation: row.get(3)?,
        idempotency_key: row.get(4)?,
        arguments_sha256: row.get(5)?,
        task_id: row.get(6)?,
    })
}

#[cfg(test)]
#[path = "../../tests/unit/repositories/mcp_task_binding_repo.rs"]
mod tests;
