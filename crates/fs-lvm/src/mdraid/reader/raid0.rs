use super::{read_member, MdRaidError, MdRaidMember};

pub(super) struct Raid0State {
    zones: Vec<Raid0Zone>,
    chunk_bytes: u64,
    layout: u32,
    size: u64,
}

struct Raid0Zone {
    logical_start: u64,
    logical_end: u64,
    device_start: u64,
    slots: Vec<u32>,
}

impl Raid0State {
    pub(super) fn new(
        geometry: &[super::RaidMemberGeometry],
        layout: u32,
        chunk_bytes: u64,
    ) -> Result<Self, MdRaidError> {
        let zones = build_zones(geometry, chunk_bytes)?;
        let size = zones
            .last()
            .map(|zone| zone.logical_end)
            .unwrap_or_default();
        Ok(Self {
            zones,
            chunk_bytes,
            layout,
            size,
        })
    }

    pub(super) fn read(
        &mut self,
        members: &mut [MdRaidMember],
        slots: &[Option<usize>],
        mut logical: u64,
        output: &mut [u8],
    ) -> std::io::Result<usize> {
        let mut total = 0usize;
        while total < output.len() {
            let zone = self
                .zones
                .iter()
                .find(|zone| logical < zone.logical_end)
                .ok_or_else(|| std::io::Error::other("RAID0 logical offset is outside zones"))?;
            let zone_logical = logical - zone.logical_start;
            let stripe = zone_logical / self.chunk_bytes;
            let within = zone_logical % self.chunk_bytes;
            let global_stripe = logical / self.chunk_bytes;
            let index = if self.layout == 1 {
                global_stripe as usize % zone.slots.len()
            } else {
                stripe as usize % zone.slots.len()
            };
            let slot = zone.slots[index] as usize;
            let member_index = slots[slot]
                .ok_or_else(|| std::io::Error::other(format!("RAID0 slot {slot} is missing")))?;
            let member_chunk = stripe / zone.slots.len() as u64;
            let data_offset =
                members[member_index].superblock.data_offset_sectors * super::SECTOR_SIZE;
            let member_offset =
                data_offset + zone.device_start + member_chunk * self.chunk_bytes + within;
            let count = (self.chunk_bytes - within)
                .min(self.size - logical)
                .min((output.len() - total) as u64) as usize;
            read_member(
                members,
                slots,
                slot,
                member_offset,
                &mut output[total..total + count],
            )?;
            logical += count as u64;
            total += count;
        }
        Ok(total)
    }
}

fn build_zones(
    geometry: &[super::RaidMemberGeometry],
    chunk_bytes: u64,
) -> Result<Vec<Raid0Zone>, MdRaidError> {
    let mut thresholds = geometry
        .iter()
        .map(|member| member.data_size_bytes)
        .collect::<Vec<_>>();
    thresholds.sort_unstable();
    thresholds.dedup();
    let mut zones = Vec::new();
    let mut device_start = 0u64;
    let mut logical_start = 0u64;
    for threshold in thresholds {
        let slots = geometry
            .iter()
            .filter(|member| member.data_size_bytes >= threshold)
            .map(|member| member.slot)
            .collect::<Vec<_>>();
        if slots.is_empty() || threshold <= device_start {
            continue;
        }
        let logical_end = logical_start
            .checked_add(
                (threshold - device_start)
                    .checked_mul(slots.len() as u64)
                    .ok_or(MdRaidError::SizeOverflow)?,
            )
            .ok_or(MdRaidError::SizeOverflow)?;
        zones.push(Raid0Zone {
            logical_start,
            logical_end,
            device_start,
            slots,
        });
        logical_start = logical_end;
        device_start = threshold;
    }
    if zones.is_empty() || chunk_bytes == 0 {
        return Err(MdRaidError::InvalidChunkSize);
    }
    Ok(zones)
}

pub(super) fn logical_size(
    geometry: &[super::RaidMemberGeometry],
    chunk_bytes: u64,
) -> Result<u64, MdRaidError> {
    Ok(build_zones(geometry, chunk_bytes)?
        .last()
        .map(|zone| zone.logical_end)
        .unwrap_or_default())
}
