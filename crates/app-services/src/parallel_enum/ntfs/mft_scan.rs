use super::super::error::ParallelEnumError;
use super::super::partition_work::PartitionWork;
use evidence_core::{EvidenceReader, LocalDiskReader, RawImageReader};
use fs_ntfs::mft_scanner::{MftRecord, MftScanner};
use image_e01::E01Reader;
use std::io::{Read, Seek, SeekFrom};

const MFT_CHUNK_RECORDS: u64 = 10_000;
const MFT_FALLBACK_SIZE: u64 = 100 * 1024 * 1024;

pub(in crate::parallel_enum) struct MftScan {
    reader: Option<Box<dyn EvidenceReader>>,
    params: NtfsMftParams,
    scanner: MftScanner,
    total_records: u64,
    buffer: Vec<u8>,
}

impl MftScan {
    pub(super) fn total_records(&self) -> u64 {
        self.total_records
    }

    pub(in crate::parallel_enum) fn read_chunk(
        &mut self,
        start_record: u64,
    ) -> Result<(Vec<MftRecord>, u64), ParallelEnumError> {
        let chunk_count = MFT_CHUNK_RECORDS.min(self.total_records - start_record);
        let byte_count = chunk_count * self.scanner.record_size() as u64;
        self.buffer.resize(byte_count as usize, 0);
        let stream_offset = start_record * self.scanner.record_size() as u64;

        let reader = self.reader.as_mut().ok_or_else(|| {
            ParallelEnumError::MftParams("MFT evidence reader is unavailable".to_string())
        })?;
        if self.params.mft_data_runs.is_empty() {
            reader
                .seek(SeekFrom::Start(
                    self.scanner.mft_abs_offset() + stream_offset,
                ))
                .map_err(ParallelEnumError::Io)?;
            reader
                .read_exact(&mut self.buffer)
                .map_err(ParallelEnumError::Io)?;
        } else {
            read_ntfs_mft_stream(
                &mut **reader,
                self.params.volume_offset,
                self.params.cluster_size,
                &self.params.mft_data_runs,
                stream_offset,
                &mut self.buffer,
            )
            .map_err(ParallelEnumError::Io)?;
        }

        Ok((
            self.scanner
                .parse_chunk(&self.buffer, start_record, chunk_count),
            chunk_count,
        ))
    }

    pub(in crate::parallel_enum) fn release_buffer(&mut self) {
        self.buffer = Vec::new();
    }

    pub(in crate::parallel_enum) fn take_reader(
        &mut self,
    ) -> Result<Box<dyn EvidenceReader>, ParallelEnumError> {
        self.reader.take().ok_or_else(|| {
            ParallelEnumError::MftParams("MFT evidence reader is unavailable".to_string())
        })
    }
}

pub(in crate::parallel_enum) fn prepare_mft_scan(
    partition: &PartitionWork,
) -> Result<MftScan, ParallelEnumError> {
    let reader = open_partition_evidence_reader(partition)?;
    prepare_mft_scan_from_reader(reader, partition.volume_offset)
}

pub(in crate::parallel_enum) fn prepare_mft_scan_from_reader(
    mut reader: Box<dyn EvidenceReader>,
    volume_offset: u64,
) -> Result<MftScan, ParallelEnumError> {
    let params = read_ntfs_mft_parameters_at(&mut *reader, volume_offset)?;
    if params.mft_data_size == 0 {
        return Err(ParallelEnumError::MftParams(
            "MFT data size is zero".to_string(),
        ));
    }
    let scanner = MftScanner::new(
        params.volume_offset,
        params.mft_cluster,
        params.cluster_size,
        params.record_size,
        params.bytes_per_sector,
        params.mft_data_size,
    );
    let total_records = scanner.total_records();
    if total_records == 0 {
        return Err(ParallelEnumError::MftParams(
            "MFT total record count is zero".to_string(),
        ));
    }
    Ok(MftScan {
        reader: Some(reader),
        params,
        scanner,
        total_records,
        buffer: Vec::new(),
    })
}

#[derive(Debug, Clone)]
pub(in crate::parallel_enum) struct NtfsMftParams {
    pub(in crate::parallel_enum) volume_offset: u64,
    pub(in crate::parallel_enum) mft_cluster: u64,
    pub(in crate::parallel_enum) cluster_size: u64,
    pub(in crate::parallel_enum) record_size: u32,
    pub(in crate::parallel_enum) bytes_per_sector: u16,
    pub(in crate::parallel_enum) mft_data_size: u64,
    pub(in crate::parallel_enum) mft_data_runs: Vec<(i64, u64)>,
}

