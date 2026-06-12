use super::db_paths::analysis_staging_db_path;
use super::rows_per_sec;
use super::schema_bootstrap::{get_worker_meta, open_analysis_staging, set_worker_meta};
use rusqlite::{params, Connection};
use std::path::Path;
use std::time::Instant;

pub(super) const INDEX_DOC_MERGE_PAGE_SIZE: i64 = 50;

/// Merge analysis worker staging DBs into the main DB and search index.
pub fn merge_analysis_staging_to_main(
    main_conn: &Connection,
    case_root: &Path,
    data_source_id: &str,
    worker_ids: &[usize],
    case_id: &str,
    index_dir: &Path,
    progress_cb: Option<&dyn Fn(usize, usize)>,
) -> Result<AnalysisMergeStats, String> {
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

        let worker_conn = open_analysis_staging(case_root, data_source_id, *worker_id)
            .map_err(|e| format!("Open analysis staging DB {}: {}", worker_id, e))?;
        if get_worker_meta(&worker_conn, "merged")
            .map_err(|e| format!("Read analysis merge state {}: {}", worker_id, e))?
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
        let worker_stats =
            merge_one_analysis_worker(main_conn, &db_path, *worker_id, case_id, data_source_id)?;
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
        let indexed = merge_one_analysis_index_docs(&db_path, index_dir, *worker_id)?;
        tracing::info!(
            "Analysis index merge profile: worker={} indexed={} elapsedMs={} rowsPerSec={}",
            worker_id,
            indexed,
            index_merge_started.elapsed().as_millis(),
            rows_per_sec(indexed, index_merge_started.elapsed())
        );
        stats.indexed_count += indexed;

        let worker_conn = open_analysis_staging(case_root, data_source_id, *worker_id)
            .map_err(|e| format!("Reopen analysis staging DB {}: {}", worker_id, e))?;
        set_worker_meta(&worker_conn, "merged", "true")
            .map_err(|e| format!("Mark analysis staging DB {} merged: {}", worker_id, e))?;

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
    db_path: &Path,
    worker_id: usize,
    case_id: &str,
    data_source_id: &str,
) -> Result<AnalysisMergeStats, String> {
    let db_path_str = db_path.to_string_lossy().replace('\'', "''");
    let attach_sql = format!("ATTACH DATABASE '{}' AS analysis_stage", db_path_str);
    let result = (|| {
        main_conn
            .execute_batch(&attach_sql)
            .map_err(|e| format!("Attach analysis DB {}: {}", worker_id, e))?;
        main_conn
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| format!("Begin analysis merge transaction {}: {}", worker_id, e))?;

        let artifact_count = main_conn
            .execute(
                "INSERT INTO main.artifacts
                 (id, case_id, data_source_id, artifact_type, source_object_id, extractor_id, extractor_version, confidence, source_attribution, title, summary, attrs, created_at)
                  SELECT id, ?1, ?2, artifact_type, file_id, extractor_id, extractor_version, confidence, source_attribution, display_name, summary, data_json, created_at
                  FROM analysis_stage.artifact_rows",
                params![case_id, data_source_id],
            )
            .map_err(|e| format!("Merge analysis artifacts {}: {}", worker_id, e))?;

        let timeline_count = main_conn
            .execute(
                "INSERT INTO main.timeline_events
                 (id, case_id, source_object_id, event_type, ts, title, description, parser_id, parser_version, confidence, source_attribution, attrs)
                  SELECT id, ?1, file_id, event_type, timestamp, title, description, parser_id, parser_version, confidence, source_attribution, data_json
                  FROM analysis_stage.timeline_rows",
                params![case_id],
            )
            .map_err(|e| format!("Merge analysis timeline {}: {}", worker_id, e))?;

        main_conn
            .execute_batch("COMMIT")
            .map_err(|e| format!("Commit analysis merge transaction {}: {}", worker_id, e))?;
        main_conn
            .execute_batch("DETACH DATABASE analysis_stage")
            .map_err(|e| format!("Detach analysis DB {}: {}", worker_id, e))?;

        Ok(AnalysisMergeStats {
            artifact_count: artifact_count as u64,
            timeline_count: timeline_count as u64,
            indexed_count: 0,
        })
    })();

    if result.is_err() {
        let _ = main_conn.execute_batch("ROLLBACK");
        let _ = main_conn.execute_batch("DETACH DATABASE analysis_stage");
    }

    result
}

pub(super) fn merge_one_analysis_index_docs(
    db_path: &Path,
    index_dir: &Path,
    worker_id: usize,
) -> Result<u64, String> {
    let conn = Connection::open(db_path)
        .map_err(|e| format!("Open analysis index docs {}: {}", worker_id, e))?;
    let index = match search::SearchIndex::open(index_dir) {
        Ok(index) => index,
        Err(_) => search::SearchIndex::create(index_dir).map_err(|e| e.to_string())?,
    };
    let mut indexed_total = 0u64;
    let mut offset = 0i64;
    loop {
        let mut stmt = conn
            .prepare(
                "SELECT file_id, path, text, language
                 FROM index_docs
                 WHERE text <> ''
                 ORDER BY file_id
                 LIMIT ?1 OFFSET ?2",
            )
            .map_err(|e| format!("Prepare index docs {}: {}", worker_id, e))?;
        let rows = stmt
            .query_map(params![INDEX_DOC_MERGE_PAGE_SIZE, offset], |row| {
                let file_id: String = row.get(0)?;
                let path: String = row.get(1)?;
                let text: String = row.get(2)?;
                let language: String = row.get(3)?;
                Ok((file_id, path, text, language))
            })
            .map_err(|e| format!("Read index docs {}: {}", worker_id, e))?;

        let mut texts = Vec::new();
        let mut paths = Vec::new();
        for row in rows {
            let (file_id, path, text, language) =
                row.map_err(|e| format!("Map index docs {}: {}", worker_id, e))?;
            texts.push(search::ExtractedText {
                file_id: file_id.clone(),
                content: text,
                encoding: language,
                extractable: true,
                byte_count: 0,
            });
            paths.push((file_id, path));
        }
        if texts.is_empty() {
            break;
        }

        indexed_total += index
            .index_documents(&texts, &paths)
            .map_err(|e| e.to_string())?;
        if texts.len() < INDEX_DOC_MERGE_PAGE_SIZE as usize {
            break;
        }
        offset += INDEX_DOC_MERGE_PAGE_SIZE;
    }
    Ok(indexed_total)
}
