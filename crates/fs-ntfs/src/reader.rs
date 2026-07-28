//! NTFS data extent reading.

use crate::attribute::{
    data_extent_logical_len, data_extent_logical_start, data_extents_declared_size,
    data_extents_logical_size, nonresident_compression_unit, parse_data_attribute_extent,
    sort_data_extents, DataAttributeExtent,
};
use crate::compression::append_compressed_unit;
use crate::data_runs::{data_runs_logical_size, parse_data_runs_ext};
use crate::utils::{is_extension_record_for, validate_file_record};
use crate::{
    fs_out_of_memory, invalid_fs_data, truncate_data_to_declared_size, ATTR_TYPE_DATA,
    ATTR_TYPE_END, MAX_BUFFERED_FILE_BYTES,
};
use std::io::{self, Read, Seek, SeekFrom};

impl crate::NtfsReader {
    /// Read non-resident attribute data by walking its data run list.
    pub(crate) fn read_attr_nonresident(
        &self,
        attr_pos: usize,
        record: &[u8],
    ) -> io::Result<Vec<u8>> {
        // Verify non-resident flag
        if attr_pos + 9 > record.len() || (record[attr_pos + 8] & 1) == 0 {
            return Ok(Vec::new());
        }
        // data_run_offset is at +0x20 in the non-resident header
        let run_off =
            u16::from_le_bytes([record[attr_pos + 0x20], record[attr_pos + 0x21]]) as usize;
        // allocated size at +0x28
        let alloc_size = u64::from_le_bytes(
            record[attr_pos + 0x28..attr_pos + 0x30]
                .try_into()
                .unwrap_or([0; 8]),
        );
        if run_off == 0 || alloc_size == 0 || attr_pos + run_off >= record.len() {
            return Ok(Vec::new());
        }

        // Upper bound to avoid OOM on corrupt data
        if alloc_size > MAX_BUFFERED_FILE_BYTES as u64 {
            return Err(invalid_fs_data(format!(
                "attribute allocation too large: {} bytes",
                alloc_size
            )));
        }

        let attr_flags = u16::from_le_bytes(
            record[attr_pos + 0x0c..attr_pos + 0x0e]
                .try_into()
                .unwrap_or([0; 2]),
        );
        let real_size = u64::from_le_bytes(
            record[attr_pos + 0x30..attr_pos + 0x38]
                .try_into()
                .unwrap_or([0; 8]),
        );
        let runs = parse_data_runs_ext(&record[attr_pos + run_off..])?;

        if attr_flags & 0x0001 != 0 {
            let compression_unit_exp = nonresident_compression_unit(record, attr_pos);
            let decoded =
                self.read_compressed_data_runs_to_vec(&runs, compression_unit_exp, real_size)?;
            return Ok(truncate_data_to_declared_size(decoded, real_size));
        }

        let buf = self.read_data_runs_to_vec(&runs, true, alloc_size)?;
        Ok(truncate_data_to_declared_size(buf, real_size))
    }

    fn read_data_runs_to_vec(
        &self,
        runs: &[crate::DataRun],
        include_sparse: bool,
        max_bytes: u64,
    ) -> io::Result<Vec<u8>> {
        let mut buf = Vec::new();
        let mut reader = self.reader.borrow_mut();

        for run in runs {
            let chunk = run
                .cluster_count
                .checked_mul(self.cluster_size)
                .ok_or_else(|| {
                    invalid_fs_data(format!(
                        "data run overflow: {} clusters × {} bytes/cluster",
                        run.cluster_count, self.cluster_size
                    ))
                })?;
            if max_bytes > 0 && buf.len() as u64 >= max_bytes {
                break;
            }
            let to_append = if max_bytes > 0 {
                chunk.min(max_bytes.saturating_sub(buf.len() as u64))
            } else {
                chunk
            } as usize;
            if to_append == 0 {
                continue;
            }
            let new_size = buf
                .len()
                .checked_add(to_append)
                .ok_or_else(|| invalid_fs_data("data run buffer size overflow"))?;
            if new_size > MAX_BUFFERED_FILE_BYTES {
                return Err(invalid_fs_data(format!(
                    "data run buffer exceeds {} byte limit (would be {} bytes)",
                    MAX_BUFFERED_FILE_BYTES, new_size
                )));
            }

            match run.lcn {
                Some(lcn) => {
                    let offset = self.cluster_to_offset(lcn)?;
                    let start = buf.len();
                    buf.resize(new_size, 0);
                    reader.seek(SeekFrom::Start(offset))?;
                    reader.read_exact(&mut buf[start..])?;
                }
                None if include_sparse => {
                    buf.resize(new_size, 0);
                }
                None => {}
            }
        }

        Ok(buf)
    }

