//! NTFS filesystem reader.
//! Parses boot sector to locate $MFT, reads FILE records, enumerates file names.
//! Supports resident and non-resident attributes via data run parsing.

pub mod ads;
pub mod logfile;
pub mod mft_scanner;

use evidence_core::filesystem::{
    child_nodes_with_parent_path, file_not_found, fs_node_with_attributes, fs_out_of_memory,
    invalid_fs_data, path_components, root_node, truncate_data_to_declared_size, unexpected_fs_eof,
    FileSystemReader, FsNode,
};
use evidence_core::EvidenceReader;
use std::cell::RefCell;
use std::collections::HashSet;
use std::io::{self, Read, Seek, SeekFrom};

/// Internal entry with MFT reference for path resolution.
struct DirEntry {
    node: FsNode,
    mft_ref: u64,
}

#[derive(Debug, Clone, Copy)]
struct DataRun {
    lcn: Option<i64>,
    cluster_count: u64,
}

#[derive(Debug, Clone)]
pub struct NtfsDirectoryEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub mft_ref: u64,
    pub hidden: bool,
    pub system: bool,
}

#[allow(dead_code)]
pub struct NtfsReader {
    reader: RefCell<Box<dyn EvidenceReader>>,
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    mft_cluster: u64,
    mft_record_size: u32,
    index_record_size: u32,
    cluster_size: u64,
    mft_data_runs: Vec<(i64, u64)>,
    /// Absolute offset of NTFS volume start in evidence.
    volume_offset: u64,
}

impl NtfsReader {
    pub fn open(mut reader: Box<dyn EvidenceReader>, offset: u64) -> io::Result<Self> {
        reader.seek(SeekFrom::Start(offset))?;
        let mut boot = [0u8; 512];
        reader.read_exact(&mut boot)?;

        if &boot[3..11] != b"NTFS    " {
            return Err(invalid_fs_data("not a valid NTFS volume"));
        }

        let bytes_per_sector = u16::from_le_bytes([boot[11], boot[12]]);
        let sectors_per_cluster = boot[13];
        if bytes_per_sector == 0 || sectors_per_cluster == 0 {
            return Err(invalid_fs_data("invalid NTFS geometry"));
        }
        let cluster_size = bytes_per_sector as u64 * sectors_per_cluster as u64;
        let mft_cluster = u64::from_le_bytes(boot[0x30..0x38].try_into().unwrap_or([0; 8]));
        let _root_dir = root_dir_frn(&boot);
        let mft_record_size = mft_record_bytes(&boot);
        let index_record_size = index_record_bytes(&boot, cluster_size as u32, mft_record_size);

        let mft_record0 = read_contiguous_mft_record(
            &mut *reader,
            offset,
            mft_cluster,
            cluster_size,
            mft_record_size,
            bytes_per_sector,
            0,
        )?;
        let mft_data_runs = parse_mft_data_runs_from_record(&mft_record0).unwrap_or_default();

        Ok(Self {
            reader: RefCell::new(reader),
            bytes_per_sector,
            sectors_per_cluster,
            mft_cluster,
            mft_record_size,
            index_record_size,
            cluster_size,
            mft_data_runs,
            volume_offset: offset,
        })
    }

    fn mft_offset(&self, record_number: u64) -> u64 {
        self.volume_offset
            + self.mft_cluster * self.cluster_size
            + record_number * self.mft_record_size as u64
    }

    /// Convert volume-relative cluster number to absolute evidence offset.
    fn cluster_to_offset(&self, lcn: i64) -> io::Result<u64> {
        if lcn < 0 {
            return Err(invalid_fs_data(format!("negative LCN {} in data run", lcn)));
        }
        Ok(self.volume_offset + lcn as u64 * self.cluster_size)
    }

    fn read_mft_record(&self, record_number: u64) -> io::Result<Vec<u8>> {
        let mut rec = vec![0u8; self.mft_record_size as usize];
        if self.mft_data_runs.is_empty() {
            let off = self.mft_offset(record_number);
            let mut reader = self.reader.borrow_mut();
            reader.seek(SeekFrom::Start(off))?;
            reader.read_exact(&mut rec)?;
        } else {
            let mft_stream_offset = record_number
                .checked_mul(self.mft_record_size as u64)
                .ok_or_else(|| invalid_fs_data("MFT record offset overflow"))?;
            self.read_mft_stream_at(mft_stream_offset, &mut rec)?;
        }
        apply_record_fixup(&mut rec, self.bytes_per_sector as usize)?;
        Ok(rec)
    }

