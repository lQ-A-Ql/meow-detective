use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::*;

#[test]
fn cancellation_during_candidate_read_discards_uncommitted_outputs() {
    let (conn, _tmp, data_source_id) = setup_case_db();
    FileRepo::new(&conn)
        .insert_batch(&[
            file_with_ds("mail-first", &data_source_id, "mailbox/first.eml", 64),
            file_with_ds("mail-second", &data_source_id, "mailbox/second.eml", 64),
        ])
        .expect("insert cancellable analysis candidates");
    let cancel_token = AtomicBool::new(false);
    let reader_calls = AtomicUsize::new(0);

    let error = run_analysis_extraction_with_cancel(
        &conn,
        "case-analysis",
        W,
        &["Email"],
        &cancel_token,
        |_, _| {
            reader_calls.fetch_add(1, Ordering::Relaxed);
            cancel_token.store(true, Ordering::Relaxed);
            Ok::<Box<dyn Read>, String>(Box::new(std::io::Cursor::new(
                b"From: analyst@example.test\r\n\r\ncancel fixture".to_vec(),
            )))
        },
    )
    .expect_err("cancellation must abort extraction");

    assert!(matches!(error, AnalysisServiceError::Cancelled));
    assert_eq!(reader_calls.load(Ordering::Relaxed), 1);
    let artifact_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM artifacts", [], |row| row.get(0))
        .expect("count analysis artifacts");
    assert_eq!(artifact_count, 0);
}
