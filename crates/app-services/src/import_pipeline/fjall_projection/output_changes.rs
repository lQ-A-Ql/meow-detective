use crate::storage_runtime::ReadWriteSourceStorageHandle;
use persistence_sqlite::repositories::source_projection_change_repo::{
    SourceProjectionChangeRepo, SourceProjectionEntity,
};
use rusqlite::Connection;
use std::collections::BTreeSet;
use transport::CommandError;

const CHANGE_PAGE_SIZE: usize = 512;

#[allow(clippy::too_many_arguments)]
pub(super) fn mirror_analysis_output_changes(
    case_connection: &Connection,
    case_root: &std::path::Path,
    case_id: &str,
    source_connection: &Connection,
    source: &ReadWriteSourceStorageHandle,
    source_id: &str,
    mut after_revision: u64,
    through_revision: u64,
) -> Result<(), CommandError> {
    let repo = SourceProjectionChangeRepo::new(source_connection);
    loop {
        let changes = repo
            .list_after(after_revision, through_revision, CHANGE_PAGE_SIZE)
            .map_err(|error| CommandError::internal(error.to_string()))?;
        if changes.is_empty() {
            break;
        }
        let mut artifact_ids = BTreeSet::new();
        let mut timeline_ids = BTreeSet::new();
        for change in &changes {
            if change.source_id != source_id {
                return Err(CommandError::internal(format!(
                    "projection change for source '{}' reached source '{}'",
                    change.source_id, source_id
                )));
            }
            match change.entity {
                SourceProjectionEntity::Artifact => {
                    artifact_ids.insert(change.entity_id.clone());
                }
                SourceProjectionEntity::Timeline => {
                    timeline_ids.insert(change.entity_id.clone());
                }
            }
        }
        super::projections::mirror_artifact_ids(
            case_connection,
            case_root,
            case_id,
            source_connection,
            source,
            source_id,
            &artifact_ids.into_iter().collect::<Vec<_>>(),
        )?;
        super::projections::mirror_timeline_ids(
            source_connection,
            source,
            source_id,
            &timeline_ids.into_iter().collect::<Vec<_>>(),
        )?;
        after_revision = changes
            .last()
            .map_or(after_revision, |change| change.revision);
    }
    Ok(())
}
