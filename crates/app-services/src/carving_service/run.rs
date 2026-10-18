use domain::{CaseId, DataSourceId};
use evidence_core::carving::{
    CarveCandidate, CarveError, CarveLimits, CarveScope, CarveStatus, ScanRequest,
};
use rusqlite::Connection;
use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use transport::{
    dto::{CarvingRejectionDto, PngCarvingRequestDto, PngCarvingSummaryDto},
    CommandError,
};

pub fn run_png_carving(
    conn: &Connection,
    root: &Path,
    case: &CaseId,
    request: &PngCarvingRequestDto,
    cancel: &AtomicBool,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<PngCarvingSummaryDto, CommandError> {
    let (factory, scope) = super::access::prepare(conn, root, case, request)?;
    let _permit = crate::resource_budget::global_pool()
        .acquire(
            crate::resource_budget::Resources {
                bytes: 16 * crate::resource_budget::MIB,
                workers: 1,
                ..Default::default()
            },
            Some(cancel),
        )
        .map_err(CommandError::from_typed_service_error)?;
    let source = crate::source_db::open_ready_source_by_id(
        conn,
        root,
        case,
        &DataSourceId(request.data_source_id.clone()),
    )
    .map_err(CommandError::from_service_error)?;
    source
        .connection
        .busy_timeout(Duration::from_millis(200))
        .map_err(CommandError::from_service_error)?;
    let reader = factory
        .open_reader_with_cancel(cancel)
        .map_err(|_| CommandError::internal("Cannot acquire carving reader"))?;
    let stop = AtomicBool::new(false);
    let (sender, receiver) = crossbeam_channel::bounded(128);
    let mut summary = PngCarvingSummaryDto {
        range_offset: scope.offset.to_string(),
        range_length: scope.length.to_string(),
        ..Default::default()
    };
    let result = std::thread::scope(|threads| {
        let stop_ref = &stop;
        let scope_ref = &scope;
        let producer =
            threads.spawn(move || produce(sender, reader, scope_ref, stop_ref, progress));
        let consumed = consume(
            &source.connection,
            case,
            &DataSourceId(scope.source_id.clone()),
            receiver,
            cancel,
            &stop,
            &mut summary,
        );
        stop.store(true, Ordering::Release);
        let produced = producer
            .join()
            .map_err(|_| CommandError::internal("Carving producer failed"));
        consumed?;
        produced?.map_err(core_error)
    })?;
    factory
        .validate_snapshot()
        .map_err(|_| CommandError::security("Carving evidence changed"))?;
    if super::access::source_revision(conn, root, case, &scope.source_id)? != scope.revision {
        return Err(CommandError::security("Carving evidence changed"));
    }
    summary.scanned_bytes = result.scanned_bytes.to_string();
    summary.candidates = result.candidates;
    summary.complete = result.complete;
    summary.partial = result.partial;
    summary.rejected = result.rejected;
    Ok(summary)
}

fn produce(
    sender: crossbeam_channel::Sender<CarveCandidate>,
    mut reader: Box<dyn evidence_core::EvidenceReader>,
    scope: &CarveScope,
    stop: &AtomicBool,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<evidence_core::carving::ScanReport, CarveError> {
    evidence_core::carving::scan_png(
        &mut *reader,
        ScanRequest {
            scope,
            limits: CarveLimits::default(),
            cancel: stop,
            progress,
        },
        |mut candidate| loop {
            if stop.load(Ordering::Acquire) {
                return Err(CarveError::Cancelled);
            }
            match sender.send_timeout(candidate, Duration::from_millis(50)) {
                Ok(()) => return Ok(()),
                Err(crossbeam_channel::SendTimeoutError::Timeout(value)) => candidate = value,
                Err(crossbeam_channel::SendTimeoutError::Disconnected(_)) => {
                    return Err(CarveError::ConsumerStopped)
                }
            }
        },
    )
}

fn consume(
    conn: &Connection,
    case: &CaseId,
    source: &DataSourceId,
    receiver: crossbeam_channel::Receiver<CarveCandidate>,
    cancel: &AtomicBool,
    stop: &AtomicBool,
    summary: &mut PngCarvingSummaryDto,
) -> Result<(), CommandError> {
    let mut batch = Vec::with_capacity(128);
    loop {
        if cancel.load(Ordering::Acquire) {
            stop.store(true, Ordering::Release);
            return Err(CommandError::cancelled("Carving cancelled"));
        }
        match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(candidate) if candidate.status == CarveStatus::Complete => batch.push(candidate),
            Ok(candidate) => {
                if summary.rejections.len() < 64 {
                    summary.rejections.push(CarvingRejectionDto {
                        offset: candidate.offset.to_string(),
                        status: format!("{:?}", candidate.status).to_ascii_lowercase(),
                        reason: candidate.reason.into(),
                    });
                }
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
        }
        if batch.len() == 128 {
            flush(conn, case, source, &mut batch, summary, cancel)?;
        }
    }
    flush(conn, case, source, &mut batch, summary, cancel)
}
fn flush(
    conn: &Connection,
    case: &CaseId,
    source: &DataSourceId,
    batch: &mut Vec<CarveCandidate>,
    summary: &mut PngCarvingSummaryDto,
    cancel: &AtomicBool,
) -> Result<(), CommandError> {
    let artifacts = super::persistence::persist(conn, case, source, batch, cancel)?;
    for artifact in artifacts {
        if summary.artifacts.len() < 128 {
            summary.artifacts.push(artifact);
        } else {
            summary.artifacts_truncated = true;
        }
    }
    batch.clear();
    Ok(())
}
pub(super) fn core_error(error: CarveError) -> CommandError {
    match error {
        CarveError::Cancelled => CommandError::cancelled("Carving cancelled"),
        CarveError::InvalidRange | CarveError::Limit => {
            CommandError::invalid_input(error.to_string())
        }
        _ => CommandError::internal("Carving read or producer failed"),
    }
}
