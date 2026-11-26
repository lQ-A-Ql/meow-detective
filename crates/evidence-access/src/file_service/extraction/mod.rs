mod audit;
mod copy;
mod destination;
mod outcome;
mod parallel;
pub(crate) mod policy;
mod progress;

pub use crate::file_service::metadata::source_extraction::extract_file_to_destination_for_case;
pub use crate::file_service::metadata::source_extraction::extract_file_to_destination_for_case_with_bitlocker;
pub use crate::file_service::metadata::source_extraction::extract_file_to_destination_for_case_with_bitlocker_and_progress;
pub use crate::file_service::metadata::source_extraction::CaseFileExtractionRequest;
pub use audit::record_file_extraction_audit;
pub use destination::extract_file_to_destination;
pub fn prepare_case_managed_export_destination(
    connection: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &domain::CaseId,
    destination: &std::path::Path,
) -> Result<std::path::PathBuf, super::FileServiceError> {
    policy::prepare_destination(
        destination,
        false,
        policy::DestinationScope::CaseManaged {
            case_conn: connection,
            case_root,
            case_id,
        },
    )
}
pub fn prepare_case_export_destination(
    connection: &rusqlite::Connection,
    case_root: &std::path::Path,
    case_id: &domain::CaseId,
    destination: &std::path::Path,
    overwrite: bool,
) -> Result<std::path::PathBuf, super::FileServiceError> {
    policy::prepare_destination(
        destination,
        overwrite,
        policy::DestinationScope::ExternalCase {
            case_conn: connection,
            case_root,
            case_id,
        },
    )
}
pub(crate) use destination::{extract_source_file, SourceExtractionRequest};
pub use outcome::extract_file_for_case_with_audit;
pub use progress::{
    FileExtractionProgressCallback, FileExtractionProgressPhase, FileExtractionProgressUpdate,
};
