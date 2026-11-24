use super::progress::{TimelineProgressCallback, TimelineWork};
use super::{
    TimelineProjectionError, TimelineProjectionStats, TimelineWriteTimings, FILE_PROJECTION_KEY,
    GRAPH_PROJECTION_KEY,
};
use domain::DataSourcePlatform;
use persistence_source::fjall::{
    CatalogRecordStore, PersistMode, ScanDirection, SegmentLimits, SegmentedReplaceOutcome,
    TimelineRecord, TimelineRecordStore,
};
use persistence_sqlite::repositories::timeline_projection_repo::TimelineProjectionRepo;
use rusqlite::Connection;
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

pub(super) fn identity(
    conn: &Connection,
    platform: DataSourcePlatform,
    cancel: &AtomicBool,
    progress: Option<&TimelineProgressCallback<'_>>,
) -> Result<String, TimelineProjectionError> {
    use sha2::Digest;
    let store = persistence_source::detail::open_store(conn)?
        .ok_or_else(|| TimelineProjectionError::Other("source generation is missing".into()))?;
    let source = persistence_source::detail::source_id(conn)?
        .ok_or_else(|| TimelineProjectionError::Other("source identity is missing".into()))?;
    let catalog = CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = catalog.read_snapshot();
    let mut hasher = sha2::Sha256::new();
    hasher.update(platform.as_storage_str());
    let mut cursor = None;
    let total = catalog.count(&snapshot, Some(&source))?;
    let mut processed = 0;
    if let Some(progress) = progress {
        progress(TimelineWork::TimelineIdentity, 0, Some(total));
    }
    loop {
        ensure_active(cancel)?;
        let page = catalog.list_all(
            &snapshot,
            &source,
            cursor.as_deref(),
            ScanDirection::Forward,
            512,
        )?;
        processed += page.records.len() as u64;
        for record in page.records {
            ensure_active(cancel)?;
            let bytes = serde_json::to_vec(&record)
                .map_err(|error| TimelineProjectionError::Other(error.to_string()))?;
            hasher.update((bytes.len() as u64).to_be_bytes());
            hasher.update(bytes);
        }
        if let Some(progress) = progress {
            progress(TimelineWork::TimelineIdentity, processed, Some(total));
        }
        cursor = page.next_cursor;
        if cursor.is_none() {
            return Ok(format!(
                "file-activity-v2:{}",
                hex::encode(hasher.finalize())
            ));
        }
    }
}

