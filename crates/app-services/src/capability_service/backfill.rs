use super::{kind_wire, state_wire, CapabilityServiceError};
use domain::{CaseId, DataSourcePlatform};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo,
    source_capability_repo::{SourceCapabilityRepo, SourceCapabilityRow},
};
use rusqlite::Connection;
use std::path::Path;
use transport::dto::{CapabilityKindDto, CapabilityStateDto};

pub fn backfill_case_capabilities(
    conn: &Connection,
    _root: &Path,
    case: &CaseId,
) -> Result<(), CapabilityServiceError> {
    let sources = DataSourceRepo::new(conn);
    let capabilities = SourceCapabilityRepo::new(conn);
    let now = chrono::Utc::now().to_rfc3339();
    for source in sources.find_by_case(case)? {
        let Some(storage) = sources.find_storage(&source.id)? else {
            continue;
        };
        let ready = matches!(storage.import_state.as_str(), "ready" | "ready_metadata");
        let metadata_only = storage.import_state == "ready_metadata";
        let platform = DataSourcePlatform::from_storage_str(Some(&storage.platform))
            .unwrap_or(DataSourcePlatform::Unknown);
        let supported = matches!(
            platform,
            DataSourcePlatform::Windows | DataSourcePlatform::Linux | DataSourcePlatform::Android
        );
        let catalog = if ready {
            CapabilityStateDto::Ready
        } else {
            CapabilityStateDto::Pending
        };
        let facts = match (supported, ready) {
            (true, true) => CapabilityStateDto::Ready,
            (true, false) => CapabilityStateDto::Pending,
            (false, _) => CapabilityStateDto::NotApplicable,
        };
        let artifacts = if metadata_only || !supported {
            CapabilityStateDto::NotApplicable
        } else {
            CapabilityStateDto::Pending
        };
        for (kind, state) in [
            (CapabilityKindDto::Catalog, catalog),
            (CapabilityKindDto::Search, catalog),
            (CapabilityKindDto::PlatformFacts, facts),
            (CapabilityKindDto::Artifacts, artifacts),
        ] {
            capabilities.insert_if_missing(&SourceCapabilityRow {
                case_id: case.0.clone(),
                data_source_id: source.id.0.clone(),
                capability: kind_wire(kind).into(),
                state: state_wire(state).into(),
                revision: 0,
                progress_done: 0,
                progress_total: None,
                phase: None,
                detail: None,
                error_code: None,
                derived: 1,
                updated_at: now.clone(),
            })?;
        }
    }
    Ok(())
}
