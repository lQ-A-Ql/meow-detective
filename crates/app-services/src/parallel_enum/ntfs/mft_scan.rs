use super::super::error::ParallelEnumError;
use super::super::partition_work::PartitionWork;
use crate::import_pipeline::profile::{Counters, PhaseRecorder};
use evidence_core::EvidenceReader;
use fs_ntfs::mft_scanner::{MftRecord, MftScanner};
use std::io::{Read, Seek, SeekFrom};
use transport::dto::import_benchmark::ImportBenchmarkPhaseDto as Phase;

pub(super) const MFT_READ_CHUNK_RECORDS: u64 = 10_000;

pub(in crate::parallel_enum) struct MftScan {
    reader: Option<Box<dyn EvidenceReader>>,
    params: NtfsMftParams,
    scanner: MftScanner,
    total_records: u64,
    buffer: Vec<u8>,
    pub(super) chunk_records: u64,
    pub(super) resource_hold: Option<std::sync::Arc<dyn evidence_core::reader::ReaderResourceHold>>,
}

impl MftScan {
    pub(super) fn reopen(
        &self,
        factory: &dyn evidence_core::EvidenceReaderFactory,
    ) -> Result<Self, ParallelEnumError> {
        let params = self.params.clone();
        let scanner = MftScanner::new(
            params.volume_offset,
            params.mft_cluster,
            params.cluster_size,
            params.record_size,
            params.bytes_per_sector,
            params.mft_data_size,
        );
        Ok(Self {
            reader: Some(factory.open_reader()?),
            params,
            scanner,
            total_records: self.total_records,
            buffer: Vec::new(),
            chunk_records: self.chunk_records,
            resource_hold: self.resource_hold.clone(),
        })
    }

    pub(super) fn total_records(&self) -> u64 {
        self.total_records
    }

    pub(super) fn record_size(&self) -> u32 {
        self.scanner.record_size()
    }

    pub(in crate::parallel_enum) fn read_chunk(
        &mut self,
        start_record: u64,
        profile: &mut PhaseRecorder,
    ) -> Result<(Vec<MftRecord>, u64), ParallelEnumError> {
        let chunk_count = profile.measure(Phase::MftRead, || self.read_raw_chunk(start_record))?;
        profile.count(
            Phase::MftRead,
            Counters {
                scanned: chunk_count,
                bytes: self.buffer.len() as u64,
                ..Default::default()
            },
        );
        let records = profile.measure(Phase::MftParse, || {
            Ok::<_, ParallelEnumError>(self.scanner.parse_chunk(
                &self.buffer,
                start_record,
                chunk_count,
            ))
        })?;
        profile.count(
            Phase::MftParse,
            Counters {
                scanned: chunk_count,
                parsed: records.len() as u64,
                ..Default::default()
            },
        );
        Ok((records, chunk_count))
    }

    fn read_raw_chunk(&mut self, start_record: u64) -> Result<u64, ParallelEnumError> {
        let chunk_count = self.chunk_records.min(self.total_records - start_record);
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

        Ok(chunk_count)
    }

    pub(in crate::parallel_enum) fn release_buffer(&mut self) {
        self.buffer = Vec::new();
        self.resource_hold = None;
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
    profile: &mut PhaseRecorder,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<MftScan, ParallelEnumError> {
    let reader = profile.measure(Phase::MftReaderOpen, || match &partition.reader_factory {
        Some(factory) => factory.open_reader_with_cancel(cancel).map_err(|e| {
            if cancel.load(std::sync::atomic::Ordering::Acquire) {
                ParallelEnumError::Cancelled
            } else {
                ParallelEnumError::Io(e)
            }
        }),
        None => open_partition_evidence_reader(partition),
    })?;
    profile.count(
        Phase::MftReaderOpen,
        Counters {
            opens: 1,
            ..Default::default()
        },
    );
    prepare_mft_scan_from_reader(reader, partition.volume_offset, profile)
}

pub(in crate::parallel_enum) fn prepare_mft_scan_from_reader(
    mut reader: Box<dyn EvidenceReader>,
    volume_offset: u64,
    profile: &mut PhaseRecorder,
) -> Result<MftScan, ParallelEnumError> {
    let params = profile.measure(Phase::MftParams, || {
        read_ntfs_mft_parameters_at(&mut *reader, volume_offset)
    })?;
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
        chunk_records: MFT_READ_CHUNK_RECORDS,
        resource_hold: None,
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
    let record_size = fs_ntfs::mft_record_bytes(&boot)?;
    if record_size > 16 * 1024 * 1024 {
        return Err(ParallelEnumError::MftResourceLimit(
            "MFT parameter record exceeds 16 MiB",
        ));
    }
    let offset = mft_cluster
        .checked_mul(cluster_size)
        .and_then(|offset| volume_offset.checked_add(offset))
        .ok_or_else(|| invalid_data("MFT record offset overflow"))?;
    let mut record = read_mft_record_zero(reader, offset, record_size)?;
    let record = fs_ntfs::ValidatedRecord::new(&mut record, bytes_per_sector as usize)
        .map_err(|error| format!("Fix up MFT record 0: {error}"))?;
    let data = record
        .mft_data()
        .map_err(|error| format!("Parse MFT stream: {error}"))?;

    Ok(NtfsMftParams {
        volume_offset,
        mft_cluster,
        cluster_size,
        record_size,
        bytes_per_sector,
        mft_data_size: data.real_size,
        mft_data_runs: data.runs,
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

pub(in crate::parallel_enum) use super::reader_open::open_partition_evidence_reader;

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
        let run_end = run_start
            .checked_add(run_bytes)
            .ok_or_else(|| invalid_data("MFT stream range overflow"))?;
        if stream_offset >= run_end {
            run_start = run_end;
            continue;
        }
        let offset_in_run = stream_offset.saturating_sub(run_start);
        let to_read = run_bytes
            .saturating_sub(offset_in_run)
            .min((output.len() - written) as u64) as usize;
        let disk_offset = (*lcn as u64)
            .checked_mul(cluster_size)
            .and_then(|offset| volume_offset.checked_add(offset))
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
