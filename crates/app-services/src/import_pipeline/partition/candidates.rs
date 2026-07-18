use std::collections::HashMap;

use evidence_core::{EvidenceReader, FileSystemReader};

use crate::datasource_service::{self, ImageFilesystemKind, PartitionStatus};
use crate::file_service;
use crate::import_pipeline::emit::ImportEventSink;

use super::status::{
    format_partition_progress_detail, format_partition_record_root_name,
    format_partition_root_name, partition_status_label,
};
use super::work::open_candidate_filesystem;

type ProgressCallback<'a> = dyn FnMut(u32, &str) -> Result<(), String> + 'a;

pub fn enumerate_partition_with_fs(
    conn: &rusqlite::Connection,
    data_source_id: &domain::DataSourceId,
    fs: &dyn FileSystemReader,
    root_name: &str,
    placeholder_roots: &HashMap<usize, domain::FileEntryId>,
    candidate: &datasource_service::ImageFilesystemCandidate,
    progress_cb: Option<&dyn Fn(u32)>,
) -> persistence_sqlite::DbResult<file_service::EnumerationStats> {
    if let Some(placeholder_id) = candidate
        .partition_index
        .and_then(|index| placeholder_roots.get(&index))
    {
        return file_service::replace_placeholder_root_with_real(
            conn,
            placeholder_id,
            fs,
            Some(root_name),
            progress_cb,
        );
    }
    file_service::enumerate_filesystem_with_root_name(
        conn,
        data_source_id,
        fs,
        Some(root_name),
        progress_cb,
    )
}

pub fn enumerate_image_data_source<R>(
    conn: &rusqlite::Connection,
    data_source_id: &domain::DataSourceId,
    mut reader: R,
    mut progress: impl FnMut(u32, &str) -> Result<(), String>,
    event_sink: Option<&dyn ImportEventSink>,
    job_id: Option<&domain::JobId>,
) -> persistence_sqlite::DbResult<file_service::EnumerationStats>
where
    R: EvidenceReader + std::io::Read + std::io::Seek + 'static,
{
    let mut probe =
        datasource_service::detect_image_filesystem(&mut reader).map_err(system_error)?;
    let source_path = reader.info().path.clone();
    let source_kind = source_kind(reader.info().kind.as_str());
    datasource_service::expand_lvm_pool_candidates(&mut probe, &source_path, &source_kind);
    if probe.candidates.is_empty() {
        return Ok(empty_stats(probe.warnings));
    }

    file_service::store_data_source_partitions(conn, data_source_id, &probe.partitions)
        .map_err(system_error)?;
    let placeholders = seed_partition_roots(
        conn,
        data_source_id,
        &probe,
        &mut progress,
        event_sink,
        job_id,
    )?;
    let mut context = CandidateEnumerationContext {
        conn,
        data_source_id,
        source_path: &source_path,
        source_kind: &source_kind,
        placeholders: &placeholders,
        progress: &mut progress,
        event_sink,
        job_id,
    };
    let mut total = empty_stats(probe.warnings);
    let candidate_count = probe.candidates.len().max(1);
    for (ordinal, candidate) in probe.candidates.iter().enumerate() {
        if let Some(stats) = context.enumerate_candidate(ordinal, candidate_count, candidate)? {
            add_stats(&mut total, stats);
        }
    }
    if !total.warnings.is_empty() {
        (context.progress)(
            60,
            &format!("Partition warnings: {}", total.warnings.join(" | ")),
        )
        .map_err(system_error)?;
    }
    Ok(total)
}

struct CandidateEnumerationContext<'a> {
    conn: &'a rusqlite::Connection,
    data_source_id: &'a domain::DataSourceId,
    source_path: &'a std::path::Path,
    source_kind: &'a domain::DataSourceKind,
    placeholders: &'a HashMap<usize, domain::FileEntryId>,
    progress: &'a mut ProgressCallback<'a>,
    event_sink: Option<&'a dyn ImportEventSink>,
    job_id: Option<&'a domain::JobId>,
}

