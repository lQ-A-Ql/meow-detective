use crate::{source_db, storage_recovery_service::StorageRecoveryError};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use persistence_sqlite::repositories::storage_control_repo::{
    StorageControlRepo, StorageOperationState,
};
use rusqlite::Connection;
use std::path::Path;

pub(super) fn finish_import_recovery(
    connection: &Connection,
    case_root: &Path,
    source_id: &str,
    operation_id: &str,
) -> Result<(), StorageRecoveryError> {
    let source_id = domain::DataSourceId(source_id.to_owned());
    let source = source_db::open_registered_source_db_read_only(connection, case_root, &source_id)?;
    let target = persistence_source::control::import_state::read_import_target(&source)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(persistence_sqlite::DbError::from)?;
    DataSourceRepo::new(&transaction).update_import_state(&source_id, target.as_str(), None)?;
    StorageControlRepo::new(&transaction).update_operation_state(
        operation_id,
        StorageOperationState::Published,
        &chrono::Utc::now().to_rfc3339(),
        None,
    )?;
    transaction
        .commit()
        .map_err(persistence_sqlite::DbError::from)?;
    Ok(())
}
