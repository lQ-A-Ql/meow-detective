use super::{read_member, MdRaidMember};

mod reconstruct;

pub(super) struct MirrorState {
    pub(super) size: u64,
}

impl MirrorState {
    pub(super) fn read(
        &mut self,
        members: &mut [MdRaidMember],
        slots: &[Option<usize>],
        logical: u64,
        output: &mut [u8],
    ) -> std::io::Result<usize> {
        let count = (self.size - logical).min(output.len() as u64) as usize;
        let mut last_error = None;
        for (slot, member) in slots.iter().enumerate() {
            let Some(member) = member else {
                continue;
            };
            let data_offset = members[*member].superblock.data_offset_sectors * super::SECTOR_SIZE;
            match read_member(
                members,
                slots,
                slot,
                data_offset + logical,
                &mut output[..count],
            ) {
                Ok(()) => return Ok(count),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or_else(|| std::io::Error::other("RAID1 has no readable mirror")))
    }
}

pub(super) struct ParityState {
    level: i32,
    layout: u32,
    raid_disks: usize,
    data_disks: usize,
    chunk_bytes: u64,
    size: u64,
}

impl ParityState {
    pub(super) fn new(
        level: i32,
        layout: u32,
        raid_disks: usize,
        data_disks: usize,
        chunk_bytes: u64,
        size: u64,
    ) -> Self {
        Self {
            level,
            layout,
            raid_disks,
            data_disks,
            chunk_bytes,
            size,
        }
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
            let chunk = logical / self.chunk_bytes;
            let within = logical % self.chunk_bytes;
            let row = chunk / self.data_disks as u64;
            let position = (chunk % self.data_disks as u64) as usize;
            let placement =
                parity_placement(self.level, self.layout, self.raid_disks, row, position);
            let count = (self.chunk_bytes - within)
                .min(self.size - logical)
                .min((output.len() - total) as u64) as usize;
            let destination = &mut output[total..total + count];
            if !self.read_or_rebuild(members, slots, row, within, placement, destination)? {
                return Err(std::io::Error::other("RAID parity reconstruction failed"));
            }
            logical += count as u64;
            total += count;
        }
        Ok(total)
    }

    fn read_or_rebuild(
        &self,
        members: &mut [MdRaidMember],
        slots: &[Option<usize>],
        row: u64,
        within: u64,
        target: Placement,
        output: &mut [u8],
    ) -> std::io::Result<bool> {
        if self
            .read_column(members, slots, row, within, target.column, output)
            .is_ok()
        {
            return Ok(true);
        }
        let missing = slots
            .iter()
            .enumerate()
            .filter_map(|(slot, member)| member.is_none().then_some(slot))
            .collect::<Vec<_>>();
        if missing.len() > usize::from(self.level == 6) + 1 {
            return Ok(false);
        }
        let columns = (0..self.raid_disks)
            .map(|column| {
                self.read_column_buffer(members, slots, row, within, column)
                    .ok()
            })
            .collect::<Vec<_>>();
        reconstruct::rebuild(
            self.level,
            self.layout,
            self.raid_disks,
            self.data_disks,
            row,
            target,
            &columns,
            output,
        )
    }

    fn read_column_buffer(
        &self,
        members: &mut [MdRaidMember],
        slots: &[Option<usize>],
        row: u64,
        within: u64,
        column: usize,
    ) -> std::io::Result<Vec<u8>> {
        let count =
            (self.chunk_bytes - within).min(self.size - row * self.chunk_bytes - within) as usize;
        let mut output = vec![0u8; count];
        self.read_column(members, slots, row, within, column, &mut output)?;
        Ok(output)
    }

    fn read_column(
        &self,
        members: &mut [MdRaidMember],
        slots: &[Option<usize>],
        row: u64,
        within: u64,
        column: usize,
        output: &mut [u8],
    ) -> std::io::Result<()> {
        let member_index = slots
            .get(column)
            .and_then(|member| *member)
            .ok_or_else(|| std::io::Error::other(format!("RAID column {column} is missing")))?;
        let data_offset = members[member_index].superblock.data_offset_sectors * super::SECTOR_SIZE;
        read_member(
            members,
            slots,
            column,
            data_offset + row * self.chunk_bytes + within,
            output,
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Placement {
    pub(super) p: usize,
    pub(super) q: Option<usize>,
    pub(super) column: usize,
    pub(super) q_exponent: usize,
}

pub(super) fn parity_placement(
    level: i32,
    layout: u32,
    n: usize,
    row: u64,
    position: usize,
) -> Placement {
    if level == 4 {
        return Placement {
            p: n - 1,
            q: None,
            column: position,
            q_exponent: position,
        };
    }
    if level == 5 {
        let (p, column) = match layout {
            0 => {
                let p = (n - 1) - row as usize % n;
                (
                    p,
                    if position >= p {
                        position + 1
                    } else {
                        position
                    },
                )
            }
            1 => {
                let p = row as usize % n;
                (
                    p,
                    if position >= p {
                        position + 1
                    } else {
                        position
                    },
                )
            }
            2 => {
                let p = (n - 1) - row as usize % n;
                (p, (p + 1 + position) % n)
            }
            3 => {
                let p = row as usize % n;
                (p, (p + 1 + position) % n)
            }
            4 => (0, position + 1),
            _ => (n - 1, position),
        };
        return Placement {
            p,
            q: None,
            column,
            q_exponent: position,
        };
    }
    raid6_placement(layout, n, row, position)
}

fn raid6_placement(layout: u32, n: usize, row: u64, position: usize) -> Placement {
    let data_disks = n - 2;
    let asymmetric = |p: usize| {
        if p == n - 1 {
            (p, 0, position + 1, position)
        } else {
            let column = if position >= p {
                position + 2
            } else {
                position
            };
            (p, p + 1, column, position)
        }
    };
    let (p, q, column, q_exponent) = match layout {
        0 => {
            let value = asymmetric((n - 1) - row as usize % n);
            (value.0, value.1, value.2, value.3)
        }
        1 => {
            let value = asymmetric(row as usize % n);
            (value.0, value.1, value.2, value.3)
        }
        2 => {
            let p = (n - 1) - row as usize % n;
            (p, (p + 1) % n, (p + 2 + position) % n, position)
        }
        3 => {
            let p = row as usize % n;
            (p, (p + 1) % n, (p + 2 + position) % n, position)
        }
        4 => (0, 1, position + 2, position),
        5 => (data_disks, data_disks + 1, position, position),
        8 => {
            let value = asymmetric(row as usize % n);
            (
                value.0,
                value.1,
                value.2,
                column_data_index(value.2, value.0, value.1),
            )
        }
        9 => {
            let p = (n - 1) - ((row + 1) as usize % n);
            let value = asymmetric(p);
            (
                value.0,
                value.1,
                value.2,
                column_data_index(value.2, value.0, value.1),
            )
        }
        10 => {
            let p = (n - 1) - row as usize % n;
            let q = (p + n - 1) % n;
            let column = (p + 1 + position) % n;
            (p, q, column, column_data_index(column, p, q))
        }
        16..=20 => {
            let mapped = parity_placement(5, layout - 16, n, row, position);
            (mapped.p, n - 1, mapped.column, position)
        }
        _ => (data_disks, data_disks + 1, position, position),
    };
    Placement {
        p,
        q: Some(q),
        column,
        q_exponent,
    }
}

fn column_data_index(column: usize, p: usize, q: usize) -> usize {
    (0..column)
        .filter(|candidate| *candidate != p && *candidate != q)
        .count()
}
