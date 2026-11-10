use super::{read_member, MdRaidError, MdRaidMember};

pub(super) struct Raid10State {
    geometry: Geometry,
}

impl Raid10State {
    pub(super) fn new(
        layout: u32,
        raid_disks: u64,
        chunk_bytes: u64,
        device_bytes: u64,
    ) -> Result<Self, MdRaidError> {
        Ok(Self {
            geometry: Geometry::new(layout, raid_disks, chunk_bytes, device_bytes)?,
        })
    }

    pub(super) fn logical_size(&self) -> u64 {
        self.geometry.logical_size
    }

    pub(super) fn covered_by(&self, slots: &[u64]) -> bool {
        self.geometry.covered_by(slots)
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
            let chunk = logical / self.geometry.chunk_bytes;
            let within = logical % self.geometry.chunk_bytes;
            let count = (self.geometry.chunk_bytes - within)
                .min(self.geometry.logical_size - logical)
                .min((output.len() - total) as u64) as usize;
            let read = self.read_chunk(
                members,
                slots,
                chunk,
                within,
                &mut output[total..total + count],
            )?;
            logical += read as u64;
            total += read;
        }
        Ok(total)
    }

    fn read_chunk(
        &self,
        members: &mut [MdRaidMember],
        slots: &[Option<usize>],
        chunk: u64,
        within: u64,
        output: &mut [u8],
    ) -> std::io::Result<usize> {
        let mut last_error = None;
        for (slot, copy_offset) in self.geometry.copies(chunk) {
            let Some(member) = slots.get(slot).and_then(|member| *member) else {
                continue;
            };
            let data_offset = members[member].superblock.data_offset_sectors * super::SECTOR_SIZE;
            match read_member(
                members,
                slots,
                slot,
                data_offset + copy_offset + within,
                output,
            ) {
                Ok(()) => return Ok(output.len()),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or_else(|| std::io::Error::other("RAID10 has no readable copy")))
    }
}

struct Geometry {
    raid_disks: u64,
    near: u64,
    far: u64,
    offset: bool,
    far_set_size: u64,
    chunk_bytes: u64,
    stride_bytes: u64,
    logical_size: u64,
}

impl Geometry {
    fn new(
        layout: u32,
        raid_disks: u64,
        chunk_bytes: u64,
        device_bytes: u64,
    ) -> Result<Self, MdRaidError> {
        let near = u64::from(layout & 0xff);
        let far = u64::from((layout >> 8) & 0xff);
        let copies = near.checked_mul(far).ok_or(MdRaidError::SizeOverflow)?;
        if near == 0 || far == 0 || copies < 2 || copies > raid_disks {
            return Err(MdRaidError::MetadataConflict(
                "RAID10 layout has an invalid copy count".to_string(),
            ));
        }
        let far_set_size = match layout >> 17 {
            0 => raid_disks,
            1 => raid_disks / far,
            2 => near * far,
            _ => {
                return Err(MdRaidError::MetadataConflict(
                    "RAID10 layout has unsupported far-set flags".to_string(),
                ))
            }
        };
        if far_set_size == 0 || far_set_size > raid_disks {
            return Err(MdRaidError::MetadataConflict(
                "RAID10 far-set geometry is invalid".to_string(),
            ));
        }
        let chunks_per_device = device_bytes / chunk_bytes;
        let array_chunks = (chunks_per_device / far)
            .checked_mul(raid_disks)
            .and_then(|value| value.checked_div(near))
            .ok_or(MdRaidError::SizeOverflow)?;
        let used_chunks = array_chunks
            .checked_mul(copies)
            .ok_or(MdRaidError::SizeOverflow)?
            .div_ceil(raid_disks);
        let stride_chunks = if layout & (1 << 16) != 0 {
            1
        } else {
            used_chunks / far
        };
        let logical_chunks = (used_chunks / far)
            .checked_mul(raid_disks)
            .and_then(|value| value.checked_div(near))
            .ok_or(MdRaidError::SizeOverflow)?;
        Ok(Self {
            raid_disks,
            near,
            far,
            offset: layout & (1 << 16) != 0,
            far_set_size,
            chunk_bytes,
            stride_bytes: stride_chunks
                .checked_mul(chunk_bytes)
                .ok_or(MdRaidError::SizeOverflow)?,
            logical_size: logical_chunks
                .checked_mul(chunk_bytes)
                .ok_or(MdRaidError::SizeOverflow)?,
        })
    }

    fn copies(&self, chunk: u64) -> Vec<(usize, u64)> {
        let scaled = chunk * self.near;
        let mut stripe = scaled / self.raid_disks;
        let mut device = scaled % self.raid_disks;
        if self.offset {
            stripe *= self.far;
        }
        let mut base = stripe * self.chunk_bytes;
        let last_set_start = (self.raid_disks / self.far_set_size - 1) * self.far_set_size;
        let last_set_size = self.far_set_size + self.raid_disks % self.far_set_size;
        let mut output = Vec::with_capacity((self.near * self.far) as usize);
        for _ in 0..self.near {
            let mut copy_device = device;
            let mut copy_offset = base;
            output.push((copy_device as usize, copy_offset));
            for _ in 1..self.far {
                let set = copy_device / self.far_set_size;
                copy_device += self.near;
                if !self.raid_disks.is_multiple_of(self.far_set_size)
                    && copy_device > last_set_start
                {
                    copy_device -= last_set_start;
                    copy_device %= last_set_size;
                    copy_device += last_set_start;
                } else {
                    copy_device %= self.far_set_size;
                    copy_device += self.far_set_size * set;
                }
                copy_offset += self.stride_bytes;
                output.push((copy_device as usize, copy_offset));
            }
            device += 1;
            if device >= self.raid_disks {
                device = 0;
                base += self.chunk_bytes;
            }
        }
        output
    }

    fn covered_by(&self, slots: &[u64]) -> bool {
        (0..self.raid_disks).all(|chunk| {
            self.copies(chunk)
                .iter()
                .any(|(slot, _)| slots.contains(&(*slot as u64)))
        })
    }
}