    fn read_data_runs_range(
        &self,
        runs: &[crate::DataRun],
        offset: u64,
        length: usize,
    ) -> io::Result<Vec<u8>> {
        let mut out = vec![0u8; length];
        if length == 0 {
            return Ok(out);
        }

        let request_end = offset
            .checked_add(length as u64)
            .ok_or_else(|| invalid_fs_data("requested range offset overflow"))?;
        let mut logical_start = 0u64;
        let mut reader = self.reader.borrow_mut();

        for run in runs {
            let run_bytes = run
                .cluster_count
                .checked_mul(self.cluster_size)
                .ok_or_else(|| {
                    invalid_fs_data(format!(
                        "data run overflow: {} clusters × {} bytes/cluster",
                        run.cluster_count, self.cluster_size
                    ))
                })?;
            let run_end = logical_start
                .checked_add(run_bytes)
                .ok_or_else(|| invalid_fs_data("data run logical offset overflow"))?;

            if run_end <= offset {
                logical_start = run_end;
                continue;
            }
            if logical_start >= request_end {
                break;
            }

            let overlap_start = offset.max(logical_start);
            let overlap_end = request_end.min(run_end);
            if overlap_start < overlap_end {
                let out_start = usize::try_from(overlap_start - offset)
                    .map_err(|_| invalid_fs_data("range output offset overflow"))?;
                let out_len = usize::try_from(overlap_end - overlap_start)
                    .map_err(|_| invalid_fs_data("range output length overflow"))?;

                if let Some(lcn) = run.lcn {
                    let run_relative = overlap_start - logical_start;
                    let disk_offset = self
                        .cluster_to_offset(lcn)?
                        .checked_add(run_relative)
                        .ok_or_else(|| invalid_fs_data("data run disk offset overflow"))?;
                    reader.seek(SeekFrom::Start(disk_offset))?;
                    reader.read_exact(&mut out[out_start..out_start + out_len])?;
                }
            }

            logical_start = run_end;
        }

        Ok(out)
    }

    fn read_compressed_data_runs_to_vec(
        &self,
        runs: &[crate::DataRun],
        compression_unit_exp: u16,
        real_size: u64,
    ) -> io::Result<Vec<u8>> {
        let unit_clusters = 1u64
            .checked_shl(compression_unit_exp.min(20) as u32)
            .filter(|value| *value > 0)
            .unwrap_or(16);
        let unit_bytes = unit_clusters
            .checked_mul(self.cluster_size)
            .ok_or_else(|| invalid_fs_data("compressed unit size overflow"))?;
        let mut out = Vec::new();
        let mut unit = Vec::new();
        let mut unit_logical_clusters = 0u64;
        let mut unit_has_sparse = false;

        for run in runs {
            let mut consumed = 0u64;
            while consumed < run.cluster_count && out.len() as u64 <= real_size {
                let unit_remaining = unit_clusters.saturating_sub(unit_logical_clusters);
                let take = (run.cluster_count - consumed).min(unit_remaining);
                if take == 0 {
                    break;
                }

                if let Some(lcn) = run.lcn {
                    let physical_lcn = lcn
                        .checked_add(consumed as i64)
                        .ok_or_else(|| invalid_fs_data("compressed data run LCN overflow"))?;
                    self.read_clusters_into(
                        physical_lcn,
                        take,
                        &mut unit,
                        MAX_BUFFERED_FILE_BYTES,
                    )?;
                } else {
                    unit_has_sparse = true;
                }

                unit_logical_clusters += take;
                consumed += take;
                if unit_logical_clusters == unit_clusters {
                    append_compressed_unit(
                        &mut out,
                        &unit,
                        unit_has_sparse,
                        unit_bytes,
                        MAX_BUFFERED_FILE_BYTES,
                    )?;
                    unit.clear();
                    unit_logical_clusters = 0;
                    unit_has_sparse = false;
                }
            }
        }

        if unit_logical_clusters > 0 && out.len() as u64 <= real_size {
            let logical_bytes = unit_logical_clusters
                .checked_mul(self.cluster_size)
                .ok_or_else(|| invalid_fs_data("compressed partial unit size overflow"))?;
            append_compressed_unit(
                &mut out,
                &unit,
                unit_has_sparse,
                logical_bytes,
                MAX_BUFFERED_FILE_BYTES,
            )?;
        }

        Ok(out)
    }

