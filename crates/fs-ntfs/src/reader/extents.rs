use std::io;

use crate::attribute::{parse_data_attribute_extent, sort_data_extents, DataAttributeExtent};
use crate::utils::{is_extension_record_for, validate_file_record};
use crate::{ATTR_TYPE_DATA, ATTR_TYPE_END};

impl crate::NtfsReader {
    pub(crate) fn collect_unnamed_data_extents(
        &self,
        inode: u64,
    ) -> io::Result<Vec<DataAttributeExtent>> {
        let record = self.read_mft_record(inode)?;
        self.collect_unnamed_data_extents_from_base(inode, record)
    }

    pub(super) fn collect_unnamed_data_extents_from_base(
        &self,
        inode: u64,
        record: Vec<u8>,
    ) -> io::Result<Vec<DataAttributeExtent>> {
        validate_file_record(&record, inode)?;

        let mut extents = Vec::new();
        self.collect_data_extents_from_record(&record, &mut extents)?;
        let external_records = self.external_attribute_records_for_unnamed_data(inode, &record)?;
        for external_record_number in external_records {
            if external_record_number == inode {
                continue;
            }

            let external = self.read_mft_record(external_record_number)?;
            if !is_extension_record_for(&external, inode) {
                tracing::warn!(
                    inode,
                    external_record_number,
                    "Skipping NTFS external attribute record that does not reference the base file"
                );
                continue;
            }
            self.collect_data_extents_from_record(&external, &mut extents)?;
        }

        sort_data_extents(&mut extents);
        Ok(extents)
    }

    fn collect_data_extents_from_record(
        &self,
        record: &[u8],
        extents: &mut Vec<DataAttributeExtent>,
    ) -> io::Result<()> {
        let attribute_offset = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
        let mut position = attribute_offset;
        while position + 8 < record.len() {
            let attribute_type =
                u32::from_le_bytes(record[position..position + 4].try_into().unwrap_or([0; 4]));
            if attribute_type == ATTR_TYPE_END {
                break;
            }
            let length = u32::from_le_bytes(
                record[position + 4..position + 8]
                    .try_into()
                    .unwrap_or([0; 4]),
            ) as usize;
            if length == 0 || position + length > record.len() {
                break;
            }

            if attribute_type == ATTR_TYPE_DATA
                && crate::attribute::is_unnamed_attribute(record, position)
            {
                if let Some(extent) = parse_data_attribute_extent(record, position, length)? {
                    extents.push(extent);
                }
            }
            position += length;
        }
        Ok(())
    }
}
