use super::{read_member, MdRaidError, MdRaidMember, SECTOR_SIZE};

pub(super) struct LinearState {
    starts: Vec<(u64, usize)>,
    size: u64,
}

impl LinearState {
    pub(super) fn new(geometry: &[super::RaidMemberGeometry]) -> Result<Self, MdRaidError> {
        let mut starts = Vec::with_capacity(geometry.len());
        let mut logical = 0u64;
        for member in geometry {
            starts.push((logical, member.slot as usize));
            logical = logical
                .checked_add(member.data_size_bytes)
                .ok_or(MdRaidError::SizeOverflow)?;
        }
        Ok(Self {
            starts,
            size: logical,
        })
    }

    pub(super) fn read(
        &mut self,
        members: &mut [MdRaidMember],
        logical: u64,
        output: &mut [u8],
    ) -> std::io::Result<usize> {
        let slots = linear_slots(members);
        let mut total = 0usize;
        while total < output.len() {
            let absolute = logical + total as u64;
            let (start, slot) = self
                .starts
                .iter()
                .rev()
                .find(|(start, _)| absolute >= *start)
                .copied()
                .ok_or_else(|| std::io::Error::other("linear RAID offset is outside members"))?;
            let member_offset = absolute - start;
            let next = self
                .starts
                .iter()
                .filter(|(candidate, _)| *candidate > start)
                .map(|(candidate, _)| *candidate)
                .min()
                .unwrap_or(self.size);
            let count = (next - absolute).min((output.len() - total) as u64) as usize;
            let member = slots
                .get(slot)
                .and_then(|member| *member)
                .ok_or_else(|| std::io::Error::other("linear RAID member metadata is missing"))?;
            let data_offset = members[member].superblock.data_offset_sectors * SECTOR_SIZE;
            read_member(
                members,
                &slots,
                slot,
                data_offset + member_offset,
                &mut output[total..total + count],
            )?;
            total += count;
        }
        Ok(total)
    }
}

fn linear_slots(members: &[MdRaidMember]) -> Vec<Option<usize>> {
    let count = members
        .iter()
        .map(|member| member.superblock.raid_disks as usize)
        .max()
        .unwrap_or_default();
    let mut slots = vec![None; count];
    for (index, member) in members.iter().enumerate() {
        if let Some(slot) = member.superblock.slot_index() {
            slots[slot as usize] = Some(index);
        }
    }
    slots
}
