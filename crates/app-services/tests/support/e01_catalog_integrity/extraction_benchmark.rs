use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use app_services::{
    bitlocker_runtime::BitLockerUnlockRegistry,
    file_service::{self, FileExtractionProgressPhase, FileExtractionProgressUpdate},
};
use domain::DataSourceId;
use rusqlite::Connection;
use tempfile::TempDir;

const ENABLE_ENV: &str = "FORENSICS_E01_EXTRACTION_BENCHMARK_ONLY";
const MIN_BYTES: u64 = 128 * 1024 * 1024;
const MAX_BYTES: u64 = 512 * 1024 * 1024;

pub(crate) struct Context<'a> {
    pub(crate) case_conn: &'a Connection,
    pub(crate) case_root: &'a Path,
    pub(crate) case_id: &'a domain::CaseId,
    pub(crate) data_source_id: &'a DataSourceId,
    pub(crate) source_conn: &'a Connection,
    pub(crate) bitlocker_runtime: &'a Arc<BitLockerUnlockRegistry>,
    pub(crate) env_name: &'a str,
}

#[derive(Debug)]
struct Candidate {
    local_id: String,
    path: String,
    size: u64,
    partition_index: u32,
    bitlocker: bool,
}

pub(crate) fn is_enabled() -> bool {
    std::env::var(ENABLE_ENV)
        .ok()
        .is_some_and(|value| matches!(value.trim(), "1" | "true" | "TRUE" | "yes" | "YES"))
}

pub(crate) fn run(context: Context<'_>) -> persistence_sqlite::DbResult<()> {
    let candidates = candidates(context.source_conn, context.data_source_id)?;
    let export_root = TempDir::new()?;
    let mut failures = Vec::new();

    for (ordinal, candidate) in candidates.into_iter().enumerate() {
        let global_id = app_services::source_db::GlobalFileId::new(
            context.data_source_id.clone(),
            domain::FileEntryId(candidate.local_id.clone()),
        )
        .encode()
        .0;
        if preview_candidate(&context, &global_id).is_err() {
            failures.push(format!("{}: preview failed", candidate.path));
            continue;
        }

        let destination = export_root.path().join(format!("candidate-{ordinal}.bin"));
        match measure_candidate(&context, &global_id, &candidate, &destination) {
            Ok(()) => return Ok(()),
            Err(error) => failures.push(format!("{}: {error}", candidate.path)),
        }
    }

    Err(persistence_sqlite::DbError::System(format!(
        "{}: no 128-512 MiB regular file completed the extraction benchmark; first failures: {}",
        context.env_name,
        failures.into_iter().take(5).collect::<Vec<_>>().join(" | ")
    )))
}

