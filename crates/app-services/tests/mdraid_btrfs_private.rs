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
    let members = plan
        .members
        .iter()
        .map(|member| evidence_access::mdraid::MdRaidDiscoveryMember {
            source_path: &member.source_path,
            source_kind: &member.source_kind,
        })
        .collect::<Vec<_>>();
    let array = discover_linux_evidence_set_mdraid(&plan.root_path, &members)
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
            persistence_source::fjall::ScanDirection::Forward,
            10,
        )
        .expect("read persisted partition metadata");
    assert_eq!(partitions.records.len(), 1);
    assert_eq!(partitions.records[0].partition_index, 0);
    assert_eq!(partitions.records[0].filesystem.as_deref(), Some("Btrfs"));
    assert!(native
        .partition_projection_complete(&snapshot, &source.id.0)
        .expect("verify partition projection marker"));
    assert!(native
        .source_projection_complete(&snapshot, &source.id.0)
        .expect("verify source projection marker"));
    assert!(FileRepo::new(&source_conn)
        .find_data_source_location(&source.id)
        .expect("read source-local location")
        .is_some());
    let tree =
        app_services::file_service::get_file_tree_for_case(&conn, &case_root, &case_id, false)
            .expect("browseable case tree");
    assert!(
        tree.iter()
            .any(|node| node.data_source_id.as_deref() == Some(source.id.0.as_str())),
        "case tree should expose the MD RAID source"
    );
    let entry = FileRepo::new(&source_conn)
        .find_by_partition_and_path(&source.id, 0, "@rootfs/etc/passwd")
        .expect("look up passwd")
        .expect("passwd entry");
    let header_cache = app_services::file_service::FileHeaderReadCache::new(case_id.0.clone());
    let header = header_cache
        .read_file_header_by_id(&source_conn, &entry.id, 256)
        .expect("read MD RAID preview header");
    assert!(
        !header.is_empty() && header.windows(5).any(|window| window == b"root:"),
        "preview should read /etc/passwd through the published MD RAID source"
    );
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
