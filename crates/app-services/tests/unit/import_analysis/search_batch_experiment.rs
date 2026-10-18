use super::*;
use domain::{DataSourceId, DataSourcePlatform};
use std::sync::{atomic::AtomicBool, Arc};

#[test]
#[ignore = "S4 generated search page trial; explicit performance runner"]
fn disk_search_trial() {
    let page_size: u64 = std::env::var("FORENSICS_S4_SEARCH_PAGE")
        .unwrap_or_else(|_| "1000".into())
        .parse()
        .unwrap();
    let temporary = tempfile::tempdir().unwrap();
    let db_path = temporary.path().join("source.db");
    let conn = persistence_sqlite::open_or_create_source(&db_path).unwrap();
    conn.execute_batch("WITH RECURSIVE n(v) AS (VALUES(0) UNION ALL SELECT v+1 FROM n WHERE v<9999)
        INSERT INTO file_entries(id,data_source_id,path,name,entry_type,size,ext,deleted,hidden,system,encrypted)
        SELECT printf('mft:%08d',v),'source',printf('[P1]/report%08d.txt',v),printf('report%08d.txt',v),'file',v,'txt',0,0,0,0 FROM n;").unwrap();
    crate::source_db::checkpoint_source_db(&conn).unwrap();
    drop(conn);
    let index_dir = temporary.path().join("search");
    let started = std::time::Instant::now();
    let stats = run_search_index_phase_with_page_size(
        SearchIndexPhaseOptions {
            publication_lease: None,
            db_path,
            data_source_id: DataSourceId("source".into()),
            platform: DataSourcePlatform::Windows,
            index_dir: index_dir.clone(),
            cancel_token: Arc::new(AtomicBool::new(false)),
        },
        page_size,
    )
    .unwrap();
    let elapsed = started.elapsed();
    assert_eq!(stats.indexed_count, 10000);
    assert_eq!(stats.failed_count, 0);
    let index = SearchIndex::open(&index_dir).unwrap();
    assert_eq!(index.document_count().unwrap(), 10000);
    let query = search::FileSearchOptions {
        query: "report".into(),
        ..Default::default()
    };
    let session = index.file_query_session(&query).unwrap();
    let hits = session.rank_after(None, 10000).unwrap();
    assert_eq!(hits.total_count, 10000);
    let mut identities = hits
        .hits
        .iter()
        .map(|hit| hit.file_id().to_string())
        .collect::<Vec<_>>();
    identities.sort();
    assert_eq!(
        identities,
        (0..10000)
            .map(|id| format!("mft:{id:08}"))
            .collect::<Vec<_>>()
    );
    use sha2::Digest;
    let digest = hex::encode(sha2::Sha256::digest(identities.join("\n")));
    println!(
        "[s4-search] {}",
        serde_json::json!({"pageSize":page_size,"documents":stats.indexed_count,
            "elapsedNs":elapsed.as_nanos(),"rssMiB":crate::runtime_resources::peak_rss_mb(),"querySha256":digest,
        "visibility":"complete generation published after final commit; metadata-only; no evidence reader"})
    );
}

#[test]
fn catalog_revision_changes_are_detected_on_the_bound_connection() {
    let temporary = tempfile::tempdir().unwrap();
    let db_path = temporary.path().join("source.db");
    let writer = persistence_sqlite::open_or_create_source(&db_path).unwrap();
    let reader = persistence_sqlite::open_existing_source_read_only(&db_path).unwrap();
    let source = DataSourceId("source".into());
    let projection = MetadataProjection::new(&source, data_version(&reader).unwrap());
    writer
        .execute(
            "INSERT INTO source_meta(key,value) VALUES ('test','changed')",
            [],
        )
        .unwrap();
    assert!(projection
        .validate(&source, &data_version(&reader).unwrap())
        .is_err());
}
