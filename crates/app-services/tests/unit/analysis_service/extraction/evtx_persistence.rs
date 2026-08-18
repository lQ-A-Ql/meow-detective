use super::*;
use domain::FileEntryId;
use rusqlite::{params, Connection};
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::sync::atomic::AtomicBool;

const CASE_ID: &str = "case-evtx";
const DATA_SOURCE_ID: &str = "source-evtx";

#[test]
fn fatal_read_after_multiple_chunks_rolls_back_partial_outputs() {
    let connection = source_connection();
    let bytes = fixture_bytes();
    let candidate = candidate(bytes.len());
    seed_previous_outputs(&connection, &candidate);
    let fail_at = 4_096 + 65_536 * 14;
    assert_supported_event_exists_before(&bytes, fail_at, &candidate.path);
    let mut reader = ReadFault::new(bytes, fail_at as u64, std::io::ErrorKind::Other);

    let result = persist_evtx_candidate_with_batch_size(
        &connection,
        CASE_ID,
        &candidate,
        event_log_capability(),
        &mut reader,
        &AtomicBool::new(false),
        1,
    )
    .expect("fatal source I/O should defer this candidate without aborting the run");

    assert!(matches!(
        result,
        EvtxPersistenceResult::Deferred(EvtxBootError::SourceIo {
            kind: std::io::ErrorKind::Other,
            ..
        })
    ));
    assert_previous_outputs_unchanged(&connection, &candidate);
    assert_no_candidate_checkpoint(&connection);
    assert_retry_replaces_previous_outputs(&connection, &candidate, fixture_bytes());
}

#[test]
fn fatal_seek_after_multiple_chunks_rolls_back_partial_outputs() {
    let connection = source_connection();
    let bytes = fixture_bytes();
    let candidate = candidate(bytes.len());
    seed_previous_outputs(&connection, &candidate);
    let fail_at = 4_096 + 65_536 * 10;
    assert_supported_event_exists_before(&bytes, fail_at, &candidate.path);
    let mut reader = SeekFault::new(bytes, fail_at as u64);

    let result = persist_evtx_candidate_with_batch_size(
        &connection,
        CASE_ID,
        &candidate,
        event_log_capability(),
        &mut reader,
        &AtomicBool::new(false),
        1,
    )
    .expect("fatal source I/O should defer this candidate without aborting the run");

    assert!(matches!(
        result,
        EvtxPersistenceResult::Deferred(EvtxBootError::SourceIo {
            kind: std::io::ErrorKind::Other,
            ..
        })
    ));
    assert_previous_outputs_unchanged(&connection, &candidate);
    assert_no_candidate_checkpoint(&connection);
}

#[test]
fn interrupted_source_read_rolls_back_and_remains_retryable_without_cancellation() {
    let connection = source_connection();
    let bytes = fixture_bytes();
    let candidate = candidate(bytes.len());
    seed_previous_outputs(&connection, &candidate);
    let fail_at = 4_096 + 65_536 * 8;
    let mut reader = ReadFault::new(bytes, fail_at as u64, std::io::ErrorKind::Interrupted);

    let result = persist_evtx_candidate_with_batch_size(
        &connection,
        CASE_ID,
        &candidate,
        event_log_capability(),
        &mut reader,
        &AtomicBool::new(false),
        1,
    )
    .expect("an interrupted source read without cancellation must remain retryable");

    assert!(matches!(
        result,
        EvtxPersistenceResult::Deferred(EvtxBootError::SourceIo {
            kind: std::io::ErrorKind::Interrupted,
            ..
        })
    ));
    assert_previous_outputs_unchanged(&connection, &candidate);
    assert_no_candidate_checkpoint(&connection);
}

#[test]
fn interrupted_read_with_cancelled_token_preserves_cancellation_semantics() {
    let connection = source_connection();
    let bytes = fixture_bytes();
    let candidate = candidate(bytes.len());
    seed_previous_outputs(&connection, &candidate);
    let cancel_token = AtomicBool::new(false);
    let fail_at = 4_096 + 65_536 * 8;
    let mut reader = CancellingReadFault::new(bytes, fail_at as u64, &cancel_token);

    let result = persist_evtx_candidate_with_batch_size(
        &connection,
        CASE_ID,
        &candidate,
        event_log_capability(),
        &mut reader,
        &cancel_token,
        1,
    );

    assert!(matches!(result, Err(AnalysisServiceError::Cancelled)));
    assert_previous_outputs_unchanged(&connection, &candidate);
    assert_no_candidate_checkpoint(&connection);
}

