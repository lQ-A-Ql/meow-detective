use super::{format_hex, signed_seek, MdRaidError, MdRaidMember, MdRaidSuperblock, SECTOR_SIZE};
use evidence_core::{EvidenceReader, ReaderInfo};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

mod linear;
mod parity;
mod raid0;
mod raid10;

use parity::{MirrorState, ParityState};
use raid0::Raid0State;
use raid10::Raid10State;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RaidMemberGeometry {
    pub slot: u32,
    pub data_offset_bytes: u64,
    pub data_size_bytes: u64,
    pub events: u64,
    pub rebuilding: bool,
}

pub struct MdRaidReader {
    members: Vec<MdRaidMember>,
    slots: Vec<Option<usize>>,
    backend: Backend,
    info: ReaderInfo,
    logical_size: u64,
    position: u64,
}

enum Backend {
    Linear(linear::LinearState),
    Raid0(Raid0State),
    Mirror(MirrorState),
    Parity(ParityState),
    Raid10(Raid10State),
}

impl MdRaidReader {
    pub fn open(members: Vec<MdRaidMember>) -> Result<Self, MdRaidError> {
        let prepared = PreparedMembers::new(members)?;
        let logical_size = prepared.logical_size()?;
        let backend = prepared.backend()?;
        let first = prepared.first_superblock();
        let array_id = format_hex(&first.array_uuid);
        let kind = if first.level == -1 {
            "Linux MD linear".to_string()
        } else {
            format!("Linux MD RAID{}", first.level)
        };
        Ok(Self {
            members: prepared.members,
            slots: prepared.slots,
            backend,
            info: ReaderInfo {
                path: PathBuf::from(format!("mdraid://{array_id}")),
                size: logical_size,
                kind,
            },
            logical_size,
            position: 0,
        })
    }

    pub fn logical_size(&self) -> u64 {
        self.logical_size
    }

    fn read_at_inner(&mut self, logical: u64, output: &mut [u8]) -> std::io::Result<usize> {
        if logical >= self.logical_size || output.is_empty() {
            return Ok(0);
        }
        let limit = (self.logical_size - logical).min(output.len() as u64) as usize;
        match &mut self.backend {
            Backend::Linear(state) => state.read(&mut self.members, logical, &mut output[..limit]),
            Backend::Raid0(state) => state.read(
                &mut self.members,
                &self.slots,
                logical,
                &mut output[..limit],
            ),
            Backend::Mirror(state) => state.read(
                &mut self.members,
                &self.slots,
                logical,
                &mut output[..limit],
            ),
            Backend::Parity(state) => state.read(
                &mut self.members,
                &self.slots,
                logical,
                &mut output[..limit],
            ),
            Backend::Raid10(state) => state.read(
                &mut self.members,
                &self.slots,
                logical,
                &mut output[..limit],
            ),
        }
    }
}

struct PreparedMembers {
    members: Vec<MdRaidMember>,
    slots: Vec<Option<usize>>,
    geometry: Vec<RaidMemberGeometry>,
    level: i32,
    raid_disks: u32,
    layout: u32,
    chunk_bytes: u64,
    size_sectors: u64,
}

impl PreparedMembers {
    fn new(members: Vec<MdRaidMember>) -> Result<Self, MdRaidError> {
        let first = members.first().ok_or(MdRaidError::EmptyMembers)?;
        if first.superblock.raid_disks == 0 || first.superblock.raid_disks > super::MAX_MD_DEVICES {
            return Err(MdRaidError::InvalidDeviceCount);
        }
        let raid_disks = first.superblock.raid_disks;
        let mut slots = vec![None; raid_disks as usize];
        let mut geometry = Vec::with_capacity(members.len());
        for (index, member) in members.iter().enumerate() {
            validate_member(member, &first.superblock)?;
            let Some(slot) = member.superblock.slot_index() else {
                continue;
            };
            if slots[slot as usize].replace(index).is_some() {
                return Err(MdRaidError::MetadataConflict(format!(
                    "multiple members claim slot {slot}"
                )));
            }
            geometry.push(RaidMemberGeometry {
                slot,
                data_offset_bytes: member
                    .superblock
                    .data_offset_sectors
                    .checked_mul(SECTOR_SIZE)
                    .ok_or(MdRaidError::InvalidDataRange)?,
                data_size_bytes: (if matches!(first.superblock.level, -1 | 0) {
                    member.superblock.data_size_sectors
                } else {
                    member
                        .superblock
                        .data_size_sectors
                        .min(member.superblock.size_sectors)
                })
                .checked_mul(SECTOR_SIZE)
                .ok_or(MdRaidError::InvalidDataRange)?,
                events: member.superblock.events,
                rebuilding: member.superblock.is_rebuilding(),
            });
        }
        geometry.sort_by_key(|entry| entry.slot);
        Ok(Self {
            level: first.superblock.level,
            raid_disks,
            layout: first.superblock.layout,
            size_sectors: first.superblock.size_sectors,
            chunk_bytes: if matches!(first.superblock.level, -1 | 1) {
                SECTOR_SIZE
            } else {
                first.superblock.chunk_bytes()?
            },
            members,
            slots,
            geometry,
        })
    }

