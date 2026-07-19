use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsMap {
    pub epoch: u32,
    pub filesystems: Vec<CephFsFilesystem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephFsFilesystem {
    pub filesystem_id: i64,
    pub mds_map: CephMdsMap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephMdsMap {
    pub epoch: u32,
    pub name: String,
    pub enabled: bool,
    pub metadata_pool_id: i64,
    pub data_pool_ids: Vec<i64>,
    pub max_mds: i32,
    pub last_failure_osd_epoch: u32,
    pub daemons: Vec<CephMdsDaemon>,
    pub in_ranks: BTreeSet<i32>,
    pub up_ranks: BTreeMap<i32, u64>,
    pub failed_ranks: BTreeSet<i32>,
    pub stopped_ranks: BTreeSet<i32>,
    pub damaged_ranks: BTreeSet<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CephMdsDaemon {
    pub gid: u64,
    pub name: String,
    pub rank: i32,
    pub incarnation: i32,
    pub state: CephMdsState,
    pub state_sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CephMdsState {
    Null,
    ReplayOnce,
    StandbyReplay,
    Starting,
    Creating,
    Standby,
    Boot,
    Stopped,
    DoesNotExist,
    Replay,
    Resolve,
    Reconnect,
    Rejoin,
    ClientReplay,
    Active,
    Stopping,
    Damaged,
}

impl CephMdsState {
    pub fn from_raw(value: i32) -> crate::Result<Self> {
        match value {
            -10 => Ok(Self::Null),
            -9 => Ok(Self::ReplayOnce),
            -8 => Ok(Self::StandbyReplay),
            -7 => Ok(Self::Starting),
            -6 => Ok(Self::Creating),
            -5 => Ok(Self::Standby),
            -4 => Ok(Self::Boot),
            -1 => Ok(Self::Stopped),
            0 => Ok(Self::DoesNotExist),
            8 => Ok(Self::Replay),
            9 => Ok(Self::Resolve),
            10 => Ok(Self::Reconnect),
            11 => Ok(Self::Rejoin),
            12 => Ok(Self::ClientReplay),
            13 => Ok(Self::Active),
            14 => Ok(Self::Stopping),
            15 => Ok(Self::Damaged),
            value => Err(crate::CephWireError::UnknownCephFsMdsState { value }),
        }
    }

    pub fn is_active(self) -> bool {
        self == Self::Active
    }
}
