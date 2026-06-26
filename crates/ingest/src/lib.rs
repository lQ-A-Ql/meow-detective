//! # Ingest Pipeline (DEPRECATED)
//!
//! This crate defines the IngestPipeline trait and related types for evidence ingestion.
//!
//! **DEPRECATED**: The production ingestion path lives in
//! pps/desktop/src-tauri/src/commands/import/pipeline.rs.
//! This crate is retained for reference but is not used by any production code.
//! Scheduled for removal in a future cleanup pass after confirming no external
//! consumers depend on the trait shapes defined here.
//!
//! Ingestion pipeline orchestration.
//!
//! Defines the trait-based pipeline for importing data sources into a case:
//! - Source classification (image vs logical directory)
//! - Partition enumeration and filesystem traversal
//! - File entry extraction and persistence
//! - Post-import pipeline (timeline projection, artifact extraction, text indexing)

pub mod config;
pub mod graph_writer;
pub mod pipeline;
pub mod sink;
pub mod stats;

pub use config::IngestConfig;
pub use graph_writer::{GraphWriter, SqliteGraphWriter};
pub use pipeline::IngestPipeline;
pub use sink::IngestSink;
pub use stats::IngestStats;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_source_e01() {
        let path = std::path::Path::new("/evidence/disk.E01");
        assert_eq!(
            pipeline::classify_source(path),
            pipeline::SourceType::E01Image
        );
    }

    #[test]
    fn classify_source_e01_lowercase() {
        let path = std::path::Path::new("/evidence/disk.e01");
        assert_eq!(
            pipeline::classify_source(path),
            pipeline::SourceType::E01Image
        );
    }

    #[test]
    fn classify_source_raw_dd() {
        let path = std::path::Path::new("/evidence/disk.dd");
        assert_eq!(
            pipeline::classify_source(path),
            pipeline::SourceType::RawImage
        );
    }

    #[test]
    fn classify_source_raw_raw() {
        let path = std::path::Path::new("/evidence/disk.raw");
        assert_eq!(
            pipeline::classify_source(path),
            pipeline::SourceType::RawImage
        );
    }

    #[test]
    fn classify_source_raw_img() {
        let path = std::path::Path::new("/evidence/disk.img");
        assert_eq!(
            pipeline::classify_source(path),
            pipeline::SourceType::RawImage
        );
    }

    #[test]
    fn classify_source_unknown_file() {
        let path = std::path::Path::new("/evidence/notes.txt");
        assert_eq!(
            pipeline::classify_source(path),
            pipeline::SourceType::Unknown
        );
    }

    #[test]
    fn classify_source_directory() {
        let tmp = tempfile::TempDir::new().unwrap();
        assert_eq!(
            pipeline::classify_source(tmp.path()),
            pipeline::SourceType::LogicalDirectory
        );
    }

    #[test]
    fn ingest_stats_merge() {
        let mut a = IngestStats {
            files_enumerated: 10,
            dirs_enumerated: 2,
            bytes_processed: 1000,
            partitions_detected: 1,
            partitions_processed: 1,
            timeline_events: 5,
            artifacts_extracted: 3,
            warning_count: 1,
            skipped_count: 0,
            failed_count: 0,
        };
        let b = IngestStats {
            files_enumerated: 20,
            dirs_enumerated: 4,
            bytes_processed: 2000,
            partitions_detected: 1,
            partitions_processed: 1,
            timeline_events: 10,
            artifacts_extracted: 7,
            warning_count: 2,
            skipped_count: 1,
            failed_count: 1,
        };
        a.merge(&b);
        assert_eq!(a.files_enumerated, 30);
        assert_eq!(a.dirs_enumerated, 6);
        assert_eq!(a.bytes_processed, 3000);
        assert_eq!(a.partitions_detected, 2);
        assert_eq!(a.partitions_processed, 2);
        assert_eq!(a.timeline_events, 15);
        assert_eq!(a.artifacts_extracted, 10);
        assert_eq!(a.warning_count, 3);
        assert_eq!(a.skipped_count, 1);
        assert_eq!(a.failed_count, 1);
    }

    #[test]
    fn ingest_stats_default() {
        let stats = IngestStats::default();
        assert_eq!(stats.files_enumerated, 0);
        assert_eq!(stats.dirs_enumerated, 0);
        assert_eq!(stats.bytes_processed, 0);
        assert_eq!(stats.partitions_detected, 0);
        assert_eq!(stats.partitions_processed, 0);
        assert_eq!(stats.timeline_events, 0);
        assert_eq!(stats.artifacts_extracted, 0);
        assert_eq!(stats.warning_count, 0);
        assert_eq!(stats.skipped_count, 0);
        assert_eq!(stats.failed_count, 0);
    }

    #[test]
    fn ingest_config_default() {
        let config = IngestConfig::default();
        assert_eq!(config.source_path, std::path::PathBuf::new());
        assert_eq!(config.case_id, "");
        assert_eq!(config.data_source_id, "");
        assert_eq!(config.artifact_file_limit_bytes, 64 * 1024 * 1024);
        assert!(config.enable_text_indexing);
        assert!(config.enable_timeline_projection);
        assert!(config.enable_artifact_extraction);
    }
}
