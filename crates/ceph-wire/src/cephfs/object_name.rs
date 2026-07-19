const MAX_MDS: u64 = 0x100;
const MDS_LOG_OFFSET: u64 = 2 * MAX_MDS;
const MDS_LOG_BACKUP_OFFSET: u64 = 3 * MAX_MDS;
const MDS_LOG_POINTER_OFFSET: u64 = 4 * MAX_MDS;
const MDS_PURGE_QUEUE_OFFSET: u64 = 5 * MAX_MDS;

pub fn format_cephfs_journal_pointer_object_name(rank: u32) -> Option<String> {
    (u64::from(rank) < MAX_MDS)
        .then(|| format!("{:x}.00000000", MDS_LOG_POINTER_OFFSET + u64::from(rank)))
}

pub fn format_cephfs_journal_data_object_name(
    rank: u32,
    journal_inode: u64,
    object_index: u32,
) -> Option<String> {
    let rank = u64::from(rank);
    let base = journal_inode.checked_sub(rank)?;
    if rank >= MAX_MDS || (base != MDS_LOG_OFFSET && base != MDS_LOG_BACKUP_OFFSET) {
        return None;
    }
    Some(format!("{journal_inode:x}.{object_index:08x}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CephFsRankTableKind {
    Inode,
    Session,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CephFsMetadataObjectClass {
    DirFragmentCandidate {
        inode: u64,
        fragment: u32,
    },
    StandaloneInodeCandidate {
        inode: u64,
    },
    JournalData {
        rank: u32,
        backup: bool,
        object_index: u32,
    },
    JournalPointer {
        rank: u32,
    },
    PurgeQueue {
        rank: u32,
        object_index: u32,
    },
    SnapTable,
    AnchorTable,
    RankTable {
        rank: u32,
        kind: CephFsRankTableKind,
    },
    OpenFileTable {
        rank: u32,
        object_index: u32,
    },
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CephFsMetadataObjectCandidates {
    pub inode: bool,
    pub dirfrag: bool,
    pub dentry: bool,
    pub backtrace: bool,
    pub xattr: bool,
    pub snapshot_realm: bool,
}

impl CephFsMetadataObjectCandidates {
    pub const INODE: u8 = 1 << 0;
    pub const DIRFRAG: u8 = 1 << 1;
    pub const DENTRY: u8 = 1 << 2;
    pub const BACKTRACE: u8 = 1 << 3;
    pub const XATTR: u8 = 1 << 4;
    pub const SNAPSHOT_REALM: u8 = 1 << 5;

    pub fn mask(self) -> u8 {
        [
            (self.inode, Self::INODE),
            (self.dirfrag, Self::DIRFRAG),
            (self.dentry, Self::DENTRY),
            (self.backtrace, Self::BACKTRACE),
            (self.xattr, Self::XATTR),
            (self.snapshot_realm, Self::SNAPSHOT_REALM),
        ]
        .into_iter()
        .filter_map(|(enabled, bit)| enabled.then_some(bit))
        .fold(0, |mask, bit| mask | bit)
    }
}

pub fn classify_cephfs_metadata_object_name(
    name: &[u8],
) -> (CephFsMetadataObjectClass, CephFsMetadataObjectCandidates) {
    let Some(name) = canonical_ascii(name) else {
        return unknown();
    };
    if let Some(class) = classify_named_table(name) {
        return (class, CephFsMetadataObjectCandidates::default());
    }
    if let Some(inode) = name
        .strip_suffix(".00000000.inode")
        .and_then(parse_inode_hex)
        .filter(|inode| *inode != 0)
    {
        return (
            CephFsMetadataObjectClass::StandaloneInodeCandidate { inode },
            inode_candidates(),
        );
    }
    let Some((inode, object_index)) = parse_file_object_name(name) else {
        return unknown();
    };
    classify_file_object(inode, object_index)
}

fn classify_named_table(name: &str) -> Option<CephFsMetadataObjectClass> {
    match name {
        "mds_snaptable" => return Some(CephFsMetadataObjectClass::SnapTable),
        "mds_anchortable" => return Some(CephFsMetadataObjectClass::AnchorTable),
        _ => {}
    }
    let remainder = name.strip_prefix("mds")?;
    let (rank, suffix) = remainder.split_once('_')?;
    let rank = parse_decimal_u32(rank)?;
    if let Some(index) = suffix
        .strip_prefix("openfiles.")
        .and_then(parse_canonical_hex_u32)
    {
        return Some(CephFsMetadataObjectClass::OpenFileTable {
            rank,
            object_index: index,
        });
    }
    let kind = match suffix {
        "inotable" => CephFsRankTableKind::Inode,
        "sessionmap" => CephFsRankTableKind::Session,
        _ => return None,
    };
    Some(CephFsMetadataObjectClass::RankTable { rank, kind })
}

fn classify_file_object(
    inode: u64,
    object_index: u32,
) -> (CephFsMetadataObjectClass, CephFsMetadataObjectCandidates) {
    if let Some(rank) = private_rank(inode, MDS_LOG_OFFSET) {
        return (
            CephFsMetadataObjectClass::JournalData {
                rank,
                backup: false,
                object_index,
            },
            CephFsMetadataObjectCandidates::default(),
        );
    }
    if let Some(rank) = private_rank(inode, MDS_LOG_BACKUP_OFFSET) {
        return (
            CephFsMetadataObjectClass::JournalData {
                rank,
                backup: true,
                object_index,
            },
            CephFsMetadataObjectCandidates::default(),
        );
    }
    if let Some(rank) = private_rank(inode, MDS_LOG_POINTER_OFFSET) {
        return if object_index == 0 {
            (
                CephFsMetadataObjectClass::JournalPointer { rank },
                CephFsMetadataObjectCandidates::default(),
            )
        } else {
            unknown()
        };
    }
    if let Some(rank) = private_rank(inode, MDS_PURGE_QUEUE_OFFSET) {
        return (
            CephFsMetadataObjectClass::PurgeQueue { rank, object_index },
            CephFsMetadataObjectCandidates::default(),
        );
    }
    if inode == 0 {
        return unknown();
    }
    (
        CephFsMetadataObjectClass::DirFragmentCandidate {
            inode,
            fragment: object_index,
        },
        dirfrag_candidates(),
    )
}

fn private_rank(inode: u64, offset: u64) -> Option<u32> {
    let rank = inode.checked_sub(offset)?;
    (rank < MAX_MDS).then_some(rank as u32)
}

fn parse_file_object_name(name: &str) -> Option<(u64, u32)> {
    let (inode, object_index) = name.split_once('.')?;
    if inode.is_empty()
        || inode.len() > 16
        || object_index.len() != 8
        || name.matches('.').count() != 1
    {
        return None;
    }
    Some((parse_inode_hex(inode)?, parse_hex_u32(object_index)?))
}

fn parse_inode_hex(value: &str) -> Option<u64> {
    if value.len() > 1 && value.starts_with('0') {
        return None;
    }
    canonical_hex(value).then(|| u64::from_str_radix(value, 16).ok())?
}

fn parse_hex_u32(value: &str) -> Option<u32> {
    canonical_hex(value).then(|| u32::from_str_radix(value, 16).ok())?
}

fn parse_canonical_hex_u32(value: &str) -> Option<u32> {
    if value.len() > 1 && value.starts_with('0') {
        return None;
    }
    parse_hex_u32(value)
}

fn parse_decimal_u32(value: &str) -> Option<u32> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    value.parse().ok()
}

fn canonical_ascii(value: &[u8]) -> Option<&str> {
    let value = std::str::from_utf8(value).ok()?;
    (!value.is_empty() && !value.contains('\0')).then_some(value)
}

fn canonical_hex(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn inode_candidates() -> CephFsMetadataObjectCandidates {
    CephFsMetadataObjectCandidates {
        inode: true,
        xattr: true,
        snapshot_realm: true,
        ..CephFsMetadataObjectCandidates::default()
    }
}

fn dirfrag_candidates() -> CephFsMetadataObjectCandidates {
    CephFsMetadataObjectCandidates {
        inode: true,
        dirfrag: true,
        dentry: true,
        backtrace: true,
        xattr: true,
        snapshot_realm: true,
    }
}

fn unknown() -> (CephFsMetadataObjectClass, CephFsMetadataObjectCandidates) {
    (
        CephFsMetadataObjectClass::Unknown,
        CephFsMetadataObjectCandidates::default(),
    )
}
