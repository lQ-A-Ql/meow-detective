//! Generated disk-only trials. Invoke one condition in each fresh test process.
use crate::timeline_service::projection_graph::populate_timeline_event_graph_with_policy;
use crate::timeline_service::{
    page_cache::with_timeline_page_cache_kib, wal_policy::with_wal_policy,
};
use crate::timeline_service::{TimelineProjectionPolicy, TimelineWriteTimings};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};
#[path = "experiment_fixture.rs"]
mod experiment_fixture;

fn policy() -> TimelineProjectionPolicy {
    let value = TimelineProjectionPolicy {
        cache_kib: 1_000_000,
        graph_batch_rows: std::env::var("FORENSICS_S4_BATCH")
            .unwrap_or_else(|_| "20000".into())
            .parse()
            .unwrap(),
        cached_graph_statements: std::env::var("FORENSICS_S4_CACHED").as_deref() == Ok("true"),
        wal_autocheckpoint_pages: Some(
            std::env::var("FORENSICS_S4_WAL")
                .unwrap_or_else(|_| "1000".into())
                .parse()
                .unwrap(),
        ),
    };
    value.validate().unwrap();
    value
}

#[test]
#[ignore = "S4 generated disk graph trial; explicit performance runner"]
fn disk_graph_trial() {
    let policy = policy();
    let events: u32 = std::env::var("FORENSICS_S4_EVENTS")
        .unwrap_or_else(|_| "120000".into())
        .parse()
        .unwrap();
    assert!((120000..=3_000_000).contains(&events));
    let temporary = tempfile::tempdir().unwrap();
    let connection =
        persistence_sqlite::open_or_create_source(&temporary.path().join("source.db")).unwrap();
    experiment_fixture::seed(&connection, events);
    let previous_cache: i64 = connection
        .pragma_query_value(None, "cache_size", |r| r.get(0))
        .unwrap();
    let previous_wal: u32 = connection
        .pragma_query_value(None, "wal_autocheckpoint", |r| r.get(0))
        .unwrap();
    let mut timings = TimelineWriteTimings::default();
    let started = Instant::now();
    let warnings = with_wal_policy(&connection, policy.wal_autocheckpoint_pages, || {
        with_timeline_page_cache_kib(&connection, policy.cache_kib, || {
            populate_timeline_event_graph_with_policy(
                &connection,
                policy,
                &AtomicBool::new(false),
                &mut timings,
            )
        })
    })
    .unwrap();
    let checkpoint_started = Instant::now();
    crate::source_db::checkpoint_source_db(&connection).unwrap();
    let checkpoint_ns = checkpoint_started.elapsed().as_nanos();
    let elapsed_ns = started.elapsed().as_nanos();
    assert!(warnings.is_empty());
    let nodes: u64 = connection
        .query_row("SELECT COUNT(*) FROM graph_nodes", [], |r| r.get(0))
        .unwrap();
    let edges: u64 = connection
        .query_row("SELECT COUNT(*) FROM graph_edges", [], |r| r.get(0))
        .unwrap();
    assert_eq!(nodes, u64::from(events) + 1);
    assert_eq!(edges, u64::from(events));
    assert_eq!(
        connection
            .pragma_query_value(None, "cache_size", |r| r.get::<_, i64>(0))
            .unwrap(),
        previous_cache
    );
    assert_eq!(
        connection
            .pragma_query_value(None, "wal_autocheckpoint", |r| r.get::<_, u32>(0))
            .unwrap(),
        previous_wal
    );
    assert!(timings.graph_buffer_rows <= u64::from(policy.graph_batch_rows));
    assert!(timings.graph_wal_sampled_bytes <= 512 * 1024 * 1024);
    let digest = experiment_fixture::digest(&connection);
    println!(
        "[s4-disk] {}",
        serde_json::json!({"schemaVersion":1,"events":events,"nodes":nodes,"edges":edges,
        "batch":policy.graph_batch_rows,"cached":policy.cached_graph_statements,"walPages":policy.wal_autocheckpoint_pages,
        "elapsedNs":elapsed_ns,"checkpointNs":checkpoint_ns,"pageNs":timings.graph_page.as_nanos(),
        "nodePrepareNs":timings.graph_node_prepare.as_nanos(),"edgePrepareNs":timings.graph_edge_prepare.as_nanos(),
        "pagePrepareNs":timings.graph_page_prepare.as_nanos(),"nodeNs":timings.graph_nodes.as_nanos(),
        "edgeNs":timings.graph_edges.as_nanos(),"commitNs":timings.graph_commit.as_nanos(),
        "bufferRows":timings.graph_buffer_rows,"bufferBytes":timings.graph_buffer_bytes,"sampledWalBytes":timings.graph_wal_sampled_bytes,
        "rssMiB":crate::runtime_resources::peak_rss_mb(),"graphSha256":digest,"warnings":warnings,
        "limits":["WAL sampled at batch boundaries; not physical writes","anonymous sort files excluded from directory samples"]})
    );
}

#[test]
#[ignore = "S4 generated disk cancellation trial; explicit performance runner"]
fn disk_graph_cancel_trial() {
    let policy = policy();
    let batch = policy.graph_batch_rows;
    let temporary = tempfile::tempdir().unwrap();
    let connection =
        persistence_sqlite::open_or_create_source(&temporary.path().join("source.db")).unwrap();
    experiment_fixture::seed(&connection, batch * 2 + 10);
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = cancel.clone();
    let cancelled_at = Arc::new(std::sync::Mutex::new(None));
    let signal_time = cancelled_at.clone();
    connection
        .create_scalar_function(
            "s4_cancel",
            0,
            rusqlite::functions::FunctionFlags::default(),
            move |_| {
                *signal_time.lock().unwrap() = Some(Instant::now());
                signal.store(true, Ordering::Release);
                Ok(0)
            },
        )
        .unwrap();
    connection
        .execute_batch(&format!(
            "CREATE TRIGGER s4_cancel_batch AFTER INSERT ON graph_edges
        WHEN NEW.source_id='event:{:08}' BEGIN SELECT s4_cancel(); END;",
            batch
        ))
        .unwrap();
    let result = populate_timeline_event_graph_with_policy(
        &connection,
        policy,
        &cancel,
        &mut TimelineWriteTimings::default(),
    );
    let drain_ms = cancelled_at.lock().unwrap().unwrap().elapsed().as_millis();
    assert!(matches!(
        result,
        Err(crate::timeline_service::TimelineServiceError::Cancelled)
    ));
    assert!(drain_ms <= 2000, "cancel drain exceeded 2s: {drain_ms}");
    let edges: u64 = connection
        .query_row("SELECT COUNT(*) FROM graph_edges", [], |r| r.get(0))
        .unwrap();
    assert_eq!(edges, u64::from(batch));
    connection
        .execute_batch("DROP TRIGGER s4_cancel_batch")
        .unwrap();
    cancel.store(false, Ordering::Release);
    assert!(populate_timeline_event_graph_with_policy(
        &connection,
        policy,
        &cancel,
        &mut TimelineWriteTimings::default()
    )
    .unwrap()
    .is_empty());
    let recovered: u64 = connection
        .query_row("SELECT COUNT(*) FROM graph_edges", [], |r| r.get(0))
        .unwrap();
    assert_eq!(recovered, u64::from(batch * 2 + 10));
    println!(
        "[s4-cancel] {}",
        serde_json::json!({"batch":batch,"cached":policy.cached_graph_statements,"drainMs":drain_ms,"committedEdges":edges,"retryEdges":recovered})
    );
}
