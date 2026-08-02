use std::io;

use crate::attribute::nonresident_compression_unit;
use crate::data_runs::parse_data_runs_ext;
use crate::{invalid_fs_data, truncate_data_to_declared_size, MAX_BUFFERED_FILE_BYTES};

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
        let run_offset =
            u16::from_le_bytes([record[attr_pos + 0x20], record[attr_pos + 0x21]]) as usize;
        let allocated_size = u64::from_le_bytes(
            record[attr_pos + 0x28..attr_pos + 0x30]
                .try_into()
                .unwrap_or([0; 8]),
        );
        if run_offset == 0 || allocated_size == 0 || attr_pos + run_offset >= record.len() {
            return Ok(Vec::new());
        }
        if allocated_size > MAX_BUFFERED_FILE_BYTES as u64 {
            return Err(invalid_fs_data(format!(
                "attribute allocation too large: {} bytes",
                allocated_size
            )));
        }

        let attribute_flags = u16::from_le_bytes(
            record[attr_pos + 0x0c..attr_pos + 0x0e]
                .try_into()
                .unwrap_or([0; 2]),
        );
        let real_size = u64::from_le_bytes(
            record[attr_pos + 0x30..attr_pos + 0x38]
                .try_into()
                .unwrap_or([0; 8]),
        );
        let runs = parse_data_runs_ext(&record[attr_pos + run_offset..])?;

        if attribute_flags & 0x0001 != 0 {
            let compression_unit_exp = nonresident_compression_unit(record, attr_pos);
            let decoded =
                self.read_compressed_data_runs_to_vec(&runs, compression_unit_exp, real_size)?;
            return Ok(truncate_data_to_declared_size(decoded, real_size));
        }

        let data = self.read_data_runs_to_vec(&runs, true, allocated_size)?;
        Ok(truncate_data_to_declared_size(data, real_size))
    }
}
