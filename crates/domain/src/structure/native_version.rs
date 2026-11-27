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
}
