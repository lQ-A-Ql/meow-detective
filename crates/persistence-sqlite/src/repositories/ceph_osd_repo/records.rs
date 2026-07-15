use super::super::ceph_bluefs_repo::CephBluefsAggregate;
use super::super::ceph_bluestore_semantic_repo::CephBluestoreSemanticAggregate;
use super::super::ceph_rocksdb_latest_state_repo::CephRocksdbLatestStateRecord;
use super::super::ceph_rocksdb_repo::CephRocksdbAggregate;
use super::super::ceph_rocksdb_sst_repo::CephRocksdbSstRecord;
use super::super::ceph_rocksdb_wal_repo::CephRocksdbWalAggregate;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephOsdInventoryRecord {
    pub id: String,
    pub data_source_id: String,
    pub partition_index: Option<u32>,
    pub lvm_vg_uuid: Option<String>,
    pub lvm_vg_name: Option<String>,
    pub lvm_lv_uuid: Option<String>,
    pub lvm_lv_name: Option<String>,
    pub osd_uuid: String,
    pub ceph_fsid: Option<String>,
    pub whoami: Option<u32>,
    pub device_role: String,
    pub device_size: u64,
    pub birth_time_seconds: i64,
    pub birth_time_nanoseconds: u32,
    pub description: String,
    pub is_multi: bool,
    pub selected_epoch: Option<i64>,
    pub valid_label_count: u32,
    pub label_health: String,
    pub osd_key_present: bool,
    pub kv_backend: Option<String>,
    pub bluefs_enabled: Option<bool>,
    pub ceph_version_when_created: Option<String>,
    pub require_osd_release: Option<u32>,
    pub sanitized_metadata_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephOsdLabelReplicaRecord {
    pub inventory_id: String,
    pub position: u64,
    pub device_size: u64,
    pub birth_time_seconds: i64,
    pub birth_time_nanoseconds: u32,
    pub description: String,
    pub is_multi: bool,
    pub epoch: Option<i64>,
    pub is_selected: bool,
    pub struct_version: u8,
    pub struct_compat_version: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct CephRocksdbMetadataSnapshot<'a> {
    pub bluefs: &'a CephBluefsAggregate,
    pub rocksdb: &'a CephRocksdbAggregate,
    pub ssts: &'a [CephRocksdbSstRecord],
    pub wals: &'a CephRocksdbWalAggregate,
    pub latest_state: &'a [CephRocksdbLatestStateRecord],
    pub semantic: &'a CephBluestoreSemanticAggregate,
}