fn preview_candidate(context: &Context<'_>, global_id: &str) -> Result<(), String> {
    let request = transport::dto::ViewerRangeRequestDto {
        handle_id: format!("file:{global_id}"),
        offset: 0,
        length: 4_096,
    };
    file_service::read_file_range_for_source_case_with_bitlocker(
        context.bitlocker_runtime,
        context.case_conn,
        context.case_root,
        context.case_id,
        &request,
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

fn candidates(
    source_conn: &Connection,
    data_source_id: &DataSourceId,
) -> persistence_sqlite::DbResult<Vec<Candidate>> {
    let mut statement = source_conn.prepare(
        "SELECT f.id, f.path, f.size, f.partition_index,
                CASE WHEN EXISTS (
                    SELECT 1 FROM data_source_partitions p
                    WHERE p.data_source_id = f.data_source_id
                      AND p.partition_index = f.partition_index
                      AND lower(p.kind_label) = 'bitlocker'
                ) THEN 1 ELSE 0 END AS is_bitlocker
         FROM file_entries f
         WHERE f.data_source_id = ?1
           AND f.entry_type = 'file' COLLATE NOCASE
           AND f.encrypted = 0
           AND f.size BETWEEN ?2 AND ?3
         ORDER BY is_bitlocker DESC, f.size DESC
         LIMIT 64",
    )?;
    let candidates = statement
        .query_map(
            rusqlite::params![&data_source_id.0, MIN_BYTES, MAX_BYTES],
            |row| {
                Ok(Candidate {
                    local_id: row.get(0)?,
                    path: row.get(1)?,
                    size: row.get(2)?,
                    partition_index: row.get(3)?,
                    bitlocker: row.get::<_, i64>(4)? != 0,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(candidates)
}

fn measure_candidate(
    context: &Context<'_>,
    global_id: &str,
    candidate: &Candidate,
    destination: &Path,
) -> Result<(), String> {
    let started = Instant::now();
    let mut copying_started = None;
    let mut finalizing_started = None;
    let mut progress_events = 0_u64;
    let mut last_bytes_written = 0_u64;
    let mut progress = |update: FileExtractionProgressUpdate| {
        progress_events = progress_events.saturating_add(1);
        last_bytes_written = update.bytes_written;
        match update.phase {
            FileExtractionProgressPhase::Copying => {
                copying_started.get_or_insert_with(Instant::now);
            }
            FileExtractionProgressPhase::Finalizing => {
                finalizing_started.get_or_insert_with(Instant::now);
            }
        }
    };
    let result = file_service::extract_file_to_destination_for_case_with_bitlocker_and_progress(
        context.bitlocker_runtime,
        file_service::CaseFileExtractionRequest {
            case_conn: context.case_conn,
            case_root: context.case_root,
            case_id: context.case_id,
            file_id: global_id,
            destination_path: destination,
            overwrite: false,
        },
        &mut progress,
    )
    .map_err(|error| error.to_string())?;
    let completed = Instant::now();
    let copying_started = copying_started.ok_or("copying progress phase was not emitted")?;
    let finalizing_started =
        finalizing_started.ok_or("finalizing progress phase was not emitted")?;
    let prepare_elapsed = copying_started.duration_since(started);
    let copy_elapsed = finalizing_started.duration_since(copying_started);
    let finalizing_elapsed = completed.duration_since(finalizing_started);
    let total_elapsed = completed.duration_since(started);

    verify_output(
        destination,
        candidate.size,
        result.bytes_written,
        last_bytes_written,
    )?;
    let record = serde_json::json!({
        "sample": context.env_name,
        "fileId": global_id,
        "path": candidate.path,
        "partitionIndex": candidate.partition_index,
        "bitlocker": candidate.bitlocker,
        "bytes": candidate.size,
        "prepareMs": duration_millis(prepare_elapsed),
        "copyMs": duration_millis(copy_elapsed),
        "finalizingMs": duration_millis(finalizing_elapsed),
        "totalMs": duration_millis(total_elapsed),
        "copyMiBPerSecond": mib_per_second(candidate.size, copy_elapsed),
        "totalMiBPerSecond": mib_per_second(candidate.size, total_elapsed),
        "progressEvents": progress_events,
        "sha256": result.sha256,
    });
    println!("FILE_EXTRACTION_BENCHMARK_JSON={record}");
    Ok(())
}

fn verify_output(
    destination: &Path,
    expected: u64,
    result_bytes: u64,
    progress_bytes: u64,
) -> Result<(), String> {
    if result_bytes != expected || progress_bytes != expected {
        return Err(format!(
            "byte count mismatch: catalog={expected} result={result_bytes} progress={progress_bytes}"
        ));
    }
    let destination_size = std::fs::metadata(destination)
        .map_err(|error| error.to_string())?
        .len();
    if destination_size != expected {
        return Err(format!(
            "destination size mismatch: catalog={expected} destination={destination_size}"
        ));
    }
    Ok(())
}

fn duration_millis(duration: Duration) -> u64 {
    duration.as_millis().min(u64::MAX as u128) as u64
}

fn mib_per_second(bytes: u64, duration: Duration) -> f64 {
    if duration.is_zero() {
        return 0.0;
    }
    bytes as f64 / (1024.0 * 1024.0) / duration.as_secs_f64()
}
