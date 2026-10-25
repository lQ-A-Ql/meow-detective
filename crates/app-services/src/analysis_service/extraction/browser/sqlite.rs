use super::super::ExtractionOutcome;
use crate::analysis_service::error::AnalysisServiceError;
use rusqlite::Connection;

pub(super) fn with_database(
    bytes: &[u8],
    _prefix: &str,
    parse: impl FnOnce(&Connection) -> Result<ExtractionOutcome, AnalysisServiceError>,
) -> Result<ExtractionOutcome, AnalysisServiceError> {
    let connection = artifacts_browser::sqlite::open_sqlite_from_bytes(bytes)?;
    parse(&connection)
}
