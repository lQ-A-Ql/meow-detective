mod audit;
mod copy;
mod destination;
pub(crate) mod policy;
mod progress;

pub use crate::file_service::metadata::source_extraction::extract_file_to_destination_for_case;
pub use crate::file_service::metadata::source_extraction::extract_file_to_destination_for_case_with_bitlocker;
pub use crate::file_service::metadata::source_extraction::extract_file_to_destination_for_case_with_bitlocker_and_progress;
pub use crate::file_service::metadata::source_extraction::CaseFileExtractionRequest;
pub use audit::record_file_extraction_audit;
pub use destination::extract_file_to_destination;
pub(crate) use destination::{extract_source_file, SourceExtractionRequest};
pub use progress::{
    FileExtractionProgressCallback, FileExtractionProgressPhase, FileExtractionProgressUpdate,
};