impl CandidateEnumerationContext<'_> {
    fn enumerate_candidate(
        &mut self,
        ordinal: usize,
        total: usize,
        candidate: &datasource_service::ImageFilesystemCandidate,
    ) -> persistence_sqlite::DbResult<Option<file_service::EnumerationStats>> {
        let root_name = format_partition_root_name(candidate);
        self.report_candidate_start(ordinal, total, candidate, &root_name)?;
        if candidate.kind == ImageFilesystemKind::BitLocker {
            self.persist_partition_progress(&root_name, ordinal as u32 + 1, total as u32, 100);
            return Ok(None);
        }
        if candidate.kind == ImageFilesystemKind::LvmPool {
            return Ok(None);
        }
        let fs = open_candidate_filesystem(self.source_path, self.source_kind, candidate)
            .map_err(|error| {
                system_error(format!("open reader for partition '{root_name}': {error}"))
            })?
            .ok_or_else(|| system_error("candidate filesystem is unavailable"))?;
        let progress = |percent: u32| {
            self.report_partition_percent(ordinal, total, &root_name, percent);
        };
        let stats = enumerate_partition_with_fs(
            self.conn,
            self.data_source_id,
            fs.as_ref(),
            &root_name,
            self.placeholders,
            candidate,
            self.job_id.map(|_| &progress as &dyn Fn(u32)),
        )?;
        self.report_candidate_complete(ordinal, total, &root_name)?;
        Ok(Some(stats))
    }

    fn report_candidate_start(
        &mut self,
        ordinal: usize,
        total: usize,
        candidate: &datasource_service::ImageFilesystemCandidate,
        root_name: &str,
    ) -> persistence_sqlite::DbResult<()> {
        let detail = candidate_start_detail(candidate.kind, root_name);
        let progress_detail =
            format_partition_progress_detail(ordinal as u32, total as u32, 5, root_name, &detail);
        (self.progress)(25 + (ordinal as u32 * 35 / total as u32), &progress_detail)
            .map_err(system_error)?;
        self.persist_partition_progress(root_name, ordinal as u32, total as u32, 0);
        Ok(())
    }

    fn report_candidate_complete(
        &mut self,
        ordinal: usize,
        total: usize,
        root_name: &str,
    ) -> persistence_sqlite::DbResult<()> {
        self.persist_partition_progress(root_name, ordinal as u32 + 1, total as u32, 100);
        let detail = format_partition_progress_detail(
            ordinal as u32,
            total as u32,
            100,
            root_name,
            &format!("Imported {root_name}"),
        );
        let progress = (25 + (ordinal as u32 * 35 / total as u32))
            .saturating_add((35 / total as u32).max(1))
            .min(68);
        (self.progress)(progress, &detail).map_err(system_error)
    }

    fn report_partition_percent(
        &self,
        ordinal: usize,
        total: usize,
        root_name: &str,
        percent: u32,
    ) {
        let Some(job_id) = self.job_id else {
            return;
        };
        let overall = 25 + ((ordinal as u32 * 35) + (percent * 35 / 100)) / total.max(1) as u32;
        let repo = persistence_sqlite::repositories::job_repo::JobRepo::new(self.conn);
        let _ = repo.update_progress(job_id, overall.min(65), &format!("{root_name} {percent}%"));
        crate::import_pipeline::emit::emit_partition_progress(
            self.event_sink,
            &job_id.0,
            root_name,
            ordinal as u32,
            total as u32,
            percent,
        );
    }

    fn persist_partition_progress(
        &self,
        root_name: &str,
        completed: u32,
        total: u32,
        percent: u32,
    ) {
        let Some(job_id) = self.job_id else {
            return;
        };
        let repo = persistence_sqlite::repositories::job_repo::JobRepo::new(self.conn);
        if let Err(error) =
            repo.update_partition_progress(job_id, root_name, completed, total, percent)
        {
            tracing::debug!("Failed to update partition progress: {error}");
        }
        crate::import_pipeline::emit::emit_partition_progress(
            self.event_sink,
            &job_id.0,
            root_name,
            completed,
            total,
            percent,
        );
    }
}