pub(in crate::parallel_enum) fn read_ntfs_mft_parameters_at(
    reader: &mut dyn EvidenceReader,
    volume_offset: u64,
) -> Result<NtfsMftParams, ParallelEnumError> {
    reader
        .seek(SeekFrom::Start(volume_offset))
        .map_err(|error| format!("Seek NTFS boot sector: {error}"))?;
    let mut boot = [0; 512];
    reader
        .read_exact(&mut boot)
        .map_err(|error| format!("Read NTFS boot sector: {error}"))?;
    validate_boot_sector(&boot)?;

    let bytes_per_sector = u16::from_le_bytes([boot[11], boot[12]]);
    let sectors_per_cluster = boot[13];
    let cluster_size = bytes_per_sector as u64 * sectors_per_cluster as u64;
    let mft_cluster = u64::from_le_bytes(boot[0x30..0x38].try_into().unwrap_or([0; 8]));
    let record_size = mft_record_size_from_boot(&boot);
    let mut record = read_mft_record_zero(
        reader,
        volume_offset + mft_cluster * cluster_size,
        record_size,
    )?;
    apply_ntfs_record_fixup(&mut record, bytes_per_sector as usize)
        .map_err(|error| format!("Fix up MFT record 0: {error}"))?;

    Ok(NtfsMftParams {
        volume_offset,
        mft_cluster,
        cluster_size,
        record_size,
        bytes_per_sector,
        mft_data_size: parse_mft_data_size(&record).unwrap_or(MFT_FALLBACK_SIZE),
        mft_data_runs: parse_mft_data_runs_from_record(&record)
            .map_err(|error| format!("Parse MFT data runs: {error}"))?,
    })
}

fn validate_boot_sector(boot: &[u8; 512]) -> Result<(), ParallelEnumError> {
    if &boot[3..11] != b"NTFS    " {
        return Err(ParallelEnumError::MftParams(
            "not an NTFS boot sector".to_string(),
        ));
    }
    if u16::from_le_bytes([boot[11], boot[12]]) == 0 || boot[13] == 0 {
        return Err(ParallelEnumError::MftParams(
            "invalid NTFS geometry".to_string(),
        ));
    }
    Ok(())
}

fn read_mft_record_zero(
    reader: &mut dyn EvidenceReader,
    offset: u64,
    record_size: u32,
) -> Result<Vec<u8>, ParallelEnumError> {
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|error| format!("Seek MFT record 0: {error}"))?;
    let mut record = vec![0; record_size as usize];
    reader
        .read_exact(&mut record)
        .map_err(|error| format!("Read MFT record 0: {error}"))?;
    Ok(record)
}

pub(in crate::parallel_enum) fn open_partition_evidence_reader(
    partition: &PartitionWork,
) -> Result<Box<dyn EvidenceReader>, ParallelEnumError> {
    if partition.uses_e01_reader() {
        return Ok(Box::new(
            E01Reader::open(&partition.source_path).map_err(|error| error.to_string())?,
        ));
    }
    if partition.source_kind.eq_ignore_ascii_case("localdisk")
        || partition.source_kind.eq_ignore_ascii_case("local_disk")
    {
        return Ok(Box::new(
            LocalDiskReader::open(&partition.source_path).map_err(|error| error.to_string())?,
        ));
    }
    Ok(Box::new(
        RawImageReader::open(&partition.source_path).map_err(|error| error.to_string())?,
    ))
}

