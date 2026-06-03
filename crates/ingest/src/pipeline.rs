use crate::config::IngestConfig;
use crate::sink::IngestSink;
use crate::stats::IngestStats;

/// Ingestion pipeline trait.
///
/// Implementations coordinate the full import flow:
/// source classification → partition enumeration → filesystem traversal →
/// file entry extraction → post-import processing.
pub trait IngestPipeline {
    /// Run the ingestion pipeline.
    fn run(&self, config: &IngestConfig, sink: &mut dyn IngestSink) -> Result<IngestStats, String>;

    /// Classify the source type (image, directory, unknown).
    fn classify_source(&self, path: &std::path::Path) -> SourceType;
}

/// Type of data source being ingested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceType {
    /// E01 forensic image.
    E01Image,
    /// Raw/dd forensic image.
    RawImage,
    /// Logical directory.
    LogicalDirectory,
    /// Unknown or unsupported format.
    Unknown,
}

/// Classify a source path by its extension and magic bytes.
pub fn classify_source(path: &std::path::Path) -> SourceType {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "e01" => SourceType::E01Image,
        "dd" | "raw" | "img" | "bin" => SourceType::RawImage,
        _ => {
            if path.is_dir() {
                SourceType::LogicalDirectory
            } else {
                SourceType::Unknown
            }
        }
    }
}
