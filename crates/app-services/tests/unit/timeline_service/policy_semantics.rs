use crate::timeline_service::{
    projection_graph::populate_timeline_event_graph_with_policy, TimelineProjectionPolicy,
    TimelineWriteTimings,
};
use std::sync::atomic::AtomicBool;

#[test]
fn cached_and_uncached_ranges_preserve_missing_target_empty_source_and_replay() {
    for cached in [false, true] {
        for require_target in [false, true] {
            let conn = persistence_sqlite::open_in_memory().unwrap();
            persistence_sqlite::runner::run_source_all(&conn).unwrap();
            if require_target {
                conn.execute_batch("DROP TABLE graph_edges;
                CREATE TABLE graph_edges(id TEXT PRIMARY KEY,case_id TEXT,source_id TEXT,target_id TEXT,
                    edge_type TEXT,confidence REAL,provenance TEXT,created_at TEXT,
                    FOREIGN KEY(source_id) REFERENCES graph_nodes(id),
                    FOREIGN KEY(target_id) REFERENCES graph_nodes(id));").unwrap();
            }
            conn.execute_batch(
                "PRAGMA foreign_keys=ON;
            INSERT INTO timeline_events(id,case_id,source_object_id,event_type,ts,title,confidence)
            VALUES ('a','case','file','FILE_CREATED','2026-10-18T00:00:00Z','label',0.75),
                   ('b','case','','FILE_CREATED','2026-10-18T00:00:00Z','empty',0.75);",
            )
            .unwrap();
            let policy = TimelineProjectionPolicy {
                graph_batch_rows: 1,
                cached_graph_statements: cached,
                ..Default::default()
            };
            let cancel = AtomicBool::new(false);
            let run = || {
                populate_timeline_event_graph_with_policy(
                    &conn,
                    policy,
                    &cancel,
                    &mut TimelineWriteTimings::default(),
                )
                .unwrap()
            };
            assert!(run()[0].starts_with(if require_target {
                "2 timeline"
            } else {
                "1 timeline"
            }));
            assert_eq!(
                conn.query_row("SELECT COUNT(*) FROM graph_edges", [], |r| r
                    .get::<_, u32>(0))
                    .unwrap(),
                if require_target { 0 } else { 1 }
            );
            conn.execute("INSERT INTO graph_nodes(id,case_id,node_type,label,created_at) VALUES ('file','case','file','target','fixed')", []).unwrap();
            assert!(run()[0].starts_with("1 timeline"));
            assert!(run()[0].starts_with("1 timeline"));
            assert_eq!(
                conn.query_row("SELECT COUNT(*) FROM graph_nodes", [], |r| r
                    .get::<_, u32>(0))
                    .unwrap(),
                3
            );
            let edge: (String, String, f64) = conn
                .query_row(
                    "SELECT id,provenance,confidence FROM graph_edges",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap();
            assert_eq!(
                edge,
                (
                    "references:a:file".into(),
                    "timeline:FILE_CREATED".into(),
                    0.75
                )
            );
            assert_eq!(
                conn.query_row(
                    "SELECT label||':'||summary||':'||tags FROM graph_nodes WHERE id='a'",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
                "label:FILE_CREATED:[]"
            );
        }
    }
}
