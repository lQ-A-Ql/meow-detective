//! Versioned, bounded diagnostic sidecar. Full v2 table hashes remain authoritative.
use super::streaming::Normalizer;
use rusqlite::{types::Value, Connection};
use serde_json::{json, Value as Json};
use sha2::{Digest, Sha256};
use std::path::Path;

pub(super) fn catalog(conn: &Connection, normalizer: &Normalizer) -> Json {
    let mut statement = conn
        .prepare(&format!(
            "{} ORDER BY benchmark_identity(id)",
            super::queries::CATALOG
        ))
        .unwrap();
    let names: Vec<String> = statement
        .column_names()
        .iter()
        .map(|v| (*v).into())
        .collect();
    let mut columns = vec![Sha256::new(); names.len()];
    let mut buckets = vec![Sha256::new(); 64];
    let mut counts = [0u64; 5]; // files, dirs, deleted, roots, missing parents
    let mut rows = statement.query([]).unwrap();
    while let Some(row) = rows.next().unwrap() {
        let id: String = row.get(0).unwrap();
        let id = normalizer.normalize(&id);
        let bucket = Sha256::digest(id.as_bytes())[0] as usize % buckets.len();
        for (i, column) in columns.iter_mut().enumerate() {
            let value: Value = row.get(i).unwrap();
            let cell = match value {
                Value::Null => json!(["null"]),
                Value::Integer(n) => json!(["integer", n]),
                Value::Real(n) => json!(["real", n]),
                Value::Text(s) => json!(["text", normalizer.normalize(&s)]),
                Value::Blob(b) => json!(["blob", hex::encode(b)]),
            };
            let bytes = serde_json::to_vec(&cell).unwrap();
            column.update(id.as_bytes());
            column.update(b"\0");
            column.update(&bytes);
            column.update(b"\n");
            buckets[bucket].update(&bytes);
            buckets[bucket].update(b"\n");
        }
        let kind: String = row.get(4).unwrap();
        counts[0] += u64::from(kind == "file");
        counts[1] += u64::from(kind == "directory");
        counts[2] += u64::from(row.get::<_, bool>(7).unwrap());
        counts[3] += u64::from(row.get::<_, Option<String>>(1).unwrap().is_none());
    }
    counts[4] = conn.query_row("SELECT COUNT(*) FROM file_entries c LEFT JOIN file_entries p ON c.parent_id=p.id WHERE c.parent_id IS NOT NULL AND p.id IS NULL", [], |r| r.get(0)).unwrap();
    let column_hashes: serde_json::Map<String, Json> = names
        .into_iter()
        .zip(columns)
        .map(|(name, hash)| (name, json!(hex::encode(hash.finalize()))))
        .collect();
    json!({"columns": column_hashes, "idHashBuckets": buckets.into_iter().map(|h| hex::encode(h.finalize())).collect::<Vec<_>>(),
        "files": counts[0], "directories": counts[1], "deleted": counts[2], "partitionRoots": counts[3], "missingParents": counts[4]})
}

