use super::RegistryBrowserError;
use crate::{
    bitlocker_runtime::BitLockerUnlockRegistry, file_service::PreviewRuntimeRegistry, source_db,
};
use domain::{CaseId, DataSourceId, FileEntry};
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, file_repo::FileRepo};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Arc};

pub struct RegistryBrowserContext<'a> {
    pub case_conn: &'a Connection,
    pub case_root: &'a Path,
    pub case_id: &'a CaseId,
    pub bitlocker: &'a Arc<BitLockerUnlockRegistry>,
    pub preview: &'a PreviewRuntimeRegistry,
}
impl RegistryBrowserContext<'_> {
    pub(super) fn validate_source(&self, id: &DataSourceId) -> Result<(), RegistryBrowserError> {
        let platform = source_db::resolve_ready_source_platform(self.case_conn, self.case_id, id)
            .map_err(|_| RegistryBrowserError::SourceUnavailable)?;
        if platform != domain::DataSourcePlatform::Windows {
            return Err(RegistryBrowserError::InvalidRequest);
        }
        Ok(())
    }
    pub(super) fn source(&self, id: &DataSourceId) -> Result<Connection, RegistryBrowserError> {
        let source = source_db::open_ready_source_read_only_by_id(
            self.case_conn,
            self.case_root,
            self.case_id,
            id,
        )
        .map_err(|_| RegistryBrowserError::SourceUnavailable)?;
        if source.platform != domain::DataSourcePlatform::Windows {
            return Err(RegistryBrowserError::InvalidRequest);
        }
        Ok(source.connection)
    }
    pub(super) fn revision(&self, id: &DataSourceId) -> Result<String, RegistryBrowserError> {
        let generation = self
            .preview
            .source_generation(&self.case_id.0, &id.0)
            .map_err(|_| RegistryBrowserError::ExpiredSession)?;
        let storage = DataSourceRepo::new(self.case_conn)
            .find_storage(id)
            .map_err(|_| RegistryBrowserError::SourceUnavailable)?
            .ok_or(RegistryBrowserError::SourceUnavailable)?;
        let source = DataSourceRepo::new(self.case_conn)
            .find_by_case(self.case_id)
            .map_err(|_| RegistryBrowserError::SourceUnavailable)?
            .into_iter()
            .find(|s| s.id == *id)
            .ok_or(RegistryBrowserError::SourceUnavailable)?;
        let text = format!(
            "{}:{}:{}:{:?}:{:?}:{}",
            self.case_id.0,
            id.0,
            source.imported_at,
            storage.source_db_rel_path,
            storage.schema_version,
            generation
        );
        Ok(hex::encode(Sha256::digest(text.as_bytes())))
    }
    pub(super) fn file(
        &self,
        conn: &Connection,
        id: &domain::FileEntryId,
    ) -> Result<FileEntry, RegistryBrowserError> {
        FileRepo::new(conn)
            .find_by_id(id)
            .map_err(|_| RegistryBrowserError::SourceUnavailable)?
            .ok_or(RegistryBrowserError::SourceUnavailable)
    }
}
