use domain::DataSourceId;
use persistence_source::fjall::{ParserRecordStore, PersistMode};
use std::{path::Path, sync::atomic::AtomicBool};

use super::{
    model::{AndroidFact, AndroidPackage},
    records,
};
use crate::analysis_service::{cancellation::ensure_not_cancelled, AnalysisServiceError};

pub(super) fn persist_android_analysis(
    case_root: &Path,
    data_source_id: &DataSourceId,
    facts: &[AndroidFact],
    packages: &[AndroidPackage],
    cancel: &AtomicBool,
) -> Result<(), AnalysisServiceError> {
    ensure_not_cancelled(cancel)?;
    let source = source_kernel::source_db::open_registered_fjall_source_read_write(
        case_root,
        data_source_id,
    )
    .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    let mut output = Vec::with_capacity(facts.len().saturating_add(packages.len()));
    for fact in facts {
        ensure_not_cancelled(cancel)?;
        output.push(records::fact_record(fact, data_source_id)?);
    }
    for package in packages {
        ensure_not_cancelled(cancel)?;
        output.push(records::package_record(package, data_source_id)?);
    }
    ParserRecordStore::from_source_store(source.source_store().clone())
        .replace_family_with_control(
            &data_source_id.0,
            records::FAMILY,
            &output,
            PersistMode::SyncAll,
            cancel,
            |_| {},
        )
        .map(|_| ())
        .map_err(|error| match error {
            persistence_source::fjall::FjallStorageError::Cancelled => {
                AnalysisServiceError::Cancelled
            }
            error => AnalysisServiceError::Other(error.to_string()),
        })
}
