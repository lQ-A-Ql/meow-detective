use evidence_core::{EvidenceReader, RawImageReader};
use image_e01::E01Reader;
use transport::CommandError;

use crate::{datasource_service, file_service, import_analysis, staging};

use crate::import_pipeline::context::ImportJobContext;
use crate::import_pipeline::partition::{
    format_partition_record_root_name, format_partition_root_name, partition_status_label,
};
use crate::import_pipeline::profile::{elapsed_ms, emit_phase_profile};

pub(super) fn seed_manifest_if_needed(
    ctx: &mut ImportJobContext<'_>,
    data_source: &domain::DataSource,
    manifest: &mut staging::StagingManifest,
    candidates: &mut Vec<datasource_service::ImageFilesystemCandidate>,
) -> Result<(), CommandError> {
    if !manifest.partitions.is_empty() {
        return Ok(());
    }
    let started = std::time::Instant::now();
    let mut probe = probe_image(ctx)?;
    datasource_service::expand_lvm_pool_candidates(
        &mut probe,
        &ctx.import_config.source_path,
        &ctx.import_config.kind,
    );
    report_probe(ctx, data_source, "probe", &probe, started.elapsed());
    persist_probe(ctx, data_source, &probe, manifest)?;
    *candidates = probe.candidates;
    Ok(())
}

pub(super) fn refresh_partition_statuses(
    case_root: &std::path::Path,
    data_source_id: &str,
    manifest: &mut staging::StagingManifest,
) -> Result<(), CommandError> {
    for partition in &mut manifest.partitions {
        refresh_partition_status(case_root, data_source_id, partition);
    }
    manifest
        .save(case_root)
        .map_err(CommandError::from_service_error)
}

pub(super) fn load_resume_candidates(
    ctx: &mut ImportJobContext<'_>,
    data_source: &domain::DataSource,
    candidates: &mut Vec<datasource_service::ImageFilesystemCandidate>,
) -> Result<(), CommandError> {
    if !candidates.is_empty() {
        return Ok(());
    }
    let started = std::time::Instant::now();
    let mut probe = probe_image(ctx)?;
    datasource_service::expand_lvm_pool_candidates(
        &mut probe,
        &ctx.import_config.source_path,
        &ctx.import_config.kind,
    );
    repair_resumed_partition_metadata(ctx, data_source, &probe)?;
    report_probe(ctx, data_source, "probe-resume", &probe, started.elapsed());
    *candidates = probe.candidates;
    Ok(())
}

fn probe_image(
    ctx: &ImportJobContext<'_>,
) -> Result<datasource_service::ImageFilesystemProbe, CommandError> {
    let path = &ctx.import_config.source_path;
    let mut reader: Box<dyn EvidenceReader> =
        if ctx.import_config.kind == domain::DataSourceKind::E01 {
            Box::new(E01Reader::open(path).map_err(CommandError::from_service_error)?)
        } else {
            Box::new(RawImageReader::open(path).map_err(CommandError::from_service_error)?)
        };
    datasource_service::detect_image_filesystem(&mut reader)
        .map_err(CommandError::from_service_error)
}

fn report_probe(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
    phase: &str,
    probe: &datasource_service::ImageFilesystemProbe,
    elapsed: std::time::Duration,
) {
    let detail = if phase == "probe" {
        format!(
            "Probe complete: phase={phase} elapsedMs={} partitions={} candidates={} rssMb={}",
            elapsed_ms(elapsed),
            probe.partitions.len(),
            probe.candidates.len(),
            import_analysis::current_rss_mb()
        )
    } else {
        format!(
            "Probe complete: phase={phase} elapsedMs={} candidates={} rssMb={}",
            elapsed_ms(elapsed),
            probe.candidates.len(),
            import_analysis::current_rss_mb()
        )
    };
    emit_phase_profile(
        ctx.event_sink(),
        ctx.job_id,
        ctx.case_id,
        Some(&data_source.id),
        if phase == "probe" { 28 } else { 30 },
        detail,
        ctx.cancel_requested(),
    );
}

