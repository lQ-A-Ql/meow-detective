//! Versioned, bounded diagnostic sidecar. Full v2 table hashes remain authoritative.
use super::streaming::Normalizer;
use rusqlite::{types::Value, Connection};
use serde_json::{json, Value as Json};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, path::Path};

pub(super) fn catalog(conn: &Connection, normalizer: &Normalizer) -> Json {
    catalog_from_rows(&super::native_catalog_snapshot(
        conn,
        &normalizer.replacements(),
    ))
}

pub(super) fn catalog_from_rows(rows: &[Vec<Value>]) -> Json {
    const NAMES: [&str; 20] = [
        "id",
        "parent_id",
        "path",
        "name",
        "entry_type",
        "size",
        "ext",
        "deleted",
        "hidden",
        "system",
        "read_only",
        "archive",
        "unix_mode",
        "encrypted",
        "created_at",
        "modified_at",
        "accessed_at",
        "changed_at",
        "hash_sha256",
        "partition_index",
    ];
    let names = NAMES.map(str::to_string).to_vec();
    let mut columns = vec![Sha256::new(); names.len()];
    let mut buckets = vec![Sha256::new(); 64];
    let mut counts = [0u64; 5]; // files, dirs, deleted, roots, missing parents
    let ids = rows
        .iter()
        .filter_map(|row| match row.first() {
            Some(Value::Text(id)) => Some(id.clone()),
            _ => None,
        })
        .collect::<HashSet<_>>();
    for row in rows {
        let id = match row.first() {
            Some(Value::Text(id)) => id.clone(),
            _ => panic!("benchmark catalog id is not text"),
        };
        let bucket = Sha256::digest(id.as_bytes())[0] as usize % buckets.len();
        for (column, value) in columns.iter_mut().zip(row.iter()) {
            let cell = match value {
                Value::Null => json!(["null"]),
                Value::Integer(n) => json!(["integer", n]),
                Value::Real(n) => json!(["real", n]),
                Value::Text(s) => json!(["text", s]),
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
        let kind = match &row[4] {
            Value::Text(value) => value.as_str(),
            _ => panic!("benchmark catalog entry type is not text"),
        };
        counts[0] += u64::from(kind == "file");
        counts[1] += u64::from(kind == "directory");
        counts[2] += u64::from(matches!(&row[7], Value::Integer(1)));
        counts[3] += u64::from(matches!(&row[1], Value::Null));
        if let Value::Text(parent_id) = &row[1] {
            counts[4] += u64::from(!ids.contains(parent_id));
        }
    }
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
    let rows = super::native_catalog_snapshot(conn, &normalizer.replacements());
    let mut count = 0u64;
    for id in rows.iter().filter_map(|row| match row.first() {
        Some(Value::Text(id)) => Some(id),
        _ => None,
    }) {
        add_id(&mut expected, id);
        count += 1;
    }
    assert_eq!(results[0]["documents"], count);
    assert_eq!(
        results[0]["coverageSetSha256"],
        hex::encode(expected),
        "Search coverage differs from Catalog ids"
    );
    let preview = preview(conn, case_conn, root, case, source, normalizer);
    json!({"version": 2, "catalog": catalog(conn, normalizer), "search": results, "contentSearch": content_results, "previewExtraction": preview,
        "scope": "post-workflow; canonical local document ids; no Tantivy segment bytes; preview first three readable eligible files"})
}

pub(super) fn search_ids(
    index: &search::SearchIndex,
    query: &str,
    normalizer: &Normalizer,
) -> Json {
    let options = search::FileSearchOptions {
        query: query.into(),
        match_path: true,
        sort_field: search::FileSearchSortField::Path,
        ..Default::default()
    };
    let session = index.file_query_session(&options).unwrap();
    let mut after = None;
    // Search uses generated raw ids as a final tie-breaker. Across independent
    // cases those ids are intentionally different. Fingerprint the canonical
    // result set in a disk-backed sorter; API ordering is verified separately.
    let temporary = tempfile::tempdir().unwrap();
    let ids = Connection::open(temporary.path().join("search-ids.db")).unwrap();
    ids.execute_batch("PRAGMA cache_size=-16384; PRAGMA journal_mode=OFF; CREATE TABLE ids(id TEXT PRIMARY KEY) WITHOUT ROWID; BEGIN;").unwrap();
    let mut insert = ids.prepare("INSERT INTO ids VALUES(?1)").unwrap();
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
            insert.execute([&id]).expect("duplicate Search document id");
            add_id(&mut coverage, &id);
            count += 1;
        }
    }
    drop(insert);
    ids.execute_batch("COMMIT").unwrap();
    let mut hash = Sha256::new();
    for id in ids
        .prepare("SELECT id FROM ids ORDER BY id COLLATE BINARY")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
    {
        hash.update(id.unwrap().as_bytes());
        hash.update(b"\n");
    }
    json!({"query": query, "documents": count, "idsOrder":"canonical-id-binary", "idsSha256": hex::encode(hash.finalize()), "coverageSetSha256": hex::encode(coverage)})
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
    let mut rows = super::native_catalog_snapshot(conn, &normalizer.replacements())
        .into_iter()
        .filter(|row| {
            matches!(&row[4], Value::Text(kind) if kind == "file")
                && matches!(&row[7], Value::Integer(0))
                && matches!(&row[13], Value::Integer(0))
                && matches!(&row[5], Value::Integer(size) if (1..=65_536).contains(size))
                && !matches!(&row[3], Value::Text(name) if name.starts_with('$'))
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        let left_path = match &left[2] {
            Value::Text(value) => value,
            _ => "",
        };
        let right_path = match &right[2] {
            Value::Text(value) => value,
            _ => "",
        };
        let left_id = match &left[0] {
            Value::Text(value) => value,
            _ => "",
        };
        let right_id = match &right[0] {
            Value::Text(value) => value,
            _ => "",
        };
        left_path
            .cmp(right_path)
            .then_with(|| left_id.cmp(right_id))
    });
    rows.into_iter().take(3).map(|row| {
        let id = match &row[0] {
            Value::Text(value) => value.clone(),
            _ => panic!("benchmark preview id is not text"),
        };
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
