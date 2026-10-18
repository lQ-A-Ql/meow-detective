use super::{
    insert_timeline_row, populate_timeline_event_graph_with_batch_inner, projection_connection,
    AtomicBool, TimelineServiceError, TimelineWriteTimings,
};
use std::sync::{atomic::Ordering, Arc};
use transport::{ErrorCategory, ServiceErrorCategory};

#[test]
fn cancellation_during_edges_rolls_back_only_the_current_graph_batch() {
    let connection = projection_connection();
    for id in ["event-a", "event-b", "event-c", "event-d"] {
        insert_timeline_row(&connection, id, "file");
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = cancel.clone();
    connection
        .create_scalar_function(
            "cancel_graph",
            0,
            rusqlite::functions::FunctionFlags::default(),
            move |_| {
                signal.store(true, Ordering::Release);
                Ok(0)
            },
        )
        .unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER cancel_late_edges AFTER INSERT ON graph_edges
             WHEN NEW.source_id='event-d'
             BEGIN SELECT cancel_graph(); END;",
        )
        .unwrap();
    let result = populate_timeline_event_graph_with_batch_inner(
        &connection,
        2,
        &cancel,
        &mut TimelineWriteTimings::default(),
    );
    assert!(matches!(result, Err(TimelineServiceError::Cancelled)));
    for table in ["graph_nodes", "graph_edges"] {
        let count: u64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(
            count, 2,
            "the interrupted second batch must roll back: {table}"
        );
    }
    connection
        .execute_batch("DROP TRIGGER cancel_late_edges")
        .unwrap();
    cancel.store(false, Ordering::Release);
    populate_timeline_event_graph_with_batch_inner(
        &connection,
        2,
        &cancel,
        &mut TimelineWriteTimings::default(),
    )
    .unwrap();
    let edges: u64 = connection
        .query_row("SELECT COUNT(*) FROM graph_edges", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        edges, 4,
        "retry must preserve committed batches without duplication"
    );
}

#[test]
fn timeline_cancellation_preserves_the_cancelled_error_category() {
    let error = TimelineServiceError::Cancelled;
    assert!(matches!(error.category(), ErrorCategory::Cancelled));
    assert_eq!(error.recoverable(), Some(true));
}

#[test]
fn cancellation_during_index_rebuild_rolls_back_events_and_completion() {
    use super::materialize_file_activity_unknown_with_cancel;
    use rusqlite::functions::FunctionFlags;
    let connection = projection_connection();
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = cancel.clone();
    connection
        .create_scalar_function(
            "index_cancel",
            1,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            move |ctx| {
                signal.store(true, Ordering::Release);
                ctx.get::<String>(0)
            },
        )
        .unwrap();
    connection
        .execute_batch(
            "CREATE INDEX cancel_index ON timeline_events(index_cancel(title));
         INSERT INTO file_entries(id,data_source_id,path,name,entry_type,modified_at)
         VALUES ('file','ds-1','/file','file','file','2026-07-18T00:00:00Z');",
        )
        .unwrap();
    let result = materialize_file_activity_unknown_with_cancel(&connection, &cancel);
    assert!(matches!(result, Err(TimelineServiceError::Cancelled)));
    assert!(connection.is_autocommit());
    for table in [
        "timeline_events",
        "graph_nodes",
        "graph_edges",
        "timeline_projection_meta",
    ] {
        let count: u64 = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "cancelled projection must not publish {table}");
    }
    let indexes: u64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name='cancel_index'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(indexes, 1);
}