pub(super) fn materialize(
    conn: &Connection,
    platform: DataSourcePlatform,
    cancel: &AtomicBool,
    identity: &str,
    policy: timeline::TimelineProjectionPolicy,
    progress: Option<&TimelineProgressCallback<'_>>,
) -> Result<TimelineProjectionStats, TimelineProjectionError> {
    let store = persistence_source::detail::open_store(conn)?
        .ok_or_else(|| TimelineProjectionError::Other("source generation is missing".into()))?;
    let source = persistence_source::detail::source_id(conn)?
        .ok_or_else(|| TimelineProjectionError::Other("source identity is missing".into()))?;
    let meta = TimelineProjectionRepo::new(conn);
    meta.ensure_meta_table()?;
    let started = Instant::now();
    let file_done = meta.is_done(FILE_PROJECTION_KEY, identity)?;
    let graph_done = meta.is_done(GRAPH_PROJECTION_KEY, identity)?;
    if file_done && graph_done {
        return Ok(TimelineProjectionStats {
            already_projected: true,
            graph_complete: true,
            ..Default::default()
        });
    }
    let estimate = super::workspace::estimate(cancel)?;
    estimate
        .require(policy)
        .map_err(|error| attach_identity(error, identity))?;
    tracing::debug!(
        required_bytes = estimate.required_bytes,
        configured_bytes = policy.reservation_bytes()?,
        engine_retained_bytes = estimate.engine_retained_bytes,
        "Timeline projection workspace planned (constant segment plan)"
    );
    drop(store);
    let mut generation = super::progress::prepare_generation(conn, &source, cancel, progress)?;
    let transaction = conn.unchecked_transaction()?;
    generation
        .bind(&transaction)
        .map_err(|error| TimelineProjectionError::Other(error.to_string()))?;
    let store = persistence_source::detail::require_source_store(&transaction)?;
    let timeline = TimelineRecordStore::from_source_store((*store).clone());
    let meta = TimelineProjectionRepo::new(&transaction);
    let mut timings = TimelineWriteTimings::default();
    let outcome = if file_done {
        None
    } else {
        let outcome = stream_file_events(
            &timeline,
            CatalogRecordStore::from_source_store((*store).clone()),
            FileEventStream {
                source: &source,
                platform,
                cancel,
                identity,
                timings: &mut timings,
                progress,
            },
        )?;
        meta.mark_done(FILE_PROJECTION_KEY, outcome.inserted_records, identity)?;
        Some(outcome)
    };
    let events_elapsed_ms = started.elapsed().as_millis();
    let graph_started = Instant::now();
    if !graph_done {
        super::graph::project(
            &transaction,
            &timeline,
            &source,
            cancel,
            &mut timings,
            progress,
        )?;
        meta.mark_done(GRAPH_PROJECTION_KEY, 0, identity)?;
    }
    let outcome = outcome.unwrap_or(SegmentedReplaceOutcome {
        inserted_records: 0,
        deleted_records: 0,
        segments: 0,
        max_segment_bytes: 0,
    });
    ensure_active(cancel)?;
    let commit_started = Instant::now();
    transaction.commit()?;
    timings.graph_commit += commit_started.elapsed();
    generation.commit();
    projection_stats(
        outcome,
        timings,
        estimate,
        started,
        events_elapsed_ms,
        graph_started.elapsed().as_millis(),
        policy,
    )
}

fn projection_stats(
    outcome: SegmentedReplaceOutcome,
    timings: TimelineWriteTimings,
    estimate: super::workspace::WorkspaceEstimate,
    started: Instant,
    events_elapsed_ms: u128,
    graph_elapsed_ms: u128,
    policy: timeline::TimelineProjectionPolicy,
) -> Result<TimelineProjectionStats, TimelineProjectionError> {
    Ok(TimelineProjectionStats {
        inserted_count: outcome.inserted_records,
        elapsed_ms: started.elapsed().as_millis(),
        events_elapsed_ms,
        graph_elapsed_ms,
        timings,
        already_projected: false,
        graph_complete: true,
        warnings: Vec::new(),
        workspace_required_bytes: estimate.required_bytes,
        workspace_available_bytes: policy.reservation_bytes()?,
        workspace_mutation_bytes: estimate.mutation_bytes,
        workspace_projected_events: estimate.projected_events,
        workspace_graph_events: estimate.graph_events,
        workspace_segments: outcome.segments,
        workspace_max_segment_bytes: outcome.max_segment_bytes,
        workspace_engine_retained_bytes: estimate.engine_retained_bytes,
        commit_count: outcome.segments,
    })
}

fn attach_identity(error: TimelineProjectionError, identity: &str) -> TimelineProjectionError {
    match error {
        TimelineProjectionError::WorkspaceLimit {
            required_bytes,
            available_bytes,
            projected_events,
            removed_events,
            graph_events,
            mutation_bytes,
            mutation_entries,
            ..
        } => TimelineProjectionError::WorkspaceLimit {
            required_bytes,
            available_bytes,
            projected_events,
            removed_events,
            graph_events,
            mutation_bytes,
            mutation_entries,
            identity: identity.to_owned(),
        },
        other => other,
    }
}

struct FileEventStream<'a> {
    source: &'a str,
    platform: DataSourcePlatform,
    cancel: &'a AtomicBool,
    identity: &'a str,
    timings: &'a mut TimelineWriteTimings,
    progress: Option<&'a TimelineProgressCallback<'a>>,
}