fn persist_probe(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
    probe: &datasource_service::ImageFilesystemProbe,
    manifest: &mut staging::StagingManifest,
) -> Result<(), CommandError> {
    let source_conn = ctx.source_connection()?;
    file_service::store_data_source_partitions(source_conn, &data_source.id, &probe.partitions)
        .map_err(CommandError::from_service_error)?;
    seed_placeholder_roots(source_conn, data_source, probe)?;
    seed_manifest_entries(manifest, &probe.candidates);
    manifest
        .save(ctx.case_root)
        .map_err(CommandError::from_service_error)
}

fn seed_placeholder_roots(
    source_conn: &rusqlite::Connection,
    data_source: &domain::DataSource,
    probe: &datasource_service::ImageFilesystemProbe,
) -> Result<(), CommandError> {
    let index_map = datasource_service::assign_effective_partition_indices(&probe.candidates);
    let candidate_names = probe
        .candidates
        .iter()
        .enumerate()
        .map(|(ordinal, candidate)| {
            let index =
                datasource_service::effective_partition_index(candidate, ordinal, &index_map);
            (index, format_partition_root_name(candidate))
        })
        .collect::<std::collections::HashMap<_, _>>();

    for partition in &probe.partitions {
        if partition.status == datasource_service::PartitionStatus::Expanded {
            continue;
        }
        let root_name = candidate_names
            .get(&partition.index)
            .cloned()
            .unwrap_or_else(|| format_partition_record_root_name(partition));
        file_service::insert_partition_placeholder_root(
            source_conn,
            &data_source.id,
            partition.index,
            &root_name,
            partition_status_label(partition.status),
        )
        .map_err(CommandError::from_service_error)?;
    }
    Ok(())
}

fn seed_manifest_entries(
    manifest: &mut staging::StagingManifest,
    candidates: &[datasource_service::ImageFilesystemCandidate],
) {
    let index_map = datasource_service::assign_effective_partition_indices(candidates);
    manifest
        .partitions
        .extend(candidates.iter().enumerate().map(|(ordinal, candidate)| {
            let index =
                datasource_service::effective_partition_index(candidate, ordinal, &index_map);
            staging::PartitionEntry {
                index,
                name: format_partition_root_name(candidate),
                fs_kind: format!("{:?}", candidate.kind),
                staging_db: format!("enum_partition_{index}.db"),
                status: staging::PartitionStatus::Pending,
                file_count: 0,
                dir_count: 0,
                total_size: 0,
                last_path: None,
                completed_at: None,
                error: None,
            }
        }));
}

fn refresh_partition_status(
    case_root: &std::path::Path,
    data_source_id: &str,
    partition: &mut staging::PartitionEntry,
) {
    let path = staging::staging_db_path(case_root, data_source_id, partition.index);
    if !path.exists() {
        return;
    }
    let Ok(connection) =
        staging::open_partition_staging(case_root, data_source_id, partition.index)
    else {
        return;
    };
    let Ok(Some(status)) = staging::get_staging_meta(&connection, "status") else {
        return;
    };
    match status.as_str() {
        "done" => {
            partition.status = staging::PartitionStatus::Done;
            partition.file_count = staging::staging_db_row_count(&connection).unwrap_or(0);
        }
        "failed" => partition.status = staging::PartitionStatus::Failed,
        _ => {}
    }
}

fn repair_resumed_partition_metadata(
    ctx: &ImportJobContext<'_>,
    data_source: &domain::DataSource,
    probe: &datasource_service::ImageFilesystemProbe,
) -> Result<(), CommandError> {
    let source_conn = ctx.source_connection()?;
    file_service::store_data_source_partitions(source_conn, &data_source.id, &probe.partitions)
        .map_err(CommandError::from_service_error)?;
    for partition in &probe.partitions {
        if partition.status == datasource_service::PartitionStatus::Expanded {
            file_service::remove_partition_placeholder_root(
                source_conn,
                &data_source.id,
                partition.index,
            )
            .map_err(CommandError::from_service_error)?;
        }
    }
    Ok(())
}