#[test]
fn parser_initialization_io_failure_preserves_previous_outputs_without_checkpoint() {
    let connection = source_connection();
    let candidate = candidate(4_096);
    seed_previous_outputs(&connection, &candidate);
    let mut reader = ReadFault::new(vec![0_u8; 4_096], 0, std::io::ErrorKind::PermissionDenied);

    let result = persist_evtx_candidate_with_batch_size(
        &connection,
        CASE_ID,
        &candidate,
        event_log_capability(),
        &mut reader,
        &AtomicBool::new(false),
        1,
    )
    .expect("initialization failure should remain retryable");

    assert!(matches!(
        result,
        EvtxPersistenceResult::Deferred(EvtxBootError::SourceIo {
            kind: std::io::ErrorKind::PermissionDenied,
            ..
        })
    ));
    assert_previous_outputs_unchanged(&connection, &candidate);
    assert_no_candidate_checkpoint(&connection);
}

#[test]
fn malformed_header_preserves_previous_outputs_without_checkpoint() {
    let connection = source_connection();
    let candidate = candidate(4_096);
    seed_previous_outputs(&connection, &candidate);
    let mut reader = Cursor::new(vec![0_u8; 4_096]);

    let result = persist_evtx_candidate_with_batch_size(
        &connection,
        CASE_ID,
        &candidate,
        event_log_capability(),
        &mut reader,
        &AtomicBool::new(false),
        1,
    )
    .expect("deterministic initialization failure should remain retryable");

    assert!(matches!(
        result,
        EvtxPersistenceResult::Deferred(EvtxBootError::ParserInit { .. })
    ));
    assert_previous_outputs_unchanged(&connection, &candidate);
    assert_no_candidate_checkpoint(&connection);
}

#[test]
fn persistence_sink_failure_rolls_back_replacement_and_checkpoint() {
    let connection = source_connection();
    let bytes = fixture_bytes();
    let candidate = candidate(bytes.len());
    seed_previous_outputs(&connection, &candidate);
    connection
        .execute_batch(
            "CREATE TEMP TRIGGER reject_new_evtx_artifact
             BEFORE INSERT ON artifacts
             WHEN NEW.id <> 'old-evtx-artifact'
             BEGIN
                 SELECT RAISE(ABORT, 'injected EVTX sink failure');
             END;",
        )
        .expect("install persistence fault trigger");
    let mut reader = Cursor::new(bytes);

    let result = persist_evtx_candidate_with_batch_size(
        &connection,
        CASE_ID,
        &candidate,
        event_log_capability(),
        &mut reader,
        &AtomicBool::new(false),
        1,
    );
    let error = match result {
        Err(error) => error,
        Ok(_) => panic!("sink failure must abort EVTX replacement"),
    };

    assert!(matches!(error, AnalysisServiceError::Db(_)));
    assert_previous_outputs_unchanged(&connection, &candidate);
    assert_no_candidate_checkpoint(&connection);
}

fn source_connection() -> Connection {
    let connection = persistence_sqlite::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&connection).expect("run source migrations");
    connection
        .execute(
            "INSERT INTO data_sources
             (id, case_id, name, kind, source_path, imported_at)
             VALUES (?1, ?2, 'EVTX source', 'e01', 'evidence.E01',
                     '2026-07-26T00:00:00Z')",
            params![DATA_SOURCE_ID, CASE_ID],
        )
        .expect("insert source registration");
    connection
}

fn candidate(size: usize) -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: FileEntryId("system-evtx".to_string()),
        data_source_id: DATA_SOURCE_ID.to_string(),
        partition_index: Some(2),
        path: "Windows/System32/winevt/Logs/System.evtx".to_string(),
        size: size as u64,
        encrypted: false,
        content_identity: "fixture:system-evtx".to_string(),
        companions: Vec::new(),
        modified_at: None,
        evidence_kind: "evtx_log".to_string(),
        parser: "evtx.structured".to_string(),
        category: "EventLogs".to_string(),
    }
}

fn event_log_capability() -> AnalysisCapability {
    crate::analysis_service::capability::find_capability("EventLogs").expect("EventLogs capability")
}

fn fixture_bytes() -> Vec<u8> {
    std::fs::read(testing::fixtures::tiny_system_evtx()).expect("read System.evtx fixture")
}

fn assert_supported_event_exists_before(bytes: &[u8], cutoff: usize, path: &str) {
    let mut count = 0_u64;
    artifacts_windows::visit_structured_events_from_read_seek(
        Cursor::new(bytes[..cutoff].to_vec()),
        path,
        |_| {
            count = count.saturating_add(1);
            Ok::<(), std::convert::Infallible>(())
        },
    )
    .expect("the pre-fault prefix should parse");
    assert!(
        count > 0,
        "the injected failure must occur after output writes"
    );
}