    fn first_superblock(&self) -> &MdRaidSuperblock {
        &self.members[0].superblock
    }

    fn logical_size(&self) -> Result<u64, MdRaidError> {
        let present = self.slots.iter().filter(|slot| slot.is_some()).count();
        match self.level {
            -1 => {
                require_complete(present, self.raid_disks as usize)?;
                self.geometry.iter().try_fold(0u64, |total, member| {
                    total
                        .checked_add(member.data_size_bytes)
                        .ok_or(MdRaidError::SizeOverflow)
                })
            }
            0 => {
                require_complete(present, self.raid_disks as usize)?;
                raid0::logical_size(&self.geometry, self.chunk_bytes)
            }
            1 => self
                .geometry
                .iter()
                .map(|member| member.data_size_bytes)
                .min()
                .ok_or(MdRaidError::EmptyMembers),
            4 | 5 => parity_logical_size(
                self.geometry
                    .iter()
                    .map(|member| member.data_size_bytes)
                    .min()
                    .ok_or(MdRaidError::EmptyMembers)?,
                self.raid_disks,
                1,
                self.chunk_bytes,
            ),
            6 => parity_logical_size(
                self.geometry
                    .iter()
                    .map(|member| member.data_size_bytes)
                    .min()
                    .ok_or(MdRaidError::EmptyMembers)?,
                self.raid_disks,
                2,
                self.chunk_bytes,
            ),
            10 => Ok(Raid10State::new(
                self.layout,
                self.raid_disks as u64,
                self.chunk_bytes,
                self.size_sectors
                    .checked_mul(SECTOR_SIZE)
                    .ok_or(MdRaidError::SizeOverflow)?,
            )?
            .logical_size()),
            _ => Err(MdRaidError::UnsupportedLevel(self.level)),
        }
    }

    fn backend(&self) -> Result<Backend, MdRaidError> {
        let present = self.slots.iter().filter(|slot| slot.is_some()).count();
        match self.level {
            -1 => {
                require_complete(present, self.raid_disks as usize)?;
                Ok(Backend::Linear(linear::LinearState::new(&self.geometry)?))
            }
            0 => {
                require_complete(present, self.raid_disks as usize)?;
                Ok(Backend::Raid0(Raid0State::new(
                    &self.geometry,
                    self.layout,
                    self.chunk_bytes,
                )?))
            }
            1 => {
                require_minimum(present, 1, self.raid_disks as usize)?;
                Ok(Backend::Mirror(MirrorState {
                    size: self
                        .geometry
                        .iter()
                        .map(|member| member.data_size_bytes)
                        .min()
                        .unwrap_or_default(),
                }))
            }
            4..=6 => {
                let parity = if self.level == 6 { 2 } else { 1 };
                let data_disks = self.raid_disks.saturating_sub(parity);
                if data_disks == 0 {
                    return Err(MdRaidError::UnsupportedLevel(self.level));
                }
                require_minimum(present, data_disks as usize, self.raid_disks as usize)?;
                Ok(Backend::Parity(ParityState::new(
                    self.level,
                    self.layout,
                    self.raid_disks as usize,
                    data_disks as usize,
                    self.chunk_bytes,
                    self.logical_size()?,
                )))
            }
            10 => {
                let state = Raid10State::new(
                    self.layout,
                    self.raid_disks as u64,
                    self.chunk_bytes,
                    self.size_sectors
                        .checked_mul(SECTOR_SIZE)
                        .ok_or(MdRaidError::SizeOverflow)?,
                )?;
                let present_slots = self
                    .slots
                    .iter()
                    .enumerate()
                    .filter_map(|(slot, member)| member.is_some().then_some(slot as u64))
                    .collect::<Vec<_>>();
                if !state.covered_by(&present_slots) {
                    return Err(MdRaidError::MetadataConflict(
                        "RAID10 has logical chunks with no available copy".to_string(),
                    ));
                }
                Ok(Backend::Raid10(state))
            }
            _ => Err(MdRaidError::UnsupportedLevel(self.level)),
        }
    }
}

