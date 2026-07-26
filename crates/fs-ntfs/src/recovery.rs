use crate::attribute::{is_unnamed_attribute, parse_data_attribute_extent, DataAttributeExtent};
use crate::data_runs::DataRun;
use crate::invalid_fs_data;
use crate::mft_scanner::MftRecordParser;
use std::io;

const BITMAP_INODE: u64 = 6;
const MFT_SCAN_RECORDS_PER_CHUNK: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NtfsAllocationState {
    Free,
    Allocated,
    PartiallyAllocated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NtfsDataExtent {
    pub logical_offset: u64,
    pub real_size: u64,
    pub resident_source_offset: Option<u64>,
    pub compressed: bool,
    pub encrypted: bool,
    pub sparse: bool,
    pub runs: Vec<DataRun>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NtfsDeletedFileRecord {
    pub record_number: u64,
    pub sequence_number: u16,
    pub parent_ref: u64,
    pub name: String,
    pub is_dir: bool,
    /// File-level NTFS EFS state derived from $STANDARD_INFORMATION,
    /// $FILE_NAME, or the unnamed $DATA attribute flags.
    pub encrypted: bool,
    pub size: u64,
    pub record_source_offset: u64,
    pub record_size: u32,
    pub has_attribute_list: bool,
    pub extents: Vec<NtfsDataExtent>,
}

impl crate::NtfsReader {
    pub fn scan_deleted_file_records(&self) -> io::Result<Vec<NtfsDeletedFileRecord>> {
        if self.mft_record_count == 0 {
            return Err(invalid_fs_data("NTFS $MFT has no readable record count"));
        }
        let record_size = usize::try_from(self.mft_record_size)
            .map_err(|_| invalid_fs_data("NTFS MFT record size exceeds platform limits"))?;
        let mut parser = MftRecordParser::new(self.mft_record_size, self.bytes_per_sector);
        let mut records = Vec::new();
        let mut chunk_start = 0u64;
        while chunk_start < self.mft_record_count {
            let chunk_records = MFT_SCAN_RECORDS_PER_CHUNK.min(self.mft_record_count - chunk_start);
            let chunk_len = usize::try_from(chunk_records)
                .ok()
                .and_then(|count| count.checked_mul(record_size))
                .ok_or_else(|| invalid_fs_data("NTFS MFT scan chunk size overflows"))?;
            let mut chunk = vec![0u8; chunk_len];
            self.read_mft_stream_at(
                chunk_start
                    .checked_mul(u64::from(self.mft_record_size))
                    .ok_or_else(|| invalid_fs_data("NTFS MFT scan offset overflows"))?,
                &mut chunk,
            )?;
            for index in 0..chunk_records {
                let record_number = chunk_start + index;
                let start = usize::try_from(index)
                    .ok()
                    .and_then(|value| value.checked_mul(record_size))
                    .ok_or_else(|| invalid_fs_data("NTFS MFT record offset overflows"))?;
                let raw = &chunk[start..start + record_size];
                if raw.len() < 0x18
                    || &raw[0..4] != b"FILE"
                    || u16::from_le_bytes([raw[0x16], raw[0x17]]) & 0x01 != 0
                {
                    continue;
                }
                let mut fixed = raw.to_vec();
                if crate::utils::apply_record_fixup(&mut fixed, self.bytes_per_sector as usize)
                    .is_err()
                {
                    continue;
                }
                let record_source_offset = self.mft_record_source_offset(record_number)?;
                if let Some(record) = deleted_record(
                    &mut parser,
                    &fixed,
                    record_number,
                    record_source_offset,
                    self.mft_record_size,
                    self.cluster_size,
                )? {
                    records.push(record);
                }
            }
            chunk_start += chunk_records;
        }
        Ok(records)
    }

    pub fn read_volume_bitmap(&self, max_bytes: usize) -> io::Result<Vec<u8>> {
        let bitmap = self.read_file_data_range(BITMAP_INODE, 0, max_bytes.saturating_add(1))?;
        if bitmap.len() > max_bytes {
            return Err(invalid_fs_data(format!(
                "NTFS $Bitmap exceeds the {max_bytes} byte recovery safety limit"
            )));
        }
        Ok(bitmap)
    }

    pub fn classify_data_run(
        &self,
        bitmap: &[u8],
        run: &DataRun,
    ) -> io::Result<NtfsAllocationState> {
        let Some(lcn) = run.lcn else {
            return Ok(NtfsAllocationState::Free);
        };
        if lcn < 0 || run.cluster_count == 0 {
            return Err(invalid_fs_data("invalid NTFS recovery data run"));
        }
        let start = lcn as u64;
        let end = start
            .checked_add(run.cluster_count)
            .ok_or_else(|| invalid_fs_data("NTFS recovery cluster range overflows"))?;
        let bitmap_bits = u64::try_from(bitmap.len())
            .ok()
            .and_then(|length| length.checked_mul(8))
            .ok_or_else(|| invalid_fs_data("NTFS bitmap size overflows"))?;
        if end > bitmap_bits {
            return Err(invalid_fs_data("NTFS data run is outside $Bitmap coverage"));
        }
        let mut has_allocated = false;
        let mut has_free = false;
        for cluster in start..end {
            let byte = bitmap[(cluster / 8) as usize];
            if byte & (1 << (cluster % 8)) == 0 {
                has_free = true;
            } else {
                has_allocated = true;
            }
            if has_allocated && has_free {
                break;
            }
        }
        Ok(match (has_allocated, has_free) {
            (false, true) | (false, false) => NtfsAllocationState::Free,
            (true, false) => NtfsAllocationState::Allocated,
            (true, true) => NtfsAllocationState::PartiallyAllocated,
        })
    }

    pub fn data_run_source_offset(&self, run: &DataRun) -> io::Result<u64> {
        let lcn = run
            .lcn
            .ok_or_else(|| invalid_fs_data("sparse NTFS data run has no source offset"))?;
        self.cluster_to_offset(lcn)
    }

    pub fn cluster_size(&self) -> u64 {
        self.cluster_size
    }

    pub fn volume_serial(&self) -> u64 {
        self.volume_serial
    }

    /// Revalidate the identity and deletion state of a persisted MFT candidate.
    ///
    /// Recovery ranges are physical evidence coordinates captured during a
    /// scan. Before exposing them again, the record must still refer to the
    /// same inactive MFT slot and sequence number.
    pub fn validate_deleted_file_record(
        &self,
        record_number: u64,
        expected_sequence: u16,
    ) -> io::Result<()> {
        if expected_sequence == 0 {
            return Err(invalid_fs_data(
                "NTFS recovery candidate has an invalid zero sequence number",
            ));
        }
        let record = self.read_mft_record(record_number)?;
        if record.len() < 0x18 || &record[0..4] != b"FILE" {
            return Err(invalid_fs_data(
                "persisted NTFS recovery candidate no longer has a FILE record",
            ));
        }
        let sequence = u16::from_le_bytes([record[0x10], record[0x11]]);
        if sequence != expected_sequence {
            return Err(invalid_fs_data(format!(
                "NTFS MFT sequence changed from {expected_sequence} to {sequence}"
            )));
        }
        let flags = u16::from_le_bytes([record[0x16], record[0x17]]);
        if flags & 0x01 != 0 {
            return Err(invalid_fs_data(
                "persisted NTFS recovery candidate is active again",
            ));
        }
        Ok(())
    }

    pub fn read_source_range(&self, offset: u64, out: &mut [u8]) -> io::Result<()> {
        use std::io::{Read, Seek, SeekFrom};

        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(offset))?;
        reader.read_exact(out)
    }
}

fn deleted_record(
    parser: &mut MftRecordParser,
    record: &[u8],
    record_number: u64,
    record_source_offset: u64,
    record_size: u32,
    cluster_size: u64,
) -> io::Result<Option<NtfsDeletedFileRecord>> {
    if record.len() < 0x18 || &record[0..4] != b"FILE" {
        return Ok(None);
    }
    let flags = u16::from_le_bytes([record[0x16], record[0x17]]);
    if flags & 0x01 != 0 {
        return Ok(None);
    }
    let Some(metadata) = parser.parse_fixed(record, record_number) else {
        return Ok(None);
    };
    let sequence_number = u16::from_le_bytes([record[0x10], record[0x11]]);
    if metadata.name.is_empty() || sequence_number == 0 {
        return Ok(None);
    }
    Ok(Some(NtfsDeletedFileRecord {
        record_number,
        sequence_number,
        parent_ref: metadata.parent_ref,
        name: metadata.name,
        is_dir: metadata.is_dir,
        encrypted: metadata.encrypted,
        size: metadata.size,
        record_source_offset,
        record_size,
        has_attribute_list: contains_attribute_list(record),
        extents: parse_record_extents(record, record_source_offset, cluster_size)?,
    }))
}

fn parse_record_extents(
    record: &[u8],
    record_source_offset: u64,
    cluster_size: u64,
) -> io::Result<Vec<NtfsDataExtent>> {
    let mut extents = Vec::new();
    let mut position = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
    while position + 8 <= record.len() {
        let attribute_type =
            u32::from_le_bytes(record[position..position + 4].try_into().unwrap_or([0; 4]));
        if attribute_type == crate::ATTR_TYPE_END {
            break;
        }
        let length = u32::from_le_bytes(
            record[position + 4..position + 8]
                .try_into()
                .unwrap_or([0; 4]),
        ) as usize;
        if length == 0
            || position
                .checked_add(length)
                .is_none_or(|end| end > record.len())
        {
            break;
        }
        if attribute_type == crate::ATTR_TYPE_DATA && is_unnamed_attribute(record, position) {
            let resident_source_offset = resident_source_offset(record, position, length)
                .and_then(|offset| record_source_offset.checked_add(offset as u64));
            if let Some(extent) = parse_data_attribute_extent(record, position, length)? {
                if let Some(mapped) = map_extent(extent, resident_source_offset, cluster_size)? {
                    extents.push(mapped);
                }
            }
        }
        position += length;
    }
    extents.sort_by_key(|extent| extent.logical_offset);
    Ok(extents)
}

fn contains_attribute_list(record: &[u8]) -> bool {
    let mut position = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
    while position + 8 <= record.len() {
        let attribute_type =
            u32::from_le_bytes(record[position..position + 4].try_into().unwrap_or([0; 4]));
        if attribute_type == crate::ATTR_TYPE_END {
            return false;
        }
        let length = u32::from_le_bytes(
            record[position + 4..position + 8]
                .try_into()
                .unwrap_or([0; 4]),
        ) as usize;
        if length == 0
            || position
                .checked_add(length)
                .is_none_or(|end| end > record.len())
        {
            return false;
        }
        if attribute_type == crate::ATTR_TYPE_ATTRIBUTE_LIST {
            return true;
        }
        position += length;
    }
    false
}

fn resident_source_offset(record: &[u8], position: usize, length: usize) -> Option<usize> {
    if record.get(position + 8).copied()? & 1 != 0 || position + 0x16 > record.len() {
        return None;
    }
    let content_offset =
        u16::from_le_bytes(record[position + 0x14..position + 0x16].try_into().ok()?) as usize;
    let source_offset = position.checked_add(content_offset)?;
    (source_offset < position.checked_add(length)? && source_offset <= record.len())
        .then_some(source_offset)
}

fn map_extent(
    extent: DataAttributeExtent,
    resident_source_offset: Option<u64>,
    cluster_size: u64,
) -> io::Result<Option<NtfsDataExtent>> {
    match extent {
        DataAttributeExtent::Resident { data } => Ok(Some(NtfsDataExtent {
            logical_offset: 0,
            real_size: data.len() as u64,
            resident_source_offset,
            compressed: false,
            encrypted: false,
            sparse: false,
            runs: Vec::new(),
        })),
        DataAttributeExtent::NonResident {
            lowest_vcn,
            real_size,
            attr_flags,
            runs,
            ..
        } => {
            let logical_offset = lowest_vcn
                .checked_mul(cluster_size)
                .ok_or_else(|| invalid_fs_data("NTFS extent logical offset overflows"))?;
            Ok(Some(NtfsDataExtent {
                logical_offset,
                real_size,
                resident_source_offset: None,
                compressed: attr_flags & 0x0001 != 0,
                encrypted: attr_flags & 0x4000 != 0,
                sparse: runs.iter().any(|run| run.lcn.is_none()),
                runs,
            }))
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/recovery.rs"]
mod tests;
