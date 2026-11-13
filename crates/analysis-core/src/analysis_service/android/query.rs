mod device;
mod packages;
mod presentation;

use crate::analysis_service::AnalysisServiceError;
use domain::DataSourceId;
use persistence_source::fjall::{ParserRecord, ScanDirection};
use std::path::Path;

fn visit_current_kind(
    case_root: &Path,
    source_id: &DataSourceId,
    kind: &str,
    mut visit: impl FnMut(ParserRecord) -> Result<(), AnalysisServiceError>,
) -> Result<(), AnalysisServiceError> {
    let source =
        source_kernel::source_db::open_registered_fjall_source_read_only(case_root, source_id)
            .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    let snapshot = source.snapshot();
    source
        .require_projection_complete(&snapshot, &source_id.0)
        .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
    let mut cursor = None;
    loop {
        let page = source
            .parser_list_by_family(
                &snapshot,
                &source_id.0,
                super::records::FAMILY,
                cursor.as_deref(),
                ScanDirection::Forward,
                512,
            )
            .map_err(|error| AnalysisServiceError::Other(error.to_string()))?;
        for record in page.records {
            if record.data_source_id != source_id.0
                || record.family != super::records::FAMILY
                || !matches!(record.kind.as_str(), "system_fact" | "package")
                || record.status != "ready"
            {
                return Err(AnalysisServiceError::Extraction(
                    "Android family index ownership is invalid".into(),
                ));
            }
            if record.kind == kind {
                visit(record)?;
            }
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(());
        }
    }
}

pub(crate) use device::get_android_device_info;
pub(crate) use packages::get_android_package_summary;
pub(crate) use presentation::hydrate_package_presentations;
