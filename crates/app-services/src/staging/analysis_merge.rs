use super::db_paths::analysis_staging_db_path;
use super::error::StagingError;
use super::rows_per_sec;
use super::schema_bootstrap::{get_worker_meta, open_analysis_staging, set_worker_meta};
use persistence_sqlite::repositories::staging_repo::StagingRepo;
use rusqlite::Connection;
use std::path::Path;
use std::time::Instant;

pub(super) const INDEX_DOC_MERGE_PAGE_SIZE: i64 = 50;

pub fn merge_analysis_staging_to_main(
    main_conn: &Connection,
    case_root: &Path,
    data_source_id: &str,
    worker_ids: &[usize],
    case_id: &str,
    index_dir: &Path,
    progress_cb: Option<&dyn Fn(usize, usize)>,
) -> Result<AnalysisMergeStats, StagingError> {
    let mut stats = AnalysisMergeStats::default();
    let total = worker_ids.len().max(1);

    for (position, worker_id) in worker_ids.iter().enumerate() {
        let db_path = analysis_staging_db_path(case_root, data_source_id, *worker_id);
        if !db_path.exists() {
            if let Some(cb) = progress_cb {
                cb(position + 1, total);
            }
            continue;
        }

        let worker_conn =
            open_analysis_staging(case_root, data_source_id, *worker_id).map_err(|e| {
                StagingError::Other(format!("Open analysis staging DB {}: {}", worker_id, e))
            })?;
        if get_worker_meta(&worker_conn, "merged")
            .map_err(|e| {
                StagingError::Other(format!("Read analysis merge state {}: {}", worker_id, e))
            })?
            .as_deref()
            == Some("true")
        {
            if let Some(cb) = progress_cb {
                cb(position + 1, total);
            }
            continue;
        }
        drop(worker_conn);

        let worker_merge_started = Instant::now();
        let worker_conn =
            open_analysis_staging(case_root, data_source_id, *worker_id).map_err(|e| {
                StagingError::Other(format!("Open analysis staging DB {}: {}", worker_id, e))
            })?;
        let worker_stats =
            merge_one_analysis_worker(main_conn, &worker_conn, case_id, data_source_id)?;
        tracing::info!(
            "Analysis DB merge profile: worker={} artifacts={} timeline={} elapsedMs={} rowsPerSec={}",
            worker_id,
            worker_stats.artifact_count,
            worker_stats.timeline_count,
            worker_merge_started.elapsed().as_millis(),
            rows_per_sec(
                worker_stats.artifact_count + worker_stats.timeline_count,
                worker_merge_started.elapsed()
            )
        );
        stats.artifact_count += worker_stats.artifact_count;
        stats.timeline_count += worker_stats.timeline_count;

        let index_merge_started = Instant::now();
        let indexed = merge_one_analysis_index_docs(&worker_conn, index_dir)?;
        tracing::info!(
            "Analysis index merge profile: worker={} indexed={} elapsedMs={} rowsPerSec={}",
            worker_id,
            indexed,
            index_merge_started.elapsed().as_millis(),
            rows_per_sec(indexed, index_merge_started.elapsed())
        );
        stats.indexed_count += indexed;

        set_worker_meta(&worker_conn, "merged", "true").map_err(|e| {
            StagingError::Other(format!(
                "Mark analysis staging DB {} merged: {}",
                worker_id, e
            ))
        })?;

        if let Some(cb) = progress_cb {
            cb(position + 1, total);
        }
    }

    Ok(stats)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct AnalysisMergeStats {
    pub artifact_count: u64,
    pub timeline_count: u64,
    pub indexed_count: u64,
}

fn merge_one_analysis_worker(
    main_conn: &Connection,
    staging_conn: &Connection,
    case_id: &str,
    data_source_id: &str,
) -> Result<AnalysisMergeStats, StagingError> {
    let (artifact_count, timeline_count) = StagingRepo::merge_analysis_staging_to_main(
        main_conn,
        staging_conn,
        case_id,
        data_source_id,
    )
    .map_err(|e| StagingError::Other(format!("Merge analysis staging: {}", e)))?;

    Ok(AnalysisMergeStats {
        artifact_count,
        timeline_count,
        indexed_count: 0,
    })
}

pub(super) fn merge_one_analysis_index_docs(
    staging_conn: &Connection,
    index_dir: &Path,
) -> Result<u64, StagingError> {
    let index = match search::SearchIndex::open(index_dir) {
        Ok(index) => index,
        Err(_) => search::SearchIndex::create(index_dir)
            .map_err(|e| StagingError::Other(e.to_string()))?,
    };
    let mut indexed_total = 0u64;
    let mut offset = 0i64;
    loop {
        let rows = StagingRepo::read_analysis_index_docs_page(
            staging_conn,
            INDEX_DOC_MERGE_PAGE_SIZE,
            offset,
        )
        .map_err(|e| StagingError::Other(e.to_string()))?;

        if rows.is_empty() {
            break;
        }

        let mut texts = Vec::with_capacity(rows.len());
        let mut paths = Vec::with_capacity(rows.len());
        for (file_id, path, text, language) in &rows {
            texts.push(search::ExtractedText {
                file_id: file_id.clone(),
                content: text.clone(),
                encoding: language.clone(),
                extractable: true,
                byte_count: 0,
            });
            paths.push((file_id.clone(), path.clone()));
        }

        indexed_total += index
            .index_documents(&texts, &paths)
            .map_err(|e| StagingError::Other(e.to_string()))?;

        if rows.len() < INDEX_DOC_MERGE_PAGE_SIZE as usize {
            break;
        }
        offset += INDEX_DOC_MERGE_PAGE_SIZE;
    }
    Ok(indexed_total)
}