fn stream_file_events(
    timeline: &TimelineRecordStore,
    catalog: CatalogRecordStore,
    request: FileEventStream<'_>,
) -> Result<SegmentedReplaceOutcome, TimelineProjectionError> {
    let FileEventStream {
        source,
        platform,
        cancel,
        identity,
        timings,
        progress,
    } = request;
    let snapshot = catalog.read_snapshot();
    let total = catalog.count(&snapshot, Some(source))?;
    let mut processed = 0;
    if let Some(progress) = progress {
        progress(TimelineWork::TimelineEvents, 0, Some(total));
    }
    let mut cursor = None;
    let mut complete = false;
    let next_batch = move || {
        if complete {
            return Ok(None);
        }
        loop {
            ensure_active(cancel)?;
            let page = catalog.list_all(
                &snapshot,
                source,
                cursor.as_deref(),
                ScanDirection::Forward,
                512,
            )?;
            let mut events = Vec::new();
            processed += page.records.len() as u64;
            for record in page.records {
                if eligible(&record, platform) {
                    events.extend(file_events_for_record(record, source, platform));
                }
            }
            if let Some(progress) = progress {
                progress(TimelineWork::TimelineEvents, processed, Some(total));
            }
            cursor = page.next_cursor;
            if !events.is_empty() {
                complete = cursor.is_none();
                return Ok(Some(events));
            }
            if cursor.is_none() {
                complete = true;
                return Ok(None);
            }
        }
    };
    // The segmented host owns cutting commits; the producer only pages records.
    // Commit wall time accumulates across every committed segment instead of
    // being overwritten by a single measurement.
    let commit_started = Instant::now();
    let outcome = timeline
        .replace_by_parser_prefix_segmented(
            source,
            "timeline.file_",
            identity,
            PersistMode::SyncAll,
            SegmentLimits::default_limits(),
            next_batch,
        )
        .map_err(|error| match error {
            persistence_source::fjall::TimelineStreamError::Storage(error) => {
                TimelineProjectionError::Storage(error)
            }
            persistence_source::fjall::TimelineStreamError::Producer(error) => error,
        })?;
    timings.event_commit += commit_started.elapsed();
    ensure_active(cancel)?;
    Ok(outcome)
}

pub(super) fn file_events_for_record(
    record: persistence_source::fjall::FileEntryRecord,
    source: &str,
    platform: DataSourcePlatform,
) -> Vec<TimelineRecord> {
    let file = evidence_access::file_service::record_to_domain(record);
    timeline::project_file_activity(&file)
        .into_iter()
        .map(|mut event| {
            event.attrs.insert(
                "platform".into(),
                serde_json::Value::String(platform.as_storage_str().into()),
            );
            TimelineRecord {
                id: event.id.0,
                data_source_id: source.into(),
                source_object_id: event.source_object_id,
                event_type: event.event_type,
                timestamp: event.timestamp,
                title: event.title,
                description: event.description,
                parser_id: event.parser_id,
                parser_version: event.parser_version,
                confidence: event.confidence,
                source_attribution: event.source_attribution,
                attrs: event.attrs,
            }
        })
        .collect()
}

pub(super) fn eligible(
    record: &persistence_source::fjall::FileEntryRecord,
    platform: DataSourcePlatform,
) -> bool {
    if record.entry_type != persistence_source::fjall::CatalogEntryType::File {
        return false;
    }
    if platform != DataSourcePlatform::Windows {
        return !record.read_only;
    }
    if record.system || record.name.starts_with('$') {
        return false;
    }
    let path =
        analysis_core::analysis_service::normalize_evidence_path(&record.path).to_ascii_lowercase();
    ![
        "windows/",
        "program files/",
        "program files (x86)/",
        "programdata/",
        "system volume information/",
    ]
    .iter()
    .any(|prefix| path.trim_start_matches('/').starts_with(prefix))
}

fn ensure_active(cancel: &AtomicBool) -> Result<(), TimelineProjectionError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(TimelineProjectionError::Cancelled);
    }
    Ok(())
}
