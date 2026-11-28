use std::{
    collections::BTreeSet,
    sync::{Arc, Mutex},
};

use btrfs_driver::{chunk::ChunkProfile, superblock::Superblock};
use evidence_core::EvidenceReader;
use fs_core::BlockRead;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{reader::EvidenceBlockDevice, BtrfsReader};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BtrfsDeviceIdentity {
    pub fsid: String,
    pub generation: u64,
    pub root: u64,
    pub chunk_root: u64,
    pub device_count: u64,
    pub devid: u64,
    pub device_uuid: String,
    pub device_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BtrfsPoolIdentity {
    pub devices: Vec<BtrfsDeviceIdentity>,
    pub profiles: Vec<String>,
    pub mapping_sha256: String,
    pub total_bytes: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum BtrfsPoolError {
    #[error("Btrfs device I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("Btrfs native validation failed: {0}")]
    Native(#[from] btrfs_driver::Error),
    #[error("Btrfs devices have conflicting identities or generations")]
    Conflict,
    #[error("Btrfs pool requires every device exactly once")]
    Incomplete,
    #[error("Btrfs profile has no admitted public content oracle: {0}")]
    UnsupportedProfile(String),
}

fn device(
    reader: Box<dyn EvidenceReader>,
    offset: u64,
) -> Result<Arc<dyn BlockRead>, BtrfsPoolError> {
    let size = reader
        .info()
        .size
        .checked_sub(offset)
        .filter(|size| *size >= 0x11000)
        .ok_or(BtrfsPoolError::Incomplete)?;
    Ok(Arc::new(EvidenceBlockDevice {
        reader: Mutex::new(reader),
        offset,
        size,
    }))
}

fn identity(superblock: &Superblock) -> BtrfsDeviceIdentity {
    BtrfsDeviceIdentity {
        fsid: hex::encode(superblock.fsid),
        generation: superblock.generation,
        root: superblock.root,
        chunk_root: superblock.chunk_root,
        device_count: superblock.num_devices,
        devid: superblock.dev_item.devid,
        device_uuid: hex::encode(superblock.dev_item.uuid),
        device_size: superblock.dev_item.total_bytes,
    }
}

pub fn inspect_device(
    reader: Box<dyn EvidenceReader>,
    offset: u64,
) -> Result<BtrfsDeviceIdentity, BtrfsPoolError> {
    let device = device(reader, offset)?;
    let (superblock, _) = btrfs_driver::superblock::read_superblock(device.as_ref())?;
    if superblock.dev_item.total_bytes > device.size_bytes() {
        return Err(BtrfsPoolError::Incomplete);
    }
    Ok(identity(&superblock))
}

impl BtrfsReader {
    pub fn open_pool(
        inputs: Vec<(Box<dyn EvidenceReader>, u64)>,
    ) -> Result<(Self, BtrfsPoolIdentity), BtrfsPoolError> {
        if inputs.is_empty() || inputs.len() > 64 {
            return Err(BtrfsPoolError::Incomplete);
        }
        let mut devices = Vec::with_capacity(inputs.len());
        let mut identities = Vec::with_capacity(inputs.len());
        for (reader, offset) in inputs {
            let device = device(reader, offset)?;
            let (superblock, _) = btrfs_driver::superblock::read_superblock(device.as_ref())?;
            if superblock.dev_item.total_bytes > device.size_bytes() {
                return Err(BtrfsPoolError::Incomplete);
            }
            identities.push(identity(&superblock));
            devices.push(device);
        }
        validate_members(&identities)?;
        let filesystem = btrfs_driver::Filesystem::mount_pool(devices)?;
        let profiles = validate_mapping(&filesystem, &identities)?;
        identities.sort_by_key(|identity| identity.devid);
        let mapping_sha256 = hex::encode(Sha256::digest(
            format!("{:?}", filesystem.chunk_map().chunks()).as_bytes(),
        ));
        let identity = BtrfsPoolIdentity {
            devices: identities,
            profiles,
            mapping_sha256,
            total_bytes: filesystem.superblock().total_bytes,
        };
        Ok((
            Self {
                filesystem: Arc::new(filesystem),
            },
            identity,
        ))
    }
}

fn validate_members(identities: &[BtrfsDeviceIdentity]) -> Result<(), BtrfsPoolError> {
    let first = identities.first().ok_or(BtrfsPoolError::Incomplete)?;
    if first.device_count != identities.len() as u64 {
        return Err(BtrfsPoolError::Incomplete);
    }
    let mut ids = BTreeSet::new();
    let mut uuids = BTreeSet::new();
    for member in identities {
        if !ids.insert(member.devid) || !uuids.insert(&member.device_uuid) {
            return Err(BtrfsPoolError::Incomplete);
        }
        if member.fsid != first.fsid
            || member.generation != first.generation
            || member.root != first.root
            || member.chunk_root != first.chunk_root
            || member.device_count != first.device_count
        {
            return Err(BtrfsPoolError::Conflict);
        }
    }
    Ok(())
}

fn validate_mapping(
    filesystem: &btrfs_driver::Filesystem,
    identities: &[BtrfsDeviceIdentity],
) -> Result<Vec<String>, BtrfsPoolError> {
    let mut profiles = BTreeSet::new();
    for chunk in filesystem.chunk_map().chunks() {
        let profile = chunk.profile()?;
        if !matches!(
            profile,
            ChunkProfile::Single
                | ChunkProfile::Dup
                | ChunkProfile::Raid0
                | ChunkProfile::Raid1
                | ChunkProfile::Raid10
        ) {
            return Err(BtrfsPoolError::UnsupportedProfile(profile.name().into()));
        }
        profiles.insert(profile.name().to_owned());
        for stripe in &chunk.stripes {
            if !identities.iter().any(|member| {
                member.devid == stripe.devid
                    && member.device_uuid == hex::encode(stripe.dev_uuid)
                    && stripe.offset < member.device_size
            }) {
                return Err(BtrfsPoolError::Conflict);
            }
        }
    }
    Ok(profiles.into_iter().collect())
}
