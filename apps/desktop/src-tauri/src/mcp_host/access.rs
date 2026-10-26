use super::{McpHostError, McpHostRuntime};
use crate::{commands::command_support::require_active_case, state::AppState};
use app_services::mcp_host_service::access::McpAccessRegistry;
use persistence_sqlite::repositories::audit_repo::{AuditAction, AuditRepo};
use std::io::{Read, Write};
use transport::{
    dto::mcp_host::access::{
        McpHostClientDto, McpHostCreateClientRequestDto, McpHostCreatedClientDto,
        McpHostCredentialRecordDto,
    },
    CommandError,
};

impl McpHostRuntime {
    pub fn create_client(
        &self,
        state: &AppState,
        request: McpHostCreateClientRequestDto,
    ) -> Result<McpHostCreatedClientDto, CommandError> {
        let active = require_active_case(state)?;
        let mut access = self
            .access
            .lock()
            .map_err(|_| CommandError::internal("客户端授权状态不可用"))?;
        let (created, record) = access
            .prepare(request, &active.case_id, &active.session_id)
            .map_err(CommandError::from_typed_service_error)?;
        let mut records = access.records();
        records.push(record.clone());
        audit_access(
            state,
            &AuditAction::McpAccessGrant,
            &created.client.id,
            "requested",
        )?;
        persist(state, &records).map_err(CommandError::from_typed_service_error)?;
        audit_access(
            state,
            &AuditAction::McpAccessGrant,
            &created.client.id,
            "completed",
        )?;
        access.insert(record);
        Ok(created)
    }

    pub fn list_clients(&self) -> Result<Vec<McpHostClientDto>, CommandError> {
        self.access
            .lock()
            .map(|access| access.clients())
            .map_err(|_| CommandError::internal("客户端授权状态不可用"))
    }

    pub fn revoke_client(&self, state: &AppState, id: &str) -> Result<(), CommandError> {
        let mut access = self
            .access
            .lock()
            .map_err(|_| CommandError::internal("客户端授权状态不可用"))?;
        let mut records = access.records();
        records.retain(|r| r.client.id != id);
        audit_access(state, &AuditAction::McpAccessRevoke, id, "requested")?;
        persist(state, &records).map_err(CommandError::from_typed_service_error)?;
        access.revoke(id);
        audit_access(state, &AuditAction::McpAccessRevoke, id, "completed")?;
        Ok(())
    }

    pub(super) fn load_access(&self, state: &AppState) -> Result<(), McpHostError> {
        let path = state
            .mcp_host_settings_path
            .with_file_name("mcp-access.json");
        let records = match std::fs::File::open(path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(128 * 1024 + 1).read_to_end(&mut bytes)?;
                if bytes.len() > 128 * 1024 {
                    return Err(McpHostError::SettingsTooLarge);
                }
                serde_json::from_slice(&bytes)?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        let registry = McpAccessRegistry::load(records)?;
        let mut access = self.access.lock().map_err(|_| McpHostError::State)?;
        for client in access.clients() {
            access.revoke(&client.id);
        }
        *access = registry;
        Ok(())
    }
}

fn audit_access(
    state: &AppState,
    action: &AuditAction,
    client_id: &str,
    phase: &str,
) -> Result<(), CommandError> {
    let active = crate::commands::command_support::snapshot_active_case(state)?;
    if let Some(active) = active {
        let connection = app_services::connection::open_existing_case_db(&active.db_path())
            .map_err(CommandError::from_typed_service_error)?;
        AuditRepo::new(&connection)
            .log(
                Some(&active.case_id),
                "desktop",
                action,
                Some(client_id),
                &serde_json::json!({"phase":phase}).to_string(),
            )
            .map_err(CommandError::from_typed_service_error)?;
    }
    tracing::info!(
        action = action.as_str(),
        client_id,
        phase,
        "MCP client authorization"
    );
    Ok(())
}

fn persist(state: &AppState, records: &[McpHostCredentialRecordDto]) -> Result<(), McpHostError> {
    let path = state
        .mcp_host_settings_path
        .with_file_name("mcp-access.json");
    let parent = path.parent().ok_or(McpHostError::State)?;
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    crate::platform_security::restrict_file_to_current_user(file.path())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(&serde_json::to_vec(records)?)?;
    file.as_file().sync_all()?;
    file.persist(path)
        .map_err(|error| McpHostError::Io(error.error))?;
    Ok(())
}