fn seed_previous_outputs(connection: &Connection, candidate: &EvidenceCandidate) {
    connection
        .execute(
            "INSERT INTO artifacts
             (id, case_id, data_source_id, artifact_type, source_object_id,
              extractor_id, title, summary, attrs)
             VALUES ('old-evtx-artifact', ?1, ?2, 'EvtxBootShutdown', ?3,
                     'evtx.previous', 'old artifact', 'old', '{}')",
            params![CASE_ID, DATA_SOURCE_ID, candidate.file_id.0],
        )
        .expect("seed old artifact");
    connection
        .execute(
            "INSERT INTO timeline_events
             (id, case_id, source_object_id, event_type, ts, title, description,
              parser_id, attrs)
             VALUES ('old-evtx-timeline', ?1, ?2, 'Boot',
                     '2026-01-01T00:00:00Z', 'old timeline', 'old',
                     'evtx.previous', '{}')",
            params![CASE_ID, candidate.file_id.0],
        )
        .expect("seed old timeline event");
}

fn assert_previous_outputs_unchanged(connection: &Connection, candidate: &EvidenceCandidate) {
    let artifacts = ArtifactRepo::new(connection)
        .list_analysis_outputs(&candidate.file_id.0, "evtx.")
        .expect("list EVTX artifacts");
    let timeline = TimelineRepo::new(connection)
        .list_analysis_outputs(&candidate.file_id.0, "evtx.")
        .expect("list EVTX timeline events");
    assert_eq!(artifacts.len(), 1);
    assert_eq!(artifacts[0].id.0, "old-evtx-artifact");
    assert_eq!(timeline.len(), 1);
    assert_eq!(timeline[0].id.0, "old-evtx-timeline");
}

fn assert_no_candidate_checkpoint(connection: &Connection) {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM source_meta
             WHERE key LIKE 'analysis_candidate_scan:%'",
            [],
            |row| row.get(0),
        )
        .expect("count candidate checkpoints");
    assert_eq!(count, 0);
}

fn assert_retry_replaces_previous_outputs(
    connection: &Connection,
    candidate: &EvidenceCandidate,
    bytes: Vec<u8>,
) {
    let result = persist_evtx_candidate_with_batch_size(
        connection,
        CASE_ID,
        candidate,
        event_log_capability(),
        &mut Cursor::new(bytes),
        &AtomicBool::new(false),
        1,
    )
    .expect("retry should execute");
    assert!(matches!(result, EvtxPersistenceResult::Persisted(_)));

    let old_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM artifacts WHERE id = 'old-evtx-artifact'",
            [],
            |row| row.get(0),
        )
        .expect("count old artifacts");
    let checkpoint_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM source_meta
             WHERE key LIKE 'analysis_candidate_scan:complete:%'",
            [],
            |row| row.get(0),
        )
        .expect("count complete checkpoints");
    assert_eq!(old_count, 0);
    assert_eq!(checkpoint_count, 1);
}

struct ReadFault {
    inner: Cursor<Vec<u8>>,
    fail_at: u64,
    kind: std::io::ErrorKind,
}

impl ReadFault {
    fn new(bytes: Vec<u8>, fail_at: u64, kind: std::io::ErrorKind) -> Self {
        Self {
            inner: Cursor::new(bytes),
            fail_at,
            kind,
        }
    }
}

impl Read for ReadFault {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.inner.position() >= self.fail_at {
            return Err(std::io::Error::new(
                self.kind,
                "injected EVTX evidence read failure",
            ));
        }
        let allowed = usize::try_from(self.fail_at - self.inner.position())
            .unwrap_or(usize::MAX)
            .min(buffer.len());
        self.inner.read(&mut buffer[..allowed])
    }
}

impl Seek for ReadFault {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(position)
    }
}

struct CancellingReadFault<'a> {
    inner: Cursor<Vec<u8>>,
    fail_at: u64,
    cancel_token: &'a AtomicBool,
}

impl<'a> CancellingReadFault<'a> {
    fn new(bytes: Vec<u8>, fail_at: u64, cancel_token: &'a AtomicBool) -> Self {
        Self {
            inner: Cursor::new(bytes),
            fail_at,
            cancel_token,
        }
    }
}

impl Read for CancellingReadFault<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.inner.position() >= self.fail_at {
            self.cancel_token
                .store(true, std::sync::atomic::Ordering::Relaxed);
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "injected EVTX cancellation",
            ));
        }
        let allowed = usize::try_from(self.fail_at - self.inner.position())
            .unwrap_or(usize::MAX)
            .min(buffer.len());
        self.inner.read(&mut buffer[..allowed])
    }
}

impl Seek for CancellingReadFault<'_> {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(position)
    }
}

struct SeekFault {
    inner: Cursor<Vec<u8>>,
    fail_at: u64,
}

impl SeekFault {
    fn new(bytes: Vec<u8>, fail_at: u64) -> Self {
        Self {
            inner: Cursor::new(bytes),
            fail_at,
        }
    }
}

impl Read for SeekFault {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buffer)
    }
}

impl Seek for SeekFault {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        if matches!(position, SeekFrom::Start(offset) if offset >= self.fail_at) {
            return Err(std::io::Error::other("injected EVTX evidence seek failure"));
        }
        self.inner.seek(position)
    }
}