fn mft_record_size_from_boot(boot: &[u8]) -> u32 {
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

fn parse_mft_data_size(record: &[u8]) -> Option<u64> {
    if record.len() < 4 || &record[0..4] != b"FILE" {
        return None;
    }
    let mut position = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
    while position + 8 < record.len() {
        let attribute_type = u32::from_le_bytes(record[position..position + 4].try_into().ok()?);
        if attribute_type == 0xFFFF_FFFF {
            break;
        }
        let length =
            u32::from_le_bytes(record[position + 4..position + 8].try_into().ok()?) as usize;
        if length < 4 || position + length > record.len() {
            break;
        }
        if attribute_type == 0x80
            && position + 0x38 <= record.len()
            && record[position + 8] & 1 != 0
        {
            return Some(u64::from_le_bytes(
                record[position + 0x30..position + 0x38].try_into().ok()?,
            ));
        }
        position += length;
    }
    None
}

fn apply_ntfs_record_fixup(record: &mut [u8], sector_size: usize) -> Result<(), String> {
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
        .ok_or_else(|| "invalid update sequence".to_string())?;
    if usa_offset + usa_bytes > record.len() {
        return Err("update sequence array exceeds record length".to_string());
    }
    let expected = [record[usa_offset], record[usa_offset + 1]];
    for index in 1..usa_count {
        let position = index
            .checked_mul(sector_size)
            .and_then(|value| value.checked_sub(2))
            .ok_or_else(|| "invalid fixup position".to_string())?;
        if position + 2 > record.len() {
            return Err("record too short for update sequence fixup".to_string());
        }
        if record[position..position + 2] != expected {
            return Err("update sequence signature mismatch".to_string());
        }
        let replacement = usa_offset + index * 2;
        record[position] = record[replacement];
        record[position + 1] = record[replacement + 1];
    }
    Ok(())
}

fn parse_mft_data_runs_from_record(record: &[u8]) -> Result<Vec<(i64, u64)>, String> {
    if record.len() < 0x18 || &record[0..4] != b"FILE" {
        return Err("MFT record 0 is not a valid FILE record".to_string());
    }
    let mut position = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
    while position + 8 < record.len() {
        let attribute_type = u32::from_le_bytes(
            record[position..position + 4]
                .try_into()
                .map_err(|_| "Invalid MFT attribute type".to_string())?,
        );
        if attribute_type == 0xFFFF_FFFF {
            break;
        }
        let length = u32::from_le_bytes(
            record[position + 4..position + 8]
                .try_into()
                .map_err(|_| "Invalid MFT attribute length".to_string())?,
        ) as usize;
        if length == 0 || position + length > record.len() {
            break;
        }
        if attribute_type == 0x80
            && position + 0x40 <= record.len()
            && record[position + 8] & 1 != 0
        {
            let run_offset =
                u16::from_le_bytes([record[position + 0x20], record[position + 0x21]]) as usize;
            if run_offset == 0 || run_offset >= length {
                return Ok(Vec::new());
            }
            return parse_ntfs_data_runs(&record[position + run_offset..position + length]);
        }
        position += length;
    }
    Ok(Vec::new())
}

fn parse_ntfs_data_runs(mut data: &[u8]) -> Result<Vec<(i64, u64)>, String> {
    const MAX_DATA_RUNS: usize = 100_000;
    let mut runs = Vec::new();
    let mut previous_lcn = 0;
    while !data.is_empty() && data[0] != 0 {
        if runs.len() >= MAX_DATA_RUNS {
            return Err(format!("too many data runs (limit: {MAX_DATA_RUNS})"));
        }
        let size_bytes = (data[0] & 0x0F) as usize;
        let offset_bytes = ((data[0] >> 4) & 0x0F) as usize;
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
        let lcn = previous_lcn + lcn_offset;
        previous_lcn = lcn;
        if cluster_count > 0 {
            runs.push((lcn, cluster_count));
        }
    }
    Ok(runs)
}

pub(in crate::parallel_enum) fn read_ntfs_mft_stream(
    reader: &mut dyn EvidenceReader,
    volume_offset: u64,
    cluster_size: u64,
    runs: &[(i64, u64)],
    mut stream_offset: u64,
    output: &mut [u8],
) -> std::io::Result<()> {
    let mut written = 0;
    let mut run_start = 0u64;
    for (lcn, cluster_count) in runs {
        let run_bytes = checked_run_bytes(*lcn, *cluster_count, cluster_size)?;
        let run_end = run_start.saturating_add(run_bytes);
        if stream_offset >= run_end {
            run_start = run_end;
            continue;
        }
        let offset_in_run = stream_offset.saturating_sub(run_start);
        let to_read = run_bytes
            .saturating_sub(offset_in_run)
            .min((output.len() - written) as u64) as usize;
        let disk_offset = volume_offset
            .checked_add((*lcn as u64).saturating_mul(cluster_size))
            .and_then(|base| base.checked_add(offset_in_run))
            .ok_or_else(|| invalid_data("MFT disk offset overflow"))?;
        reader.seek(SeekFrom::Start(disk_offset))?;
        reader.read_exact(&mut output[written..written + to_read])?;
        written += to_read;
        if written == output.len() {
            return Ok(());
        }
        stream_offset = run_end;
        run_start = run_end;
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::UnexpectedEof,
        format!(
            "MFT stream ended before read completed (read {written} of {} bytes)",
            output.len()
        ),
    ))
}

fn checked_run_bytes(lcn: i64, clusters: u64, cluster_size: u64) -> std::io::Result<u64> {
    if lcn < 0 {
        return Err(invalid_data(format!("negative MFT LCN {lcn}")));
    }
    clusters.checked_mul(cluster_size).ok_or_else(|| {
        invalid_data(format!(
            "MFT run overflow: {clusters} clusters x {cluster_size} bytes"
        ))
    })
}

fn invalid_data(message: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into())
}

fn read_sized_le(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .enumerate()
        .take(8)
        .fold(0, |value, (index, byte)| {
            value | (*byte as u64) << (index * 8)
        })
}

fn read_sized_le_signed(bytes: &[u8]) -> i64 {
    let length = bytes.len().min(8);
    if length == 0 {
        return 0;
    }
    let mut value = read_sized_le(&bytes[..length]);
    if bytes[length - 1] & 0x80 != 0 {
        for index in length..8 {
            value |= 0xFFu64 << (index * 8);
        }
    }
    value as i64
}