    fn read_clusters_into(
        &self,
        lcn: i64,
        cluster_count: u64,
        out: &mut Vec<u8>,
        max_bytes: usize,
    ) -> io::Result<()> {
        let bytes = cluster_count
            .checked_mul(self.cluster_size)
            .ok_or_else(|| {
                invalid_fs_data(format!(
                    "data run overflow: {} clusters × {} bytes/cluster",
                    cluster_count, self.cluster_size
                ))
            })? as usize;
        let new_size = out
            .len()
            .checked_add(bytes)
            .ok_or_else(|| invalid_fs_data("data run buffer size overflow"))?;
        if new_size > max_bytes {
            return Err(invalid_fs_data(format!(
                "data run buffer exceeds {} byte limit (would be {} bytes)",
                max_bytes, new_size
            )));
        }

        let offset = self.cluster_to_offset(lcn)?;
        let start = out.len();
        out.resize(new_size, 0);
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(offset))?;
        reader.read_exact(&mut out[start..])?;
        Ok(())
    }

    /// Read the $DATA attribute of a file by MFT inode.
    /// Handles both resident (inline) and non-resident (data run chain) $DATA.
    pub(crate) fn read_file_data(&self, inode: u64) -> io::Result<Vec<u8>> {
        let extents = self.collect_unnamed_data_extents(inode)?;
        if extents.is_empty() {
            return Ok(Vec::new());
        }

        self.read_data_extents_to_vec(&extents)
    }

    pub(crate) fn read_file_data_range(
        &self,
        inode: u64,
        offset: u64,
        length: usize,
    ) -> io::Result<Vec<u8>> {
        if length == 0 {
            return Ok(Vec::new());
        }

        let rec = self.read_mft_record(inode)?;
        validate_file_record(&rec, inode)?;

        let extents = self.collect_unnamed_data_extents_from_base(inode, rec)?;
        if extents.is_empty() {
            return Ok(Vec::new());
        }

        self.read_data_extents_range(&extents, offset, length)
    }

    pub(crate) fn collect_unnamed_data_extents(
        &self,
        inode: u64,
    ) -> io::Result<Vec<DataAttributeExtent>> {
        let rec = self.read_mft_record(inode)?;
        self.collect_unnamed_data_extents_from_base(inode, rec)
    }

    fn collect_unnamed_data_extents_from_base(
        &self,
        inode: u64,
        rec: Vec<u8>,
    ) -> io::Result<Vec<DataAttributeExtent>> {
        validate_file_record(&rec, inode)?;

        let mut extents = Vec::new();
        self.collect_data_extents_from_record(&rec, &mut extents)?;

        let external_records = self.external_attribute_records_for_unnamed_data(inode, &rec)?;
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
        let attr_off = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
        let mut pos = attr_off;
        while pos + 8 < record.len() {
            let typ = u32::from_le_bytes(record[pos..pos + 4].try_into().unwrap_or([0; 4]));
            if typ == ATTR_TYPE_END {
                break;
            }
            let len =
                u32::from_le_bytes(record[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
            if len == 0 || pos + len > record.len() {
                break;
            }

            if typ == ATTR_TYPE_DATA && crate::attribute::is_unnamed_attribute(record, pos) {
                if let Some(extent) = parse_data_attribute_extent(record, pos, len)? {
                    extents.push(extent);
                }
            }

            pos += len;
        }

        Ok(())
    }

    fn read_data_extents_to_vec(&self, extents: &[DataAttributeExtent]) -> io::Result<Vec<u8>> {
        let data_len = data_extents_logical_size(extents, self.cluster_size)?;
        if data_len as usize > MAX_BUFFERED_FILE_BYTES {
            return Err(invalid_fs_data(format!(
                "data run buffer exceeds {} byte limit (would be {} bytes)",
                MAX_BUFFERED_FILE_BYTES, data_len
            )));
        }

        let mut out = vec![0u8; data_len as usize];
        for extent in extents {
            let extent_start = data_extent_logical_start(extent, self.cluster_size)?;
            let extent_bytes = self.read_data_extent_to_vec(extent)?;
            let start = usize::try_from(extent_start)
                .map_err(|_| invalid_fs_data("data extent offset too large"))?;
            if start >= out.len() {
                continue;
            }
            let end = start.saturating_add(extent_bytes.len()).min(out.len());
            out[start..end].copy_from_slice(&extent_bytes[..end - start]);
        }

        Ok(truncate_data_to_declared_size(
            out,
            data_extents_declared_size(extents, self.cluster_size)?,
        ))
    }

    fn read_data_extent_to_vec(&self, extent: &DataAttributeExtent) -> io::Result<Vec<u8>> {
        match extent {
            DataAttributeExtent::Resident { data } => Ok(data.clone()),
            DataAttributeExtent::NonResident {
                allocated_size,
                real_size,
                attr_flags,
                compression_unit_exp,
                runs,
                ..
            } => {
                if *attr_flags & 0x0001 != 0 {
                    let decoded = self.read_compressed_data_runs_to_vec(
                        runs,
                        *compression_unit_exp,
                        *real_size,
                    )?;
                    return Ok(truncate_data_to_declared_size(decoded, *real_size));
                }

                let allocated = data_runs_logical_size(runs, self.cluster_size)?;
                let allocated = if *allocated_size > 0 {
                    (*allocated_size).min(allocated)
                } else {
                    allocated
                };
                let buf = self.read_data_runs_to_vec(runs, true, allocated)?;
                Ok(buf)
            }
        }
    }

    pub(crate) fn read_data_extents_range(
        &self,
        extents: &[DataAttributeExtent],
        offset: u64,
        length: usize,
    ) -> io::Result<Vec<u8>> {
        let logical_size = data_extents_declared_size(extents, self.cluster_size)?;
        if offset >= logical_size {
            return Ok(Vec::new());
        }

        let length_u64 = u64::try_from(length)
            .map_err(|_| fs_out_of_memory("requested range length is too large"))?;
        let bounded_len = length_u64.min(logical_size.saturating_sub(offset));
        let bounded_len = usize::try_from(bounded_len)
            .map_err(|_| fs_out_of_memory("requested range length is too large"))?;
        let mut out = vec![0u8; bounded_len];
        let request_end = offset
            .checked_add(bounded_len as u64)
            .ok_or_else(|| invalid_fs_data("requested range offset overflow"))?;

        for extent in extents {
            let extent_start = data_extent_logical_start(extent, self.cluster_size)?;
            let extent_len = data_extent_logical_len(extent, self.cluster_size)?;
            let extent_end = extent_start
                .checked_add(extent_len)
                .ok_or_else(|| invalid_fs_data("data extent logical offset overflow"))?;
            if extent_end <= offset || extent_start >= request_end {
                continue;
            }

            let overlap_start = offset.max(extent_start);
            let overlap_end = request_end.min(extent_end);
            let out_start = usize::try_from(overlap_start - offset)
                .map_err(|_| invalid_fs_data("range output offset overflow"))?;
            let out_len = usize::try_from(overlap_end - overlap_start)
                .map_err(|_| invalid_fs_data("range output length overflow"))?;

            let bytes = self.read_data_extent_range(
                extent,
                overlap_start.saturating_sub(extent_start),
                out_len,
            )?;
            let copy_len = bytes.len().min(out_len);
            out[out_start..out_start + copy_len].copy_from_slice(&bytes[..copy_len]);
        }

        Ok(out)
    }

    fn read_data_extent_range(
        &self,
        extent: &DataAttributeExtent,
        offset: u64,
        length: usize,
    ) -> io::Result<Vec<u8>> {
        match extent {
            DataAttributeExtent::Resident { data } => {
                let Ok(start) = usize::try_from(offset) else {
                    return Ok(Vec::new());
                };
                if start >= data.len() {
                    return Ok(Vec::new());
                }
                let end = start.saturating_add(length).min(data.len());
                Ok(data[start..end].to_vec())
            }
            DataAttributeExtent::NonResident {
                attr_flags, runs, ..
            } => {
                let extent_len = data_extent_logical_len(extent, self.cluster_size)?;
                if offset >= extent_len {
                    return Ok(Vec::new());
                }
                let length_u64 = u64::try_from(length)
                    .map_err(|_| fs_out_of_memory("requested range length is too large"))?;
                let bounded_len = length_u64.min(extent_len.saturating_sub(offset));
                let bounded_len = usize::try_from(bounded_len)
                    .map_err(|_| fs_out_of_memory("requested range length is too large"))?;
                if *attr_flags & 0x0001 != 0 {
                    return Err(invalid_fs_data(
                        "range reads for compressed NTFS data are not supported",
                    ));
                }
                self.read_data_runs_range(runs, offset, bounded_len)
            }
        }
    }
}