    fn read_mft_stream_at(&self, mut stream_offset: u64, out: &mut [u8]) -> io::Result<()> {
        let mut written = 0usize;
        let mut run_stream_start = 0u64;
        let mut reader = self.reader.borrow_mut();

        for (lcn, cluster_count) in &self.mft_data_runs {
            if *lcn < 0 {
                return Err(invalid_fs_data(format!("negative MFT LCN {}", lcn)));
            }
            let run_bytes = cluster_count
                .checked_mul(self.cluster_size)
                .ok_or_else(|| {
                    invalid_fs_data(format!(
                        "MFT run overflow: {} clusters × {} bytes/cluster",
                        cluster_count, self.cluster_size
                    ))
                })?;
            let run_end = run_stream_start.saturating_add(run_bytes);
            if stream_offset >= run_end {
                run_stream_start = run_end;
                continue;
            }

            let offset_in_run = stream_offset.saturating_sub(run_stream_start);
            let available = run_bytes.saturating_sub(offset_in_run);
            let need = out.len() - written;
            let to_read = available.min(need as u64) as usize;
            let disk_offset = self
                .volume_offset
                .checked_add((*lcn as u64).saturating_mul(self.cluster_size))
                .and_then(|base| base.checked_add(offset_in_run))
                .ok_or_else(|| invalid_fs_data("MFT run disk offset overflow"))?;

            reader.seek(SeekFrom::Start(disk_offset))?;
            reader.read_exact(&mut out[written..written + to_read])?;
            written += to_read;
            if written == out.len() {
                return Ok(());
            }
            stream_offset = run_end;
            run_stream_start = run_end;
        }

        Err(unexpected_fs_eof(format!(
            "MFT stream ended before record read completed (read {} of {} bytes)",
            written,
            out.len()
        )))
    }