fn seed_partition_roots(
    conn: &rusqlite::Connection,
    data_source_id: &domain::DataSourceId,
    probe: &datasource_service::ImageFilesystemProbe,
    progress: &mut ProgressCallback<'_>,
    event_sink: Option<&dyn ImportEventSink>,
    job_id: Option<&domain::JobId>,
) -> persistence_sqlite::DbResult<HashMap<usize, domain::FileEntryId>> {
    let mut roots = HashMap::new();
    let total = probe.partitions.len().max(1);
    for (ordinal, partition) in probe.partitions.iter().enumerate() {
        let root_name = format_partition_record_root_name(partition);
        let detail = partition_detection_detail(partition.status, &root_name);
        let progress_detail = if partition.status == PartitionStatus::Supported {
            format_partition_progress_detail(ordinal as u32, total as u32, 0, &root_name, &detail)
        } else {
            detail
        };
        progress(12 + (ordinal as u32 * 8 / total as u32), &progress_detail)
            .map_err(system_error)?;
        report_seed_progress(conn, event_sink, job_id, &root_name, ordinal, total);
        if partition.status == PartitionStatus::Expanded {
            continue;
        }
        let root = file_service::insert_partition_placeholder_root(
            conn,
            data_source_id,
            partition.index,
            &root_name,
            partition_status_label(partition.status),
        )?;
        roots.insert(partition.index, root);
    }
    Ok(roots)
}

fn report_seed_progress(
    conn: &rusqlite::Connection,
    event_sink: Option<&dyn ImportEventSink>,
    job_id: Option<&domain::JobId>,
    root_name: &str,
    ordinal: usize,
    total: usize,
) {
    let Some(job_id) = job_id else {
        return;
    };
    let repo = persistence_sqlite::repositories::job_repo::JobRepo::new(conn);
    if let Err(error) =
        repo.update_partition_progress(job_id, root_name, ordinal as u32, total as u32, 0)
    {
        tracing::debug!("Failed to update partition progress: {error}");
    }
    crate::import_pipeline::emit::emit_partition_progress(
        event_sink,
        &job_id.0,
        root_name,
        ordinal as u32,
        total as u32,
        0,
    );
}

fn partition_detection_detail(status: PartitionStatus, root_name: &str) -> String {
    match status {
        PartitionStatus::Supported => format!("Detected {root_name}; queued for import"),
        PartitionStatus::Expanded => {
            format!("Detected {root_name}; expanded into logical volumes")
        }
        PartitionStatus::EncryptedBitLocker => format!("Detected locked {root_name}"),
        PartitionStatus::Unsupported => format!("Detected unsupported {root_name}"),
    }
}

fn candidate_start_detail(kind: ImageFilesystemKind, root_name: &str) -> String {
    match kind {
        ImageFilesystemKind::Ntfs
        | ImageFilesystemKind::Fat
        | ImageFilesystemKind::Ext4
        | ImageFilesystemKind::Xfs
        | ImageFilesystemKind::Btrfs => format!("Enumerating {root_name}"),
        ImageFilesystemKind::BitLocker => format!("Skipping locked {root_name}"),
        ImageFilesystemKind::LvmPool => {
            tracing::warn!(
                "LvmPool reached enumeration phase unexpectedly for '{root_name}'; skipping"
            );
            format!("Discovering LVM logical volumes in {root_name}")
        }
    }
}

fn source_kind(kind: &str) -> domain::DataSourceKind {
    if kind.eq_ignore_ascii_case("e01") {
        domain::DataSourceKind::E01
    } else {
        domain::DataSourceKind::Raw
    }
}

fn empty_stats(warnings: Vec<String>) -> file_service::EnumerationStats {
    file_service::EnumerationStats {
        file_count: 0,
        dir_count: 0,
        total_size: 0,
        warnings,
        diagnostics: Vec::new(),
    }
}

fn add_stats(
    total: &mut file_service::EnumerationStats,
    partition: file_service::EnumerationStats,
) {
    total.file_count = total.file_count.saturating_add(partition.file_count);
    total.dir_count = total.dir_count.saturating_add(partition.dir_count);
    total.total_size = total.total_size.saturating_add(partition.total_size);
    total.warnings.extend(partition.warnings);
    total.diagnostics.extend(partition.diagnostics);
}

fn system_error(error: impl ToString) -> persistence_sqlite::DbError {
    persistence_sqlite::DbError::System(error.to_string())
}