fn validate_member(member: &MdRaidMember, first: &MdRaidSuperblock) -> Result<(), MdRaidError> {
    let sb = &member.superblock;
    if !sb.checksum_valid {
        return Err(MdRaidError::InvalidChecksum);
    }
    if sb.array_uuid != first.array_uuid
        || sb.level != first.level
        || sb.raid_disks != first.raid_disks
        || sb.layout != first.layout
        || sb.chunk_sectors != first.chunk_sectors
        || sb.size_sectors != first.size_sectors
        || sb.events != first.events
        || sb.metadata_version != first.metadata_version
    {
        return Err(MdRaidError::MetadataConflict(
            "array identity, geometry, metadata version, or events differs".to_string(),
        ));
    }
    if sb.feature_map & !(1 | 4096) != 0
        || (sb.level != 0 && sb.feature_map & 4096 != 0)
        || (matches!(sb.level, 1 | 4 | 5 | 6 | 10) && sb.resync_offset_sectors != u64::MAX)
    {
        return Err(MdRaidError::UnsupportedState(
            "member requires recovery, reshape, bad-block, journal, or unsupported feature handling"
                .to_string(),
        ));
    }
    validate_layout(sb)?;
    let data_end = sb
        .data_offset_sectors
        .checked_add(sb.data_size_sectors)
        .and_then(|sector| sector.checked_mul(SECTOR_SIZE))
        .ok_or(MdRaidError::InvalidDataRange)?;
    if sb.data_size_sectors == 0 || data_end > member.reader.info().size {
        return Err(MdRaidError::InvalidDataRange);
    }
    Ok(())
}

fn validate_layout(sb: &MdRaidSuperblock) -> Result<(), MdRaidError> {
    let supported = match sb.level {
        -1 | 1 => sb.layout == 0,
        0 => matches!(sb.layout, 0..=2),
        4 => sb.layout == 0,
        5 => matches!(sb.layout, 0..=5),
        6 => matches!(sb.layout, 0..=5 | 8..=10 | 16..=20),
        10 => sb.layout & !0x7ffff == 0,
        _ => false,
    };
    if supported {
        Ok(())
    } else {
        Err(MdRaidError::UnsupportedState(format!(
            "RAID{} layout {} is unsupported",
            sb.level, sb.layout
        )))
    }
}

fn require_complete(actual: usize, expected: usize) -> Result<(), MdRaidError> {
    if actual == expected {
        Ok(())
    } else {
        Err(MdRaidError::MemberCount {
            expected: expected as u32,
            actual,
        })
    }
}

fn require_minimum(actual: usize, minimum: usize, expected: usize) -> Result<(), MdRaidError> {
    if actual >= minimum {
        Ok(())
    } else {
        Err(MdRaidError::MemberCount {
            expected: expected as u32,
            actual,
        })
    }
}

fn parity_logical_size(
    member_bytes: u64,
    raid_disks: u32,
    parity_disks: u32,
    chunk_bytes: u64,
) -> Result<u64, MdRaidError> {
    let data_disks = raid_disks.saturating_sub(parity_disks);
    if data_disks == 0 || chunk_bytes == 0 {
        return Err(MdRaidError::InvalidChunkSize);
    }
    (member_bytes - member_bytes % chunk_bytes)
        .checked_mul(u64::from(data_disks))
        .ok_or(MdRaidError::SizeOverflow)
}

fn read_member(
    members: &mut [MdRaidMember],
    slot_member: &[Option<usize>],
    slot: usize,
    offset: u64,
    output: &mut [u8],
) -> std::io::Result<()> {
    let member = slot_member
        .get(slot)
        .and_then(|member| *member)
        .ok_or_else(|| std::io::Error::other(format!("MD RAID slot {slot} is missing")))?;
    let reader = &mut members[member].reader;
    reader.seek(SeekFrom::Start(offset))?;
    reader.read_exact(output)
}

impl Read for MdRaidReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.read_at_inner(self.position, buffer)?;
        self.position = self.position.saturating_add(read as u64);
        Ok(read)
    }
}

impl Seek for MdRaidReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        let next = match position {
            SeekFrom::Start(value) => value,
            SeekFrom::End(value) => signed_seek(self.logical_size, value)?,
            SeekFrom::Current(value) => signed_seek(self.position, value)?,
        };
        self.position = next;
        Ok(next)
    }
}

impl EvidenceReader for MdRaidReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}
