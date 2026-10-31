use super::{McpHostError, McpHostRuntime};
use crate::{commands::command_support::require_active_case, state::AppState};
use std::io::Write;
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
        persist(state, &records).map_err(CommandError::from_typed_service_error)?;
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
        records.retain(|record| record.client.id != id);
        persist(state, &records).map_err(CommandError::from_typed_service_error)?;
        access.revoke(id);
        Ok(())
    }
}

fn persist(state: &AppState, records: &[McpHostCredentialRecordDto]) -> Result<(), McpHostError> {
    let path = state
        .mcp_host_settings_path
        .with_file_name("mcp-access.json");
    let parent = path.parent().ok_or(McpHostError::State)?;
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&serde_json::to_vec(records)?)?;
    file.as_file().sync_all()?;
    file.persist(path)
        .map_err(|error| McpHostError::Io(error.error))?;
    Ok(())
}
