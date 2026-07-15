#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephOsdDeviceBindingRecord {
    pub inventory_id: String,
    pub data_source_id: String,
    pub source_path: String,
    pub canonical_source_path: String,
    pub source_kind: String,
    pub lvm_vg_uuid: String,
    pub lvm_vg_name: String,
    pub lvm_lv_uuid: String,
    pub lvm_lv_name: String,
    pub device_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephOsdPvBindingRecord {
    pub inventory_id: String,
    pub ordinal: u32,
    pub source_path: String,
    pub canonical_source_path: String,
    pub source_kind: String,
    pub pv_offset: u64,
    pub pv_uuid: String,
    pub pv_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephOsdDeviceBindingAggregate {
    pub device: CephOsdDeviceBindingRecord,
    pub physical_volumes: Vec<CephOsdPvBindingRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephOsdRegisteredSourceIdentity {
    pub data_source_id: String,
    pub source_path: String,
    pub canonical_source_path: Option<String>,
    pub source_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephOsdSourceBoundDevice {
    pub source: CephOsdRegisteredSourceIdentity,
    pub binding: CephOsdDeviceBindingAggregate,
}
