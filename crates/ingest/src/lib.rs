//! Ingestion pipeline orchestration.
//!
//! TODO: Implement pipeline coordination for multi-source ingest,
//! progress aggregation, and cancellation. Currently all ingest
//! logic lives in `app-services::file_service`.

pub fn crate_name() -> &'static str {
    "ingest"
}
