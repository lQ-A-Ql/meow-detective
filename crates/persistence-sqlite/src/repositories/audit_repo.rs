//! 审计日志仓储。
//!
//! 记录和查询用户或系统在案件中的关键操作，用于安全边界复核、
//! MCP 调用留痕、导出留痕与问题追溯。

use crate::connection::DbResult;
use crate::sql_builder::ClauseBuilder;
use rusqlite::{params, Connection};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AuditLogEntry {
    pub id: String,
    pub case_id: Option<String>,
    pub user_id: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub details: String,
    pub ip_address: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy)]
pub enum AuditAction {
    CaseCreate,
    CaseOpen,
    CaseClose,
    CaseDelete,
    DataSourceImport,
    DataSourceDelete,
    DataSourceRename,
    BitLockerUnlock,
    BitLockerLock,
    BitLockerKeyRestore,
    BitLockerKeyForget,
    BitLockerCatalogImport,
    FileView,
    FileExtract,
    ImageMount,
    ImageUnmount,
    EmulationPrepare,
    EmulationLaunch,
    EmulationRelease,
    SearchExecute,
    ReportGenerate,
    ReportExport,
    ArtifactView,
    TimelineView,
    McpConnect,
    McpDisconnect,
    McpTest,
    McpResourceList,
    McpResourceRead,
    McpToolList,
    McpToolCall,
    McpPromptList,
    McpPromptGet,
}

impl AuditAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CaseCreate => "case.create",
            Self::CaseOpen => "case.open",
            Self::CaseClose => "case.close",
            Self::CaseDelete => "case.delete",
            Self::DataSourceImport => "datasource.import",
            Self::DataSourceDelete => "datasource.delete",
            Self::DataSourceRename => "datasource.rename",
            Self::BitLockerUnlock => "bitlocker.unlock",
            Self::BitLockerLock => "bitlocker.lock",
            Self::BitLockerKeyRestore => "bitlocker.key.restore",
            Self::BitLockerKeyForget => "bitlocker.key.forget",
            Self::BitLockerCatalogImport => "bitlocker.catalog.import",
            Self::FileView => "file.view",
            Self::FileExtract => "file.extract",
            Self::ImageMount => "image.mount",
            Self::ImageUnmount => "image.unmount",
            Self::EmulationPrepare => "emulation.prepare",
            Self::EmulationLaunch => "emulation.launch",
            Self::EmulationRelease => "emulation.release",
            Self::SearchExecute => "search.execute",
            Self::ReportGenerate => "report.generate",
            Self::ReportExport => "report.export",
            Self::ArtifactView => "artifact.view",
            Self::TimelineView => "timeline.view",
            Self::McpConnect => "mcp.connect",
            Self::McpDisconnect => "mcp.disconnect",
            Self::McpTest => "mcp.test",
            Self::McpResourceList => "mcp.resource.list",
            Self::McpResourceRead => "mcp.resource.read",
            Self::McpToolList => "mcp.tool.list",
            Self::McpToolCall => "mcp.tool.call",
            Self::McpPromptList => "mcp.prompt.list",
            Self::McpPromptGet => "mcp.prompt.get",
        }
    }

    pub fn resource_type(&self) -> &'static str {
        match self {
            Self::CaseCreate | Self::CaseOpen | Self::CaseClose | Self::CaseDelete => "case",
            Self::DataSourceImport | Self::DataSourceDelete | Self::DataSourceRename => {
                "datasource"
            }
            Self::BitLockerUnlock
            | Self::BitLockerLock
            | Self::BitLockerKeyRestore
            | Self::BitLockerKeyForget
            | Self::BitLockerCatalogImport => "bitlocker",
            Self::FileView | Self::FileExtract => "file",
            Self::ImageMount | Self::ImageUnmount => "mount",
            Self::EmulationPrepare | Self::EmulationLaunch | Self::EmulationRelease => "emulation",
            Self::SearchExecute => "search",
            Self::ReportGenerate | Self::ReportExport => "report",
            Self::ArtifactView => "artifact",
            Self::TimelineView => "timeline",
            Self::McpConnect
            | Self::McpDisconnect
            | Self::McpTest
            | Self::McpResourceList
            | Self::McpResourceRead
            | Self::McpToolList
            | Self::McpToolCall
            | Self::McpPromptList
            | Self::McpPromptGet => "mcp",
        }
    }
}

pub struct AuditRepo<'a> {
    conn: &'a Connection,
}

impl<'a> AuditRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn log(
        &self,
        case_id: Option<&str>,
        user_id: &str,
        action: &AuditAction,
        resource_id: Option<&str>,
        details: &str,
    ) -> DbResult<()> {
        let id = Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT INTO audit_log (id, case_id, user_id, action, resource_type, resource_id, details)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                id,
                case_id,
                user_id,
                action.as_str(),
                action.resource_type(),
                resource_id,
                details,
            ],
        )?;
        Ok(())
    }

    pub fn log_simple(
        &self,
        case_id: Option<&str>,
        action: &AuditAction,
        resource_id: Option<&str>,
    ) -> DbResult<()> {
        self.log(case_id, "system", action, resource_id, "{}")
    }

    pub fn query(
        &self,
        case_id: Option<&str>,
        action: Option<&str>,
        limit: u32,
        offset: u64,
    ) -> DbResult<Vec<AuditLogEntry>> {
        let mut builder = ClauseBuilder::new();
        if let Some(cid) = case_id {
            builder.push_eq("case_id", cid.to_string());
        }
        if let Some(act) = action {
            builder.push_eq("action", act.to_string());
        }
        let limit_param = builder.push_param(limit);
        let offset_param = builder.push_param(offset);

        let sql = format!(
            "SELECT id, case_id, user_id, action, resource_type, resource_id, details, ip_address, created_at
             FROM audit_log {}
             ORDER BY created_at DESC LIMIT ?{limit_param} OFFSET ?{offset_param}",
            builder.where_clause(),
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(builder.param_refs().as_slice(), |row| {
            Ok(AuditLogEntry {
                id: row.get(0)?,
                case_id: row.get(1)?,
                user_id: row.get(2)?,
                action: row.get(3)?,
                resource_type: row.get(4)?,
                resource_id: row.get(5)?,
                details: row.get(6)?,
                ip_address: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    pub fn count(&self, case_id: Option<&str>) -> DbResult<u64> {
        let (sql, param_values) = match case_id {
            Some(cid) => (
                "SELECT COUNT(*) FROM audit_log WHERE case_id = ?1".to_string(),
                vec![Box::new(cid.to_string()) as Box<dyn rusqlite::types::ToSql>],
            ),
            None => ("SELECT COUNT(*) FROM audit_log".to_string(), vec![]),
        };

        let mut stmt = self.conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();
        let count: i64 = stmt.query_row(params_refs.as_slice(), |r| r.get(0))?;
        Ok(count as u64)
    }

    pub fn count_by_action(&self, action: &str) -> DbResult<u64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM audit_log WHERE action = ?1",
            params![action],
            |r| r.get(0),
        )?;
        Ok(count as u64)
    }

    /// Delete audit log entries older than `days` days.
    /// Enforces a minimum retention of 365 days for forensic compliance.
    pub fn cleanup_old(&self, days: u32) -> DbResult<usize> {
        let effective_days = days.max(365);
        tracing::info!(
            requested_days = days,
            effective_days,
            "audit_log cleanup_old"
        );
        let count = self.conn.execute(
            "DELETE FROM audit_log WHERE created_at < datetime('now', ?1)",
            params![format!("-{} days", effective_days)],
        )?;
        Ok(count)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/repositories/audit_repo.rs"]
mod tests;
