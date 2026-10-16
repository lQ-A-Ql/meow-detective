//! Bounded-memory oracle for large NTFS benchmarks. Sorting uses SQLite's disk
//! sorter, and only one row is materialized in Rust at a time.
use rusqlite::{functions::FunctionFlags, types::Value, Connection};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, path::Path, sync::Arc};

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Fingerprint {
    pub rows: u64,
    pub sha256: String,
}

pub(super) fn profile(path: &Path, workers: usize) -> serde_json::Value {
    // The runner fingerprints either one raw file or a complete E01 segment
    // set. Refuse virtual adapters whose backing files are outside that scope.
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("e01"))
    {
        use evidence_core::EvidenceReader;
        let reader = evidence_core::RawImageReader::open(path).unwrap();
        assert_eq!(
            reader.info().kind,
            "raw",
            "benchmark requires bare RAW bytes"
        );
        assert_eq!(reader.backing_paths().len(), 1);
    }
    super::with_import(path, true, workers, |conn, case, source, report| {
        let replacements = super::canonical_replacements(conn, case, source);
        let normalizer = configure(conn, replacements);
        let catalog = fingerprint(conn, &[super::queries::CATALOG], &normalizer);
        let graph = fingerprint(
            conn,
            &[super::queries::NODES, super::queries::EDGES],
            &normalizer,
        );
        let timeline = fingerprint(conn, &[super::queries::TIMELINE], &normalizer);
        assert!(
            report.partition_aggregates.iter().any(|sample| {
                sample.phase == super::Phase::MftRead && sample.scanned_records > 0
            }),
            "benchmark requires at least one NTFS MFT scan"
        );
        serde_json::json!({
            "schemaVersion": 2, "oracleVersion": 2,
            "oracleOrder": "canonical-primary-id; graph-nodes-before-edges",
            "analysisMode": "metadataOnly", "platform": "windows",
            "workersRequested": workers,
            "workersResolved": app_services::parallel_enum::resolve_worker_count(Some(workers)),
            "catalogRows": catalog.rows, "catalogSha256": catalog.sha256,
            "graphRows": graph.rows, "graphSha256": graph.sha256,
            "timelineRows": timeline.rows, "timelineSha256": timeline.sha256,
            "report": report,
        })
    })
}

#[derive(Clone)]
pub(super) struct Normalizer {
    identities: Arc<HashMap<String, String>>,
    regex: Arc<regex::Regex>,
}

impl Normalizer {
    fn normalize(&self, value: &str) -> String {
        self.regex
            .replace_all(value, |capture: &regex::Captures<'_>| {
                self.identities
                    .get(&capture[0])
                    .cloned()
                    .unwrap_or_else(|| capture[0].to_owned())
            })
            .into_owned()
    }
}

pub(super) fn configure(conn: &Connection, replacements: Vec<(String, String)>) -> Normalizer {
    conn.execute_batch("PRAGMA cache_size=-16384; PRAGMA temp_store=FILE; PRAGMA threads=1;")
        .unwrap();
    let normalizer = Normalizer {
        identities: Arc::new(replacements.into_iter().collect()),
        regex: Arc::new(
            regex::Regex::new(r"(?i)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
                .unwrap(),
        ),
    };
    let function = normalizer.clone();
    conn.create_scalar_function(
        "benchmark_identity",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        move |context| {
            let value = context.get::<String>(0)?;
            Ok(function.normalize(&value))
        },
    )
    .unwrap();
    normalizer
}

pub(super) fn fingerprint(
    conn: &Connection,
    queries: &[&str],
    normalizer: &Normalizer,
) -> Fingerprint {
    let mut hasher = Sha256::new();
    let mut count = 0;
    for query in queries {
        // Every exported table has a unique, non-null text id. Canonical ids
        // therefore define a total order independently of generated UUIDs.
        let mut statement = conn
            .prepare(&format!("{query} ORDER BY benchmark_identity(id)"))
            .unwrap();
        let columns = statement.column_count();
        let mut rows = statement.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            let mut values = Vec::with_capacity(columns);
            for column in 0..columns {
                let value: Value = row.get(column).unwrap();
                values.push(match value {
                    Value::Null => serde_json::json!(["null"]),
                    Value::Integer(n) => serde_json::json!(["integer", n]),
                    Value::Real(n) => serde_json::json!(["real", n]),
                    Value::Text(value) => {
                        let normalized = normalizer.normalize(&value);
                        serde_json::json!(["text", normalized])
                    }
                    Value::Blob(value) => serde_json::json!(["blob", hex::encode(value)]),
                });
            }
            hasher.update(serde_json::to_vec(&values).unwrap());
            hasher.update(b"\n");
            count += 1;
        }
    }
    Fingerprint {
        rows: count,
        sha256: hex::encode(hasher.finalize()),
    }
}
