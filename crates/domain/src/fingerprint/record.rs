use super::encoding::normalize_sha256;
use super::{ForensicMetadata, ForensicObjectType, FMD_SCHEMA_VERSION};
use crate::{Artifact, DataSource, FileEntry};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForensicFingerprint {
    pub object_id: String,
    pub case_id: Option<String>,
    pub source_id: Option<String>,
    pub object_type: ForensicObjectType,
    pub parent_object_id: Option<String>,
    pub source_locator: Option<String>,
    pub byte_length: Option<u64>,
    pub content_sha256: Option<String>,
    pub metadata_sha256: String,
    pub parser_id: Option<String>,
    pub parser_version: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl ForensicFingerprint {
    pub fn new(metadata: ForensicMetadata, content_sha256: Option<&str>) -> Self {
        let metadata_sha256 = metadata.metadata_sha256();
        Self {
            object_id: metadata.object_id,
            case_id: metadata.case_id,
            source_id: metadata.source_id,
            object_type: metadata.object_type,
            parent_object_id: metadata.parent_object_id,
            source_locator: metadata.source_locator,
            byte_length: metadata.byte_length,
            content_sha256: normalize_sha256(content_sha256),
            metadata_sha256,
            parser_id: metadata.parser_id,
            parser_version: metadata.parser_version,
            created_at: Utc::now(),
        }
    }

    pub fn metadata(&self) -> ForensicMetadata {
        ForensicMetadata {
            object_id: self.object_id.clone(),
            case_id: self.case_id.clone(),
            source_id: self.source_id.clone(),
            object_type: self.object_type,
            parent_object_id: self.parent_object_id.clone(),
            source_locator: self.source_locator.clone(),
            byte_length: self.byte_length,
            parser_id: self.parser_id.clone(),
            parser_version: self.parser_version.clone(),
        }
    }

    pub fn for_data_source(case_id: &str, source: &DataSource) -> Self {
        Self::new(
            ForensicMetadata {
                object_id: source.id.0.clone(),
                case_id: Some(case_id.to_string()),
                source_id: Some(source.id.0.clone()),
                object_type: ForensicObjectType::DataSource,
                parent_object_id: None,
                source_locator: Some(format!("reader:{}", source.kind)),
                byte_length: source.provenance.evidence_size,
                parser_id: source.provenance.reader_kind.clone(),
                parser_version: None,
            },
            source.provenance.source_hash_sha256.as_deref(),
        )
    }

    pub fn for_file_entry(entry: &FileEntry, case_id: Option<&str>) -> Self {
        Self::new(
            ForensicMetadata {
                object_id: entry.id.0.clone(),
                case_id: case_id.map(str::to_string),
                source_id: Some(entry.data_source_id.0.clone()),
                object_type: ForensicObjectType::FileEntry,
                parent_object_id: entry.parent_id.as_ref().map(|id| id.0.clone()),
                source_locator: Some(entry.path.clone()),
                byte_length: entry.size,
                parser_id: None,
                parser_version: None,
            },
            entry.hash_sha256.as_deref(),
        )
    }

    pub fn for_artifact(artifact: &Artifact, case_id: &str, source_id: &str) -> Self {
        Self::new(
            ForensicMetadata {
                object_id: artifact.id.0.clone(),
                case_id: Some(case_id.to_string()),
                source_id: Some(source_id.to_string()),
                object_type: ForensicObjectType::Artifact,
                parent_object_id: artifact.source_object_id.as_ref().map(|id| id.0.clone()),
                source_locator: artifact.source_attribution.clone(),
                byte_length: None,
                parser_id: artifact.extractor_id.clone(),
                parser_version: artifact.extractor_version.clone(),
            },
            None,
        )
    }

    pub fn for_report(report_id: &str, case_id: &str, locator: &str, parser_id: &str) -> Self {
        Self::new(
            ForensicMetadata {
                object_id: report_id.to_string(),
                case_id: Some(case_id.to_string()),
                source_id: None,
                object_type: ForensicObjectType::Report,
                parent_object_id: None,
                source_locator: Some(locator.to_string()),
                byte_length: None,
                parser_id: Some(parser_id.to_string()),
                parser_version: Some(FMD_SCHEMA_VERSION.to_string()),
            },
            None,
        )
    }
}

pub fn normalize_content_sha256(value: Option<&str>) -> Option<String> {
    normalize_sha256(value)
}