pub(super) fn capture(
    conn: &Connection,
    case_conn: &Connection,
    root: &Path,
    case: &str,
    source: &str,
    normalizer: &Normalizer,
) -> Json {
    let index_path = app_services::source_db::registered_source_index_dir(
        case_conn,
        root,
        &domain::DataSourceId(source.into()),
    )
    .unwrap();
    let index = search::SearchIndex::open(&index_path).unwrap();
    let content_path = app_services::source_db::source_content_index_dir(&index_path);
    let content = content_path
        .join("meta.json")
        .is_file()
        .then(|| search::SearchIndex::open(&content_path).unwrap());
    let queries = ["", "file-16", "hello", "__oracle_absent_20261016__"];
    let results: Vec<_> = queries
        .into_iter()
        .map(|query| search_ids(&index, query, normalizer))
        .collect();
    let content_results: Vec<_> = content
        .as_ref()
        .map(|index| {
            queries
                .into_iter()
                .map(|query| search_ids(index, query, normalizer))
                .collect()
        })
        .unwrap_or_default();
    let mut expected = [0u8; 32];
    let mut statement = conn.prepare("SELECT id FROM file_entries").unwrap();
    let mut count = 0u64;
    for id in statement.query_map([], |r| r.get::<_, String>(0)).unwrap() {
        add_id(&mut expected, &normalizer.normalize(&id.unwrap()));
        count += 1;
    }
    assert_eq!(results[0]["documents"], count);
    assert_eq!(
        results[0]["coverageSetSha256"],
        hex::encode(expected),
        "Search coverage differs from Catalog ids"
    );
    let preview = preview(conn, case_conn, root, case, source, normalizer);
    json!({"version": 1, "catalog": catalog(conn, normalizer), "search": results, "contentSearch": content_results, "previewExtraction": preview,
        "scope": "post-workflow; canonical local document ids; no Tantivy segment bytes; preview first three readable eligible files"})
}

fn search_ids(index: &search::SearchIndex, query: &str, normalizer: &Normalizer) -> Json {
    let options = search::FileSearchOptions {
        query: query.into(),
        match_path: true,
        sort_field: search::FileSearchSortField::Path,
        ..Default::default()
    };
    let session = index.file_query_session(&options).unwrap();
    let mut after = None;
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut coverage = [0u8; 32];
    loop {
        let page = session.rank_after(after.as_ref(), 10_000).unwrap();
        if page.hits.is_empty() {
            assert_eq!(count, page.total_count);
            break;
        }
        for hit in page.hits {
            after = Some(hit.after_key());
            let id = normalizer.normalize(hit.file_id());
            hash.update(id.as_bytes());
            hash.update(b"\n");
            add_id(&mut coverage, &id);
            count += 1;
        }
    }
    json!({"query": query, "documents": count, "idsSha256": hex::encode(hash.finalize()), "coverageSetSha256": hex::encode(coverage)})
}

// Order independent 256-bit sum of SHA256(id), with document count checked separately.
fn add_id(sum: &mut [u8; 32], id: &str) {
    let mut carry = 0u16;
    for (byte, hash) in sum.iter_mut().zip(Sha256::digest(id.as_bytes())) {
        let value = *byte as u16 + hash as u16 + carry;
        *byte = value as u8;
        carry = value >> 8;
    }
}

fn preview(
    conn: &Connection,
    case_conn: &Connection,
    root: &Path,
    case: &str,
    source: &str,
    normalizer: &Normalizer,
) -> Vec<Json> {
    let mut statement = conn.prepare("SELECT id FROM file_entries WHERE entry_type='file' AND deleted=0 AND encrypted=0 AND size BETWEEN 1 AND 65536 AND name NOT LIKE '$%' ORDER BY path,id LIMIT 3").unwrap();
    let ids = statement.query_map([], |r| r.get::<_, String>(0)).unwrap();
    ids.map(|id| {
        let id = id.unwrap();
        let global = format!("ds:{source}:{id}");
        let case = domain::CaseId(case.into());
        let preview = app_services::file_service::text_preview_for_source_case(case_conn, root, &case, &global, Some(65536));
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("export.bin");
        match app_services::file_service::extract_file_to_destination_for_case(case_conn, root, &case, &global, &path, false) {
            Ok(_) => {
                let bytes = std::fs::read(&path).unwrap();
                assert!(app_services::file_service::extract_file_to_destination_for_case(case_conn, root, &case, &global, &path, false).is_err());
                json!({"id": normalizer.normalize(&id), "bytes": bytes.len(), "sha256": hex::encode(Sha256::digest(&bytes)),
                    "textPreview": preview.ok().map(|p| p.content)})
            }
            Err(error) => json!({"id": normalizer.normalize(&id), "unreadable": true,
                "category": transport::ServiceErrorCategory::category(&error)}),
        }
    }).collect()
}
