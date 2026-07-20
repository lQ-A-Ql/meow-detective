use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub(crate) const FILE_HANDLE_PREFIX: &str = "file:";

pub(crate) enum RangeContentReader {
    Seekable(Box<dyn evidence_core::ReadSeek>),
    Streaming(Box<dyn Read>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewDescriptor {
    pub case_id: String,
    pub file_id: String,
    pub source_kind: String,
    pub source_path: String,
    pub partition_index: Option<usize>,
    pub filesystem_kind: Option<String>,
    pub path: String,
    pub mime: Option<String>,
    pub size: u64,
    pub data_source_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub partition_candidates: Vec<PreviewPartitionCandidate>,
    #[serde(default)]
    pub entry_size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry_modified_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ceph_fs: Option<PreviewCephFsDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewCephFsDescriptor {
    pub filesystem_identity: String,
    pub filesystem_id: i64,
    pub fsmap_epoch: u32,
    pub inode: u64,
    pub stripe_unit: u32,
    pub stripe_count: u32,
    pub object_size: u32,
    pub pool_id: i64,
    pub pool_namespace: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inline_data: Option<Vec<u8>>,
    pub projection_sha256: String,
    pub schema_version: u32,
    pub decoder_profile: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sparse_extents: Vec<crate::ceph_reconstruction::CephFsSparseExtentProof>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewPartitionCandidate {
    pub partition_index: usize,
    pub filesystem_kind: String,
    pub offset: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lvm_identity: Option<PreviewLvmIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewLvmIdentity {
    pub vg_uuid: String,
    pub vg_name: String,
    pub lv_uuid: String,
    pub lv_name: String,
    pub pv_offsets: Vec<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pv_sources: Vec<PreviewLvmPhysicalVolumeSource>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewLvmPhysicalVolumeSource {
    pub source_path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source_kind: String,
    pub offset: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub pv_uuid: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pv_name: Option<String>,
}

pub trait PreviewReadContext {
    fn conn(&self) -> &rusqlite::Connection;

    fn case_id(&self) -> &str {
        ""
    }

    fn get_cached_preview_descriptor(&mut self, _key: &str) -> Option<serde_json::Value> {
        None
    }

    fn set_cached_preview_descriptor(&mut self, _key: &str, _value: &serde_json::Value) {}

    fn open_evidence_reader(
        &mut self,
        descriptor: &PreviewDescriptor,
    ) -> Result<Box<dyn evidence_core::EvidenceReader>, crate::file_service::FileServiceError> {
        open_host_evidence_reader(
            &descriptor.source_kind,
            Path::new(&descriptor.source_path),
            &descriptor.case_id,
        )
    }

    fn read_cephfs_range(
        &mut self,
        _descriptor: &PreviewDescriptor,
        _offset: u64,
        _length: usize,
    ) -> Result<Option<Vec<u8>>, crate::file_service::FileServiceError> {
        Ok(None)
    }
}

impl PreviewReadContext for &rusqlite::Connection {
    fn conn(&self) -> &rusqlite::Connection {
        self
    }
}

impl<T> PreviewReadContext for &mut T
where
    T: PreviewReadContext + ?Sized,
{
    fn conn(&self) -> &rusqlite::Connection {
        (**self).conn()
    }

    fn case_id(&self) -> &str {
        (**self).case_id()
    }

    fn get_cached_preview_descriptor(&mut self, key: &str) -> Option<serde_json::Value> {
        (**self).get_cached_preview_descriptor(key)
    }

    fn set_cached_preview_descriptor(&mut self, key: &str, value: &serde_json::Value) {
        (**self).set_cached_preview_descriptor(key, value);
    }

    fn open_evidence_reader(
        &mut self,
        descriptor: &PreviewDescriptor,
    ) -> Result<Box<dyn evidence_core::EvidenceReader>, crate::file_service::FileServiceError> {
        (**self).open_evidence_reader(descriptor)
    }

    fn read_cephfs_range(
        &mut self,
        descriptor: &PreviewDescriptor,
        offset: u64,
        length: usize,
    ) -> Result<Option<Vec<u8>>, crate::file_service::FileServiceError> {
        (**self).read_cephfs_range(descriptor, offset, length)
    }
}

impl<'a, G, S> PreviewReadContext for (&'a rusqlite::Connection, &'a str, G, S)
where
    G: FnMut(&str) -> Option<serde_json::Value>,
    S: FnMut(&str, &serde_json::Value),
{
    fn conn(&self) -> &rusqlite::Connection {
        self.0
    }

    fn case_id(&self) -> &str {
        self.1
    }

    fn get_cached_preview_descriptor(&mut self, key: &str) -> Option<serde_json::Value> {
        (self.2)(key)
    }

    fn set_cached_preview_descriptor(&mut self, key: &str, value: &serde_json::Value) {
        (self.3)(key, value);
    }
}

pub(crate) fn open_host_evidence_reader(
    source_kind: &str,
    source_path: &Path,
    case_id: &str,
) -> Result<Box<dyn evidence_core::EvidenceReader>, crate::file_service::FileServiceError> {
    match source_kind {
        "e01" => crate::e01_reader_cache::open_e01_reader_cached(source_path, case_id)
            .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
            .map_err(crate::file_service::FileServiceError::Io),
        "raw" => evidence_core::RawImageReader::open(source_path)
            .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
            .map_err(crate::file_service::FileServiceError::Io),
        other => Err(crate::file_service::FileServiceError::Unsupported(format!(
            "Evidence reader is not available for source kind '{other}'",
        ))),
    }
}
