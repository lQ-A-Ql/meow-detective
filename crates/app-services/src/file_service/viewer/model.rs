use std::io::Read;

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
