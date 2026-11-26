mod aggregation;
mod fields;

use crate::analysis_service::AnalysisServiceError;
use persistence_source::fjall::{
    encode_key, ArtifactRecordStore, KeyPart, KeyspaceName, PersistMode, ScanDirection,
    SourceStore, VersionedEnvelope,
};
use std::sync::atomic::AtomicBool;
use transport::dto::ReportSourceSummaryDto;

pub fn summary_key(source_id: &str) -> Result<Vec<u8>, AnalysisServiceError> {
    encode_key("report-summary", &[KeyPart::Text(source_id.to_owned())])
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))
}

pub(crate) fn rebuild(
    store: &SourceStore,
    source_id: &str,
    cancel: &AtomicBool,
) -> Result<(), AnalysisServiceError> {
    let repository = ArtifactRecordStore::from_source_store(store.clone());
    let snapshot = repository.read_snapshot();
    let mut summary = aggregation::Summary::default();
    for family in fields::FAMILIES {
        let mut cursor = None;
        loop {
            crate::analysis_service::ensure_not_cancelled(cancel)?;
            let page = repository
                .list_by_type(
                    &snapshot,
                    source_id,
                    family,
                    cursor.as_deref(),
                    ScanDirection::Forward,
                    256,
                )
                .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
            for record in page.records {
                summary.push(&record.artifact_type, &record.attrs)?;
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
    }
    let data: ReportSourceSummaryDto = summary.finish();
    let envelope =
        VersionedEnvelope::from_serializable("report_summary", 1, source_id, source_id, 1, &data)
            .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    store
        .write_envelope(
            KeyspaceName::Meta,
            summary_key(source_id)?,
            &envelope,
            PersistMode::Buffer,
        )
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/report_summary.rs"]
mod tests;
