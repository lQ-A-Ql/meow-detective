use super::error::ParallelEnumError;
use super::ntfs;

#[derive(Debug, Clone, Copy)]
pub(crate) struct NtfsEnumerationStats {
    pub(crate) file_count: u64,
    pub(crate) dir_count: u64,
    pub(crate) total_size: u64,
    pub(crate) directory_index_failures: u64,
}

pub(crate) fn enumerate_ntfs_reader_to_staging(
    conn: &rusqlite::Connection,
    reader: Box<dyn evidence_core::EvidenceReader>,
    data_source_id: &str,
    partition_index: usize,
    volume_offset: u64,
    cancel_token: &std::sync::atomic::AtomicBool,
) -> Result<NtfsEnumerationStats, ParallelEnumError> {
    let stats = ntfs::enumerate_ntfs_reader_to_staging(
        conn,
        reader,
        data_source_id,
        partition_index,
        volume_offset,
        cancel_token,
        None,
    )?;
    Ok(NtfsEnumerationStats {
        file_count: stats.file_count,
        dir_count: stats.dir_count,
        total_size: stats.total_size,
        directory_index_failures: stats.directory_index_failures,
    })
}
