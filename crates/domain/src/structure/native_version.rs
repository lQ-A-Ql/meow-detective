use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MdMetadataVersion {
    V0_90,
    V1_0,
    V1_1,
    V1_2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NativeVersion {
    LvmLabel,
    LvmCommitted {
        vg_uuid: String,
        seqno: u64,
    },
    Md {
        metadata_version: MdMetadataVersion,
        events: u64,
    },
    CephRbd {
        image_id: String,
        pool_id: i64,
        snapshot_id: Option<u64>,
        lineage_sha256: String,
    },
    CephFs {
        filesystem_id: i64,
        fsmap_epoch: u32,
        mdsmap_epoch: u32,
        namespace_input_sha256: String,
        journal_boundary_sha256: Option<String>,
    },
}
