use super::{
    registry::touch, RegistryBrowserContext, RegistryBrowserError, RegistryBrowserRegistry,
};
use artifacts_windows::registry::browser::{BrowserKey, HiveSnapshot};
use std::{collections::HashMap, time::Instant};
use transport::dto::{RegistryBrowserSourceDto, RegistryHiveCloseRequestDto};

pub(super) struct Session {
    pub source: RegistryBrowserSourceDto,
    pub hive: HiveSnapshot,
    pub last_used: Instant,
    pub keys: HashMap<String, BrowserKey>,
    pub values: HashMap<String, (u32, u32)>,
    pub cursors: HashMap<String, (String, usize)>,
    pub metadata_bytes: usize,
}
impl Session {
    pub fn issue_key(&mut self, key: BrowserKey) -> Result<String, RegistryBrowserError> {
        if let Some((id, _)) = self
            .keys
            .iter()
            .find(|(_, known)| known.offset == key.offset && known.path == key.path)
        {
            return Ok(id.clone());
        }
        if self.keys.len() >= 16_384 {
            return Err(RegistryBrowserError::Busy);
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.charge(key.path.len() + key.name.len() + 512)?;
        self.keys.insert(id.clone(), key);
        Ok(id)
    }
    pub fn key(&self, id: &str) -> Result<BrowserKey, RegistryBrowserError> {
        self.keys
            .get(id)
            .cloned()
            .ok_or(RegistryBrowserError::InvalidRequest)
    }
    pub fn start(&self, cursor: Option<&str>, scope: &str) -> Result<usize, RegistryBrowserError> {
        match cursor {
            None => Ok(0),
            Some(cursor) => self
                .cursors
                .get(cursor)
                .filter(|(key, _)| key == scope)
                .map(|(_, at)| *at)
                .ok_or(RegistryBrowserError::InvalidRequest),
        }
    }
    pub fn cursor(
        &mut self,
        next: Option<usize>,
        scope: String,
    ) -> Result<Option<String>, RegistryBrowserError> {
        let Some(at) = next else { return Ok(None) };
        if let Some((id, _)) = self
            .cursors
            .iter()
            .find(|(_, value)| **value == (scope.clone(), at))
        {
            return Ok(Some(id.clone()));
        }
        if self.cursors.len() >= 4096 {
            return Err(RegistryBrowserError::Busy);
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.charge(scope.len() + 256)?;
        self.cursors.insert(id.clone(), (scope, at));
        Ok(Some(id))
    }
    pub fn charge(&mut self, bytes: usize) -> Result<(), RegistryBrowserError> {
        if self.metadata_bytes.saturating_add(bytes) > super::registry::METADATA_BUDGET {
            return Err(RegistryBrowserError::Busy);
        }
        self.metadata_bytes += bytes;
        Ok(())
    }
}
pub(super) fn with_session<T>(
    registry: &RegistryBrowserRegistry,
    ctx: &RegistryBrowserContext<'_>,
    id: &str,
    operation: impl FnOnce(&mut Session) -> Result<T, RegistryBrowserError>,
) -> Result<T, RegistryBrowserError> {
    let mut state = registry.lock()?;
    let session = state
        .sessions
        .get_mut(id)
        .ok_or(RegistryBrowserError::ExpiredSession)?;
    if session.source.case_id != ctx.case_id.0 {
        return Err(RegistryBrowserError::ExpiredSession);
    }
    let source = domain::DataSourceId(session.source.data_source_id.clone());
    ctx.validate_source(&source)?;
    if ctx.revision(&source)? != session.source.source_revision {
        state.sessions.remove(id);
        return Err(RegistryBrowserError::ExpiredSession);
    }
    touch(session);
    operation(session)
}
pub fn close_hive(
    registry: &RegistryBrowserRegistry,
    case_id: &str,
    request: RegistryHiveCloseRequestDto,
) -> Result<(), RegistryBrowserError> {
    let mut state = registry.lock()?;
    if state
        .sessions
        .get(&request.session_id)
        .is_some_and(|s| s.source.case_id != case_id)
    {
        return Err(RegistryBrowserError::ExpiredSession);
    }
    state.sessions.remove(&request.session_id);
    Ok(())
}