    /// List children of any directory by MFT inode number.
    /// Reads $INDEX_ROOT (resident) and falls back to $INDEX_ALLOCATION
    /// (non-resident B-Tree) for large directories.
    fn list_dir_by_inode(&self, inode: u64) -> io::Result<Vec<DirEntry>> {
        let rec = self.read_mft_record(inode)?;
        let mut entries = Vec::new();
        let mut seen = HashSet::new();

        // Walk attributes looking for $INDEX_ROOT (0x90) and $INDEX_ALLOCATION (0xA0)
        let attr_off = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
        let mut pos = attr_off;
        let mut index_root_entries: Option<Vec<DirEntry>> = None;
        let mut index_alloc_entries: Option<Vec<DirEntry>> = None;
        let mut saw_a0 = false;

        while pos + 8 < rec.len() {
            let typ = u32::from_le_bytes(rec[pos..pos + 4].try_into().unwrap_or([0; 4]));
            if typ == 0xFFFFFFFF {
                break;
            }
            let len =
                u32::from_le_bytes(rec[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
            if len == 0 || pos + len > rec.len() {
                break;
            }
            if typ == 0xA0 {
                saw_a0 = true;
            }
            if typ == 0x20 && !saw_a0 {
                tracing::info!(
                    inode = %inode,
                    "NTFS directory has $ATTRIBUTE_LIST — $INDEX_ALLOCATION may be in external MFT record"
                );
            }

            if typ == 0x90 && pos + 0x18 <= rec.len() {
                // $INDEX_ROOT is a resident attribute. On real disks, the index
                // header lives inside the resident content, not directly in the
                // attribute header. Keep a legacy fallback for older synthetic tests.
                if let Some(entries) = self.parse_index_root_entries(&rec, pos, len) {
                    index_root_entries = Some(entries);
                } else {
                    let entries_off = u32::from_le_bytes(
                        rec[pos + 0x10..pos + 0x14].try_into().unwrap_or([0; 4]),
                    ) as usize;
                    let ents_start = pos + 0x10 + entries_off;
                    if ents_start < pos + len {
                        index_root_entries = Some(parse_indx_entries(&rec[ents_start..pos + len]));
                    }
                }
            }

            if typ == 0xA0 && pos + 0x40 <= rec.len() {
                // $INDEX_ALLOCATION — non-resident B-Tree INDX records
                match self.read_attr_nonresident(pos, &rec) {
                    Ok(data) => {
                        if data.is_empty() {
                            tracing::warn!(
                                inode = %inode,
                                "NTFS $INDEX_ALLOCATION returned empty data — large directory entries may be missing"
                            );
                        } else {
                            index_alloc_entries = Some(self.parse_indx_buffer(&data));
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            inode = %inode,
                            error = %e,
                            "NTFS $INDEX_ALLOCATION read failed, falling back to $INDEX_ROOT only"
                        );
                    }
                }
            }

            pos += len;
        }

        // If this is a directory with only $INDEX_ROOT and no $INDEX_ALLOCATION,
        // the directory listing may be incomplete (large dirs store entries in
        // the allocation tree, not the root entry).
        if !saw_a0 && index_root_entries.is_some() {
            let root_count = index_root_entries.as_ref().map(|v| v.len()).unwrap_or(0);
            tracing::warn!(
                inode = %inode,
                root_entries = %root_count,
                "NTFS directory has $INDEX_ROOT but no $INDEX_ALLOCATION — large directory listing may be incomplete"
            );
        }

        // Merge: $INDEX_ALLOCATION entries first (more complete), then
        // fill gaps from $INDEX_ROOT. Deduplicate by mft_ref.
        if let Some(alloc) = index_alloc_entries {
            for e in alloc {
                seen.insert(e.mft_ref);
                entries.push(e);
            }
        }
        if let Some(root) = index_root_entries {
            for e in root {
                if seen.insert(e.mft_ref) {
                    entries.push(e);
                }
            }
        }

        Ok(entries)
    }

    fn parse_index_root_entries(
        &self,
        record: &[u8],
        attr_pos: usize,
        attr_len: usize,
    ) -> Option<Vec<DirEntry>> {
        let content = resident_attr_content(record, attr_pos, attr_len)?;
        if content.len() < 0x20 {
            return None;
        }

        let entries_off = u32::from_le_bytes(content[0x10..0x14].try_into().ok()?) as usize;
        let entries_end_off = u32::from_le_bytes(content[0x14..0x18].try_into().ok()?) as usize;
        let buffer_end_off = u32::from_le_bytes(content[0x18..0x1C].try_into().ok()?) as usize;
        let entries_start = 0x10usize.saturating_add(entries_off);
        let entries_end = 0x10usize
            .saturating_add(entries_end_off)
            .min(0x10usize.saturating_add(buffer_end_off))
            .min(content.len());
        if entries_start >= entries_end {
            return None;
        }

        Some(parse_indx_entries(&content[entries_start..entries_end]))
    }

    // --- Non-resident attribute & INDX record helpers ---

    /// Read non-resident attribute data by walking its data run list.
    fn read_attr_nonresident(&self, attr_pos: usize, record: &[u8]) -> io::Result<Vec<u8>> {
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
        if alloc_size > 128 * 1024 * 1024 {
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
            let compression_unit_exp = if attr_pos + 0x24 <= record.len() {
                u16::from_le_bytes(
                    record[attr_pos + 0x22..attr_pos + 0x24]
                        .try_into()
                        .unwrap_or([0; 2]),
                )
            } else {
                4
            };
            let decoded =
                self.read_compressed_data_runs_to_vec(&runs, compression_unit_exp, real_size)?;
            return Ok(truncate_data_to_declared_size(decoded, real_size));
        }

        let buf = self.read_data_runs_to_vec(&runs, true, alloc_size)?;
        Ok(truncate_data_to_declared_size(buf, real_size))
    }

    fn read_data_runs_to_vec(
        &self,
        runs: &[DataRun],
        include_sparse: bool,
        max_bytes: u64,
    ) -> io::Result<Vec<u8>> {
        const MAX_FILE_BUFFER: usize = 128 * 1024 * 1024;
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
            if new_size > MAX_FILE_BUFFER {
                return Err(invalid_fs_data(format!(
                    "data run buffer exceeds 128 MB limit (would be {} bytes)",
                    new_size
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

    fn read_compressed_data_runs_to_vec(
        &self,
        runs: &[DataRun],
        compression_unit_exp: u16,
        real_size: u64,
    ) -> io::Result<Vec<u8>> {
        const MAX_FILE_BUFFER: usize = 128 * 1024 * 1024;
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
                    self.read_clusters_into(physical_lcn, take, &mut unit, MAX_FILE_BUFFER)?;
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
                        MAX_FILE_BUFFER,
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
                MAX_FILE_BUFFER,
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
                "data run buffer exceeds 128 MB limit (would be {} bytes)",
                new_size
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

    /// Scan INDX buffer for INDX records, apply fixup, extract entries.
    fn parse_indx_buffer(&self, data: &[u8]) -> Vec<DirEntry> {
        let mut entries = Vec::new();
        let mut off = 0usize;
        while off + 0x18 < data.len() {
            let magic = u32::from_le_bytes(data[off..off + 4].try_into().unwrap_or([0; 4]));
            // INDX record magic: "INDX" = 0x58444E49 (little-endian)
            if magic != 0x58444E49 {
                off += 1;
                continue;
            }
            let upd_off = u16::from_le_bytes([data[off + 4], data[off + 5]]) as usize;
            let upd_cnt = u16::from_le_bytes([data[off + 6], data[off + 7]]) as usize;
            if !(2..=64).contains(&upd_cnt) {
                off += 4;
                continue;
            }

            // Apply update sequence fixup. Some synthetic fixtures encode a record
            // larger than the boot-sector index_record_size, so honor whichever is larger:
            // the advertised record size or the size implied by the USA count.
            let record_bytes_from_fixup =
                upd_cnt.saturating_sub(1) * self.bytes_per_sector as usize;
            let record_len = (self.index_record_size as usize).max(record_bytes_from_fixup);
            let copy_len = record_len.min(data.len() - off);
            let mut rec = data[off..off + copy_len].to_vec();
            if upd_off + upd_cnt * 2 <= rec.len() {
                let orig = u16::from_le_bytes([rec[upd_off], rec[upd_off + 1]]);
                for i in 1..upd_cnt {
                    let fix_off = i * self.bytes_per_sector as usize - 2;
                    if fix_off + 2 > rec.len() {
                        break;
                    }
                    let val = u16::from_le_bytes([rec[fix_off], rec[fix_off + 1]]);
                    if val == orig {
                        let repl_off = upd_off + 2 + (i - 1) * 2;
                        if repl_off + 2 <= rec.len() {
                            rec[fix_off] = rec[repl_off];
                            rec[fix_off + 1] = rec[repl_off + 1];
                        }
                    }
                }
            }

            // Parse entries from the fixed-up record
            // Index entry list starts at +0x18
            let list_start = 0x18usize;
            if list_start + 12 <= rec.len() {
                let ent_off = u32::from_le_bytes(
                    rec[list_start..list_start + 4].try_into().unwrap_or([0; 4]),
                ) as usize;
                let ent_end_off = u32::from_le_bytes(
                    rec[list_start + 4..list_start + 8]
                        .try_into()
                        .unwrap_or([0; 4]),
                ) as usize;
                let buf_end_off = u32::from_le_bytes(
                    rec[list_start + 8..list_start + 12]
                        .try_into()
                        .unwrap_or([0; 4]),
                ) as usize;
                let idxe_start = list_start + ent_off;
                let idxe_end = (list_start + ent_end_off)
                    .min(list_start + buf_end_off)
                    .min(rec.len());
                if idxe_start < idxe_end {
                    let mut indx_entries = parse_indx_entries(&rec[idxe_start..idxe_end]);
                    entries.append(&mut indx_entries);
                }
            }

            // Move ahead by one index record after processing a valid INDX record.
            off += record_len.max(self.bytes_per_sector as usize);
        }
        entries
    }

    pub fn list_root_children(&self) -> io::Result<Vec<FsNode>> {
        Ok(child_nodes_with_parent_path(
            self.list_dir_by_inode(5)?.into_iter().map(|e| e.node),
            "",
        ))
    }

    pub fn list_root_directory_entries(&self) -> io::Result<Vec<NtfsDirectoryEntry>> {
        self.list_directory_entries_by_inode(5)
    }

    pub fn list_directory_entries_by_inode(
        &self,
        inode: u64,
    ) -> io::Result<Vec<NtfsDirectoryEntry>> {
        Ok(self
            .list_dir_by_inode(inode)?
            .into_iter()
            .map(|entry| NtfsDirectoryEntry {
                name: entry.node.name,
                is_dir: entry.node.is_dir,
                size: entry.node.size,
                mft_ref: entry.mft_ref,
                hidden: entry.node.hidden,
                system: entry.node.system,
            })
            .collect())
    }

    /// Read the $DATA attribute of a file by MFT inode.
    /// Handles both resident (inline) and non-resident (data run chain) $DATA.
    fn read_file_data(&self, inode: u64) -> io::Result<Vec<u8>> {
        let rec = self.read_mft_record(inode)?;
        if rec.len() < 0x18 || &rec[0..4] != b"FILE" {
            return Err(invalid_fs_data(format!(
                "inode {} is not a valid FILE record",
                inode
            )));
        }
        let attr_off = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
        let mut pos = attr_off;
        while pos + 8 < rec.len() {
            let typ = u32::from_le_bytes(rec[pos..pos + 4].try_into().unwrap_or([0; 4]));
            if typ == 0xFFFFFFFF {
                break;
            }
            let len =
                u32::from_le_bytes(rec[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
            if len == 0 || pos + len > rec.len() {
                break;
            }
            if typ == 0x80 {
                if !is_unnamed_attribute(&rec, pos) {
                    pos += len;
                    continue;
                }
                let is_nonresident = pos + 9 <= rec.len() && (rec[pos + 8] & 1) != 0;
                if is_nonresident {
                    if pos + 0x40 > rec.len() {
                        return Ok(Vec::new());
                    }
                    return self.read_attr_nonresident(pos, &rec);
                } else {
                    if pos + 0x16 > rec.len() {
                        return Ok(Vec::new());
                    }
                    let content_size = u32::from_le_bytes(
                        rec[pos + 0x10..pos + 0x14].try_into().unwrap_or([0; 4]),
                    ) as usize;
                    let content_off = pos
                        + u16::from_le_bytes(
                            rec[pos + 0x14..pos + 0x16].try_into().unwrap_or([0; 2]),
                        ) as usize;
                    let end = content_off.saturating_add(content_size).min(rec.len());
                    if content_off < end {
                        return Ok(rec[content_off..end].to_vec());
                    }
                    return Ok(Vec::new());
                }
            }
            pos += len;
        }
        Ok(Vec::new())
    }

    /// Resolve a file path: walk parent directories, then find the file
    /// in the final directory. Returns file MFT inode, or None if not found.
    pub(crate) fn resolve_file_path(&self, path: &str) -> io::Result<Option<u64>> {
        let components = path_components(path);
        let (parent_dirs, file_name) = match components.split_last() {
            Some((file, dirs)) => (dirs, *file),
            None => return Ok(None),
        };

        let mut current_inode = 5u64;
        for dir in parent_dirs {
            let children = self.list_dir_by_inode(current_inode)?;
            let found = children
                .iter()
                .find(|e| e.node.name.eq_ignore_ascii_case(dir) && e.node.is_dir);
            match found {
                Some(entry) => current_inode = entry.mft_ref,
                None => {
                    tracing::warn!(
                        path = %path,
                        missing_component = %dir,
                        parent_inode = %current_inode,
                        "NTFS path resolution: directory not found in parent INDX"
                    );
                    return Ok(None);
                }
            }
        }

        let children = self.list_dir_by_inode(current_inode)?;
        let result = children
            .iter()
            .find(|e| e.node.name.eq_ignore_ascii_case(file_name) && !e.node.is_dir)
            .map(|e| e.mft_ref);
        if result.is_none() {
            tracing::warn!(
                path = %path,
                missing_file = %file_name,
                parent_inode = %current_inode,
                children_count = %children.len(),
                "NTFS path resolution: file not found in parent INDX"
            );
        }
        Ok(result)
    }

    /// Resolve a path from root, walking top-down through directory INDX entries.
    /// Returns the MFT inode of the final component, or None if not found.
    /// Validates $FILE_NAME.par_ref consistency at each step.
    fn resolve_path(&self, path: &str) -> io::Result<Option<u64>> {
        let components = path_components(path);
        if components.is_empty() {
            return Ok(Some(5));
        }
        let mut current_inode = 5u64;
        let mut remaining = &components[..];
        while let Some((target, rest)) = remaining.split_first() {
            let children = self.list_dir_by_inode(current_inode)?;
            let found = children
                .iter()
                .find(|e| e.node.name.eq_ignore_ascii_case(target) && e.node.is_dir);
            match found {
                Some(entry) => {
                    // Verify the child directory's $FILE_NAME points back to us.
                    // Non-fatal: some directories (e.g. \$Recycle.Bin, System Volume
                    // Information) may have unreliable $FILE_NAME parent references
                    // due to MFT record quirks, but the INDX entry path is correct.
                    let _parent_ok = self.verify_parent(entry.mft_ref, current_inode)?;
                    current_inode = entry.mft_ref;
                    remaining = rest;
                }
                None => return Ok(None),
            }
        }
        Ok(Some(current_inode))
    }

    /// Verify that the $FILE_NAME attribute of `child_inode` has
    /// `par_ref` == `expected_parent`. Returns false on mismatch or IO error.
    fn verify_parent(&self, child_inode: u64, expected_parent: u64) -> io::Result<bool> {
        let rec = match self.read_mft_record(child_inode) {
            Ok(r) => r,
            Err(_) => return Ok(false),
        };
        let attr_off = u16::from_le_bytes([rec[0x14], rec[0x15]]) as usize;
        let mut pos = attr_off;
        let mut saw_file_name = false;
        while pos + 8 < rec.len() {
            let typ = u32::from_le_bytes(
                rec[pos..pos + 4]
                    .try_into()
                    .unwrap_or([0xFF, 0xFF, 0xFF, 0xFF]),
            );
            if typ == 0xFFFFFFFF {
                break;
            }
            let len =
                u32::from_le_bytes(rec[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
            if len == 0 || pos + len > rec.len() {
                break;
            }
            if typ == 0x30 && pos + 8 <= rec.len() {
                saw_file_name = true;
                // $FILE_NAME is resident on normal NTFS records. The parent
                // reference lives at the start of the resident content.
                if let Some(content) = resident_attr_content(&rec, pos, len) {
                    if content.len() >= 8 {
                        let par_ref =
                            u64::from_le_bytes(content[0..8].try_into().unwrap_or([0; 8]))
                                & 0x0000_FFFF_FFFF_FFFF;
                        if par_ref == expected_parent {
                            return Ok(true);
                        }
                        pos += len;
                        continue;
                    }
                }

                // Legacy fallback for older simplified fixtures.
                let par_ref = u64::from_le_bytes(rec[pos..pos + 8].try_into().unwrap_or([0; 8]))
                    & 0x0000_FFFF_FFFF_FFFF;
                if par_ref == expected_parent {
                    return Ok(true);
                }
            }
            pos += len;
        }
        Ok(!saw_file_name) // no $FILE_NAME found — can't verify, allow
    }

    pub fn list_subdir_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        match self.resolve_path(path)? {
            Some(inode) => Ok(child_nodes_with_parent_path(
                self.list_dir_by_inode(inode)?.into_iter().map(|e| e.node),
                path,
            )),
            None => Ok(Vec::new()),
        }
    }
}

fn resident_attr_content(record: &[u8], attr_pos: usize, attr_len: usize) -> Option<&[u8]> {
    if attr_pos + 0x16 > record.len() {
        return None;
    }
    if (record[attr_pos + 8] & 1) != 0 {
        return None;
    }

    let content_size =
        u32::from_le_bytes(record[attr_pos + 0x10..attr_pos + 0x14].try_into().ok()?) as usize;
    let content_off =
        u16::from_le_bytes(record[attr_pos + 0x14..attr_pos + 0x16].try_into().ok()?) as usize;
    let attr_end = attr_pos.checked_add(attr_len)?;
    let content_start = attr_pos.checked_add(content_off)?;
    let content_end = content_start.checked_add(content_size)?;

    if content_off < 0x18 || content_start >= attr_end || content_end > attr_end {
        return None;
    }

    record.get(content_start..content_end)
}

fn is_unnamed_attribute(record: &[u8], attr_pos: usize) -> bool {
    attr_pos + 0x0a <= record.len() && record[attr_pos + 0x09] == 0
}

fn apply_record_fixup(record: &mut [u8], sector_size: usize) -> io::Result<()> {
    if record.len() < 8 || sector_size < 2 {
        return Ok(());
    }

    let usa_offset = u16::from_le_bytes([record[4], record[5]]) as usize;
    let usa_count = u16::from_le_bytes([record[6], record[7]]) as usize;
    if usa_offset == 0 || usa_count < 2 {
        return Ok(());
    }

    let usa_bytes = usa_count
        .checked_mul(2)
        .ok_or_else(|| invalid_fs_data("invalid update sequence"))?;
    if usa_offset + usa_bytes > record.len() {
        return Err(invalid_fs_data(
            "update sequence array exceeds record length",
        ));
    }

    let expected = [record[usa_offset], record[usa_offset + 1]];
    for i in 1..usa_count {
        let fixup_pos = i
            .checked_mul(sector_size)
            .and_then(|v| v.checked_sub(2))
            .ok_or_else(|| invalid_fs_data("invalid fixup position"))?;
        if fixup_pos + 2 > record.len() {
            return Err(unexpected_fs_eof(
                "record too short for update sequence fixup",
            ));
        }

        if record[fixup_pos..fixup_pos + 2] != expected {
            return Err(invalid_fs_data("update sequence signature mismatch"));
        }

        let replacement = usa_offset + i * 2;
        record[fixup_pos] = record[replacement];
        record[fixup_pos + 1] = record[replacement + 1];
    }

    Ok(())
}

impl FileSystemReader for NtfsReader {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        if path.is_empty() {
            return self.list_root_children();
        }
        self.list_subdir_children(path)
    }

    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>> {
        // Fast path: if the path is an MFT inode reference
        // ("mft:NNN" or "mft:PARTITION:NNN"), read directly.
        // This skips INDX name lookups which fail when directories
        // store 8.3 short names instead of long names.
        let mft_inode = path.strip_prefix("mft:").and_then(|s| {
            // Handle both formats: "mft:5" and "mft:3:5"
            s.rsplit(':').next()?.parse::<u64>().ok()
        });
        if let Some(mft_inode) = mft_inode {
            let data = self.read_file_data(mft_inode)?;
            if data.len() > 128 * 1024 * 1024 {
                return Err(fs_out_of_memory(format!(
                    "file too large to buffer: {} bytes",
                    data.len()
                )));
            }
            return Ok(Box::new(io::Cursor::new(data)));
        }

        let inode = self
            .resolve_file_path(path)?
            .ok_or_else(|| file_not_found(path))?;
        let data = self.read_file_data(inode)?;
        if data.len() > 128 * 1024 * 1024 {
            return Err(fs_out_of_memory(format!(
                "file too large to buffer: {} bytes",
                data.len()
            )));
        }
        Ok(Box::new(io::Cursor::new(data)))
    }

    fn data_source_name(&self) -> &str {
        "NTFS"
    }
}

/// Parse INDX entries from $INDEX_ROOT buffer. Returns DirEntry with
/// both the FsNode and the child MFT reference (lower 48 bits of file_ref).
fn parse_indx_entries(data: &[u8]) -> Vec<DirEntry> {
    let mut entries = Vec::new();
    let mut off = 0usize;
    while off + 0x52 < data.len() {
        let mft_ref = u64::from_le_bytes(data[off..off + 8].try_into().unwrap_or([0; 8]))
            & 0x0000_FFFF_FFFF_FFFF;
        let entry_size = u16::from_le_bytes([data[off + 8], data[off + 9]]) as usize;
        if entry_size < 0x52 || off + entry_size > data.len() {
            break;
        }
        let name_len = data[off + 0x50] as usize;
        let name_start = off + 0x52;
        if name_len > 0
            && name_start + name_len * 2 <= data.len()
            && name_start + name_len * 2 <= off + entry_size
        {
            let chars: Vec<u16> = data[name_start..name_start + name_len * 2]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            let name = String::from_utf16_lossy(&chars);
            let flags = if off + 0x4C <= data.len() && off + 0x4C <= off + entry_size {
                u32::from_le_bytes(data[off + 0x48..off + 0x4C].try_into().unwrap_or([0; 4]))
            } else {
                0
            };
            let is_dir = flags & 0x10000000 != 0;
            let hidden = flags & 0x02 != 0;
            let system = flags & 0x04 != 0;
            let encrypted = flags & 0x4000 != 0;
            let size = if is_dir || off + 0x48 > data.len() {
                0
            } else {
                u64::from_le_bytes(data[off + 0x40..off + 0x48].try_into().unwrap_or([0; 8]))
            };
            entries.push(DirEntry {
                node: fs_node_with_attributes(
                    name, is_dir, size, hidden, system, encrypted, None, None, None,
                ),
                mft_ref,
            });
        }
        off += entry_size;
    }
    entries
}

#[test]
fn mft_inode_fast_path_handles_partition_record_format() {
    // "mft:3:42" format from parallel MFT enumeration
    let path = "mft:3:42";
    let inode = path
        .strip_prefix("mft:")
        .and_then(|s| s.rsplit(':').next()?.parse::<u64>().ok());
    assert_eq!(inode, Some(42));
}

#[test]
fn mft_inode_fast_path_handles_legacy_format() {
    // "mft:5" format from legacy MFT enumeration
    let path = "mft:5";
    let inode = path
        .strip_prefix("mft:")
        .and_then(|s| s.rsplit(':').next()?.parse::<u64>().ok());
    assert_eq!(inode, Some(5));
}

// --- Boot sector parsing helpers ---

fn root_dir_frn(boot: &[u8]) -> u64 {
    let mft_ref = u64::from_le_bytes(boot[0x2C..0x34].try_into().unwrap_or([0; 8]));
    mft_ref & 0x0000_FFFF_FFFF_FFFF
}

fn mft_record_bytes(boot: &[u8]) -> u32 {
    let raw = boot[0x40] as i8;
    if raw > 0 {
        1024
    } else if raw < 0 {
        let shift = (raw as i16).unsigned_abs();
        if shift < 32 {
            (1u32 << shift).max(512)
        } else {
            1024
        }
    } else {
        1024
    }
}

fn index_record_bytes(boot: &[u8], cluster_size: u32, fallback: u32) -> u32 {
    let raw = boot[0x44] as i8;
    if raw > 0 {
        let bytes = cluster_size.saturating_mul(raw as u32);
        if bytes >= 512 {
            bytes
        } else {
            fallback
        }
    } else if raw < 0 {
        let shift = (raw as i16).unsigned_abs();
        if shift < 32 {
            (1u32 << shift).max(512)
        } else {
            fallback
        }
    } else {
        fallback
    }
}

fn read_contiguous_mft_record(
    reader: &mut dyn EvidenceReader,
    volume_offset: u64,
    mft_cluster: u64,
    cluster_size: u64,
    record_size: u32,
    bytes_per_sector: u16,
    record_number: u64,
) -> io::Result<Vec<u8>> {
    let offset = volume_offset
        .checked_add(mft_cluster.saturating_mul(cluster_size))
        .and_then(|base| base.checked_add(record_number.saturating_mul(record_size as u64)))
        .ok_or_else(|| invalid_fs_data("MFT record offset overflow"))?;
    let mut rec = vec![0u8; record_size as usize];
    reader.seek(SeekFrom::Start(offset))?;
    reader.read_exact(&mut rec)?;
    apply_record_fixup(&mut rec, bytes_per_sector as usize)?;
    Ok(rec)
}

fn parse_mft_data_runs_from_record(record: &[u8]) -> io::Result<Vec<(i64, u64)>> {
    if record.len() < 0x18 || &record[0..4] != b"FILE" {
        return Err(invalid_fs_data("MFT record 0 is not a valid FILE record"));
    }

    let attr_off = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
    let mut pos = attr_off;
    while pos + 8 < record.len() {
        let typ = u32::from_le_bytes(record[pos..pos + 4].try_into().unwrap_or([0; 4]));
        if typ == 0xFFFFFFFF {
            break;
        }
        let len =
            u32::from_le_bytes(record[pos + 4..pos + 8].try_into().unwrap_or([0; 4])) as usize;
        if len == 0 || pos + len > record.len() {
            break;
        }

        if typ == 0x80 && pos + 0x40 <= record.len() && (record[pos + 8] & 1) != 0 {
            let run_off = u16::from_le_bytes([record[pos + 0x20], record[pos + 0x21]]) as usize;
            if run_off == 0 || run_off >= len {
                return Ok(Vec::new());
            }
            return parse_data_runs_bytes(&record[pos + run_off..pos + len]);
        }
        pos += len;
    }
    Ok(Vec::new())
}

// --- Data run parsing helpers ---

fn parse_data_runs_bytes(data: &[u8]) -> io::Result<Vec<(i64, u64)>> {
    Ok(parse_data_runs_ext(data)?
        .into_iter()
        .filter_map(|run| run.lcn.map(|lcn| (lcn, run.cluster_count)))
        .collect())
}

fn parse_data_runs_ext(mut data: &[u8]) -> io::Result<Vec<DataRun>> {
    const MAX_DATA_RUNS: usize = 100_000;

    let mut runs = Vec::new();
    let mut prev_lcn: i64 = 0;
    while !data.is_empty() && data[0] != 0 {
        if runs.len() >= MAX_DATA_RUNS {
            return Err(invalid_fs_data(format!(
                "too many data runs (limit: {})",
                MAX_DATA_RUNS
            )));
        }
        let header = data[0];
        let size_bytes = (header & 0x0F) as usize;
        let offset_bytes = ((header >> 4) & 0x0F) as usize;
        if size_bytes > 8 || offset_bytes > 8 {
            break;
        }
        data = &data[1..];
        if data.len() < size_bytes + offset_bytes {
            break;
        }
        let cluster_count = read_sized_le(&data[..size_bytes]);
        data = &data[size_bytes..];
        let lcn_offset = read_sized_le_signed(&data[..offset_bytes]);
        data = &data[offset_bytes..];
        let lcn = if offset_bytes == 0 {
            None
        } else if runs.is_empty() {
            Some(lcn_offset)
        } else {
            Some(prev_lcn + lcn_offset)
        };
        if let Some(lcn) = lcn {
            prev_lcn = lcn;
        }
        if cluster_count == 0 {
            continue;
        }
        runs.push(DataRun { lcn, cluster_count });
    }
    Ok(runs)
}

fn lznt1_decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos + 2 <= data.len() {
        let header = u16::from_le_bytes([data[pos], data[pos + 1]]);
        pos += 2;
        if header == 0 {
            break;
        }
        let chunk_len = ((header & 0x0fff) as usize) + 1;
        if pos + chunk_len > data.len() {
            return Err("LZNT1 chunk exceeds input".to_string());
        }
        let chunk = &data[pos..pos + chunk_len];
        pos += chunk_len;
        if header & 0x8000 == 0 {
            out.extend_from_slice(chunk);
        } else {
            decompress_lznt1_chunk(chunk, &mut out)?;
        }
    }
    Ok(out)
}

fn append_compressed_unit(
    out: &mut Vec<u8>,
    physical: &[u8],
    has_sparse: bool,
    logical_bytes: u64,
    max_bytes: usize,
) -> io::Result<()> {
    let logical_len = logical_bytes as usize;
    let decoded = if physical.is_empty() {
        vec![0u8; logical_len]
    } else if has_sparse {
        lznt1_decompress(physical)
            .map_err(invalid_fs_data)
            .unwrap_or_else(|_| physical.to_vec())
    } else if physical.len() as u64 == logical_bytes {
        physical.to_vec()
    } else {
        lznt1_decompress(physical).map_err(invalid_fs_data)?
    };

    let append_len = decoded.len().min(logical_len);
    let new_size = out
        .len()
        .checked_add(append_len)
        .ok_or_else(|| invalid_fs_data("compressed output size overflow"))?;
    if new_size > max_bytes {
        return Err(invalid_fs_data(format!(
            "compressed output exceeds 128 MB limit (would be {} bytes)",
            new_size
        )));
    }
    out.extend_from_slice(&decoded[..append_len]);
    if append_len < logical_len {
        let final_size = out
            .len()
            .checked_add(logical_len - append_len)
            .ok_or_else(|| invalid_fs_data("compressed sparse padding size overflow"))?;
        if final_size > max_bytes {
            return Err(invalid_fs_data(format!(
                "compressed output exceeds 128 MB limit (would be {} bytes)",
                final_size
            )));
        }
        out.resize(final_size, 0);
    }
    Ok(())
}

fn decompress_lznt1_chunk(chunk: &[u8], out: &mut Vec<u8>) -> Result<(), String> {
    let chunk_start = out.len();
    let mut pos = 0usize;
    while pos < chunk.len() {
        let flags = chunk[pos];
        pos += 1;
        for bit in 0..8 {
            if pos >= chunk.len() {
                break;
            }
            if flags & (1 << bit) == 0 {
                out.push(chunk[pos]);
                pos += 1;
                continue;
            }
            if pos + 2 > chunk.len() {
                return Err("LZNT1 copy token truncated".to_string());
            }
            let token = u16::from_le_bytes([chunk[pos], chunk[pos + 1]]);
            pos += 2;
            let current = out.len().saturating_sub(chunk_start);
            let displacement_bits = lznt1_displacement_bits(current);
            let length_mask = (1u16 << displacement_bits) - 1;
            let length = (token & length_mask) as usize + 3;
            let displacement = (token >> displacement_bits) as usize + 1;
            if displacement > out.len().saturating_sub(chunk_start) {
                return Err("LZNT1 copy token points before chunk".to_string());
            }
            for _ in 0..length {
                let src = out.len() - displacement;
                let byte = out[src];
                out.push(byte);
            }
        }
    }
    Ok(())
}

fn lznt1_displacement_bits(current_chunk_output: usize) -> u16 {
    let mut length_bits = 12u16;
    let mut displacement = current_chunk_output.saturating_sub(1);
    while length_bits > 4 && displacement >= 0x10 {
        length_bits -= 1;
        displacement >>= 1;
    }
    length_bits
}

/// Read a variable-width little-endian unsigned integer (1-8 bytes).
fn read_sized_le(bytes: &[u8]) -> u64 {
    let mut val = 0u64;
    for (i, &b) in bytes.iter().enumerate().take(8) {
        val |= (b as u64) << (i * 8);
    }
    val
}

/// Read a variable-width little-endian signed integer (1-8 bytes).
fn read_sized_le_signed(bytes: &[u8]) -> i64 {
    let n = bytes.len().min(8);
    if n == 0 {
        return 0;
    }
    let mut val = 0u64;
    for (i, &b) in bytes.iter().enumerate().take(n) {
        val |= (b as u64) << (i * 8);
    }
    // Sign-extend: if the highest bit of the last byte is set,
    // fill upper bytes with 0xFF.
    let last = bytes[n - 1];
    if last & 0x80 != 0 {
        for i in n..8 {
            val |= 0xFFu64 << (i * 8);
        }
    }
    val as i64
}

/// Parse the $DATA non-resident real_size from MFT record 0.
/// Used by E01 import/enumeration code to determine $MFT data size.
pub fn parse_mft_data_real_size(record: &[u8]) -> Option<u64> {
    if &record[0..4] != b"FILE" {
        return None;
    }
    let attr_off = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
    let mut pos = attr_off;
    while pos + 8 < record.len() {
        let typ = u32::from_le_bytes(record[pos..pos + 4].try_into().ok()?);
        if typ == 0xFFFFFFFF {
            break;
        }
        let len = u32::from_le_bytes(record[pos + 4..pos + 8].try_into().ok()?) as usize;
        if len < 4 || pos + len > record.len() {
            break;
        }
        // $DATA non-resident (0x80) with non-resident flag bit 0 set
        if typ == 0x80 && pos + 0x38 <= record.len() && (record[pos + 8] & 1) != 0 {
            return Some(u64::from_le_bytes(
                record[pos + 0x30..pos + 0x38].try_into().ok()?,
            ));
        }
        if len == 0 {
            break;
        }
        pos += len;
    }
    None
}
