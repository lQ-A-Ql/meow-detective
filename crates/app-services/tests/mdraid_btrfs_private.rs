use app_services::cluster_service::{
    discover_linux_evidence_set_mdraid, materialize_mdraid_array, plan_linux_evidence_set_import,
};
use domain::{CaseId, CaseMeta};
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::{case_repo::CaseRepo, datasource_repo::DataSourceRepo};
use std::time::Instant;

#[test]
#[ignore = "requires FORENSICS_PVE_CLUSTER_ROOT real mdraid Btrfs E01 directory"]
fn mdraid_btrfs_catalog_materializes_to_completion() {
    let root = std::env::var_os("FORENSICS_PVE_CLUSTER_ROOT")
        .map(std::path::PathBuf::from)
        .expect("set FORENSICS_PVE_CLUSTER_ROOT");
    let plan = plan_linux_evidence_set_import(&root, Some("mdraid-btrfs".into()))
        .expect("plan Linux evidence set");
    let array = discover_linux_evidence_set_mdraid(&plan)
        .expect("discover MD RAID")
        .into_iter()
        .find(|array| array.name == "mac:0")
        .expect("discover mac:0");

    let temporary = tempfile::tempdir().expect("temporary directory");
    let case_root = temporary.path().join("case");
    std::fs::create_dir_all(&case_root).expect("case root");
    let conn = persistence_sqlite::connection::open_in_memory().expect("case database");
    persistence_sqlite::runner::run_all(&conn).expect("case migrations");
    let case_id = CaseId("mdraid-btrfs-materialization".into());
    let now = chrono::Utc::now();
    CaseRepo::new(&conn)
        .create(&CaseMeta {
            id: case_id.clone(),
            name: "MD RAID Btrfs".into(),
            number: None,
            examiner: None,
            notes: None,
            created_at: now,
            updated_at: now,
        })
        .expect("create case");

    let started = Instant::now();
    let source = materialize_mdraid_array(&conn, &case_root, &case_id, &array)
        .expect("materialize MD RAID Btrfs");
    let elapsed = started.elapsed();
    let source_path = app_services::source_db::source_db_path(&case_root, &source.id);
    let source_conn =
        persistence_sqlite::connection::open_existing(&source_path).expect("open source database");
    let count = FileRepo::new(&source_conn)
        .count_by_data_source(&source.id)
        .expect("count catalog records");
    let native =
        app_services::source_db::open_registered_fjall_source_read_only(&case_root, &source.id)
            .expect("open published Fjall source");
    let snapshot = native.snapshot();
    let partitions = native
        .partition_list_by_source(
            &snapshot,
            &source.id.0,
            None,
            persistence_fjall::ScanDirection::Forward,
            10,
        )
        .expect("read persisted partition metadata");
    assert_eq!(partitions.records.len(), 1);
    assert_eq!(partitions.records[0].partition_index, 0);
    assert_eq!(partitions.records[0].filesystem.as_deref(), Some("Btrfs"));
    assert!(native
        .partition_projection_complete(&snapshot, &source.id.0)
        .expect("verify partition projection marker"));
    let storage = DataSourceRepo::new(&conn)
        .find_storage(&source.id)
        .expect("read storage")
        .expect("source storage");
    let rss_mb = app_services::import_analysis::current_rss_mb();
    let peak_rss_mb = app_services::import_analysis::peak_rss_mb();
    eprintln!(
        "MD RAID Btrfs: records={count} elapsed_ms={} state={} rss_mb={rss_mb} peak_rss_mb={peak_rss_mb}",
        elapsed.as_millis(),
        storage.import_state
    );
    assert!(count > 1_000, "catalog should contain a materialized tree");
    assert_eq!(storage.import_state, "ready");
}
