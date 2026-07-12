mod discovery;
mod info;
mod mapping;
mod open;

use crate::label::LvmLabel;
use crate::metadata::{LvMeta, VolumeGroup};
use evidence_core::EvidenceReader;
use std::sync::{Arc, Mutex};

pub(crate) type SharedReader = Arc<Mutex<Box<dyn EvidenceReader>>>;

pub(super) struct DiscoveredPv {
    pub(super) reader: SharedReader,
    pub(super) label: LvmLabel,
    pub(super) pv_offset: u64,
}

#[derive(Debug, Clone)]
pub struct LvInfo {
    pub name: String,
    pub uuid: String,
    pub size_bytes: u64,
    pub role: String,
    pub status: Vec<String>,
    pub visible: bool,
    pub directly_mappable: bool,
    pub unsupported_reason: Option<String>,
}

pub struct LvmPool {
    pub(crate) volume_group: VolumeGroup,
    pub(crate) device_readers: Vec<SharedReader>,
    pub(crate) pv_start_offsets: Vec<(String, u64)>,
    pub(crate) pv_data_offsets: Vec<(String, u64)>,
    pub(crate) logical_volumes: Vec<LvMeta>,
}

pub(crate) use info::lv_info_from_meta;
