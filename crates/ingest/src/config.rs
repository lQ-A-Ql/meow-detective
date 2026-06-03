use std::path::PathBuf;

/// Configuration for an ingestion run.
#[derive(Debug, Clone)]
pub struct IngestConfig {
    /// Path to the data source (image file or directory).
    pub source_path: PathBuf,
    /// Case ID to associate entries with.
    pub case_id: String,
    /// Data source ID.
    pub data_source_id: String,
    /// Maximum bytes to read per file for artifact extraction.
    pub artifact_file_limit_bytes: u64,
    /// Whether to run text indexing after enumeration.
    pub enable_text_indexing: bool,
    /// Whether to run timeline projection after enumeration.
    pub enable_timeline_projection: bool,
    /// Whether to run artifact extraction after enumeration.
    pub enable_artifact_extraction: bool,
}

impl Default for IngestConfig {
    fn default() -> Self {
        Self {
            source_path: PathBuf::new(),
            case_id: String::new(),
            data_source_id: String::new(),
            artifact_file_limit_bytes: 64 * 1024 * 1024, // 64 MB
            enable_text_indexing: true,
            enable_timeline_projection: true,
            enable_artifact_extraction: true,
        }
    }
}
