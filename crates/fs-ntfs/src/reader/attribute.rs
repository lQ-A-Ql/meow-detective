use std::io;

use crate::attribute::parse_data_attribute_extent;
use crate::invalid_fs_data;

impl crate::NtfsReader {
    /// Read non-resident attribute data by walking its data run list.
    pub(crate) fn read_attr_nonresident(
        &self,
        attr_pos: usize,
        record: &[u8],
    ) -> io::Result<Vec<u8>> {
        if attr_pos + 9 > record.len() || (record[attr_pos + 8] & 1) == 0 {
            return Ok(Vec::new());
        }
        let attribute_length = u32::from_le_bytes(
            record[attr_pos + 4..attr_pos + 8]
                .try_into()
                .unwrap_or([0; 4]),
        ) as usize;
        let extent = parse_data_attribute_extent(record, attr_pos, attribute_length)?
            .ok_or_else(|| invalid_fs_data("invalid non-resident NTFS attribute"))?;
        self.read_data_extents_to_vec(&[extent])
    }
}
