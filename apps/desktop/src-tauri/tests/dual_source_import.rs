//! Real-sample dual data source import isolation regression.
//!
//! Run explicitly with:
//! `powershell -ExecutionPolicy Bypass -File scripts/check-stage2-real-sample-isolation.ps1`

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use app_services::import_analysis::ImportAnalysisMode;
use app_services::import_pipeline::{execute_import_job_with_counts, ImportJobOptions};
use app_services::source_db::{self, GlobalFileId};
use domain::{CaseId, CaseMeta, DataSourceId, DataSourcePlatform, FileEntryId};
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo, case_repo::CaseRepo, datasource_repo::DataSourceRepo,
    file_repo::FileRepo, job_repo::JobRepo, timeline_repo::TimelineRepo,
};
use tempfile::TempDir;
use transport::dto::{ReleaseGateStatusDto, ViewerRangeRequestDto};

const WINDOWS_E01_ENV: &str = "FORENSICS_STAGE2_WINDOWS_E01";
const LINUX_E01_ENV: &str = "FORENSICS_STAGE2_LINUX_E01";

#[test]
#[ignore = "requires real Windows/Linux E01 fixtures and performs full serial imports"]
fn real_samples_import_into_isolated_source_databases_serially() {
    run_real_samples(false);
}

#[test]
#[ignore = "requires real Windows/Linux E01 fixtures and performs full serial imports"]
fn real_samples_remain_isolated_when_linux_imports_first() {
    run_real_samples(true);
}

fn run_real_samples(linux_first: bool) {
    let windows_e01 = fixture_path(WINDOWS_E01_ENV);
    let linux_e01 = fixture_path(LINUX_E01_ENV);
    assert_fixture_exists(&windows_e01);
    assert_fixture_exists(&linux_e01);

    let temp = TempDir::new().expect("temp case root");
    let case_root = temp.path().join("dual-source-isolation-case");
    std::fs::create_dir_all(&case_root).expect("create case root");

    let case_id = CaseId("dual-source-isolation".to_string());
    let case_conn = persistence_sqlite::connection::open_or_create(&case_root.join("app.db"))
        .expect("open app db");
    persistence_sqlite::runner::run_all(&case_conn).expect("run app migrations");
    CaseRepo::new(&case_conn)
        .create(&CaseMeta {
            id: case_id.clone(),
            name: "Dual Source Isolation".to_string(),
            number: None,
            examiner: Some("real-sample-regression".to_string()),
            notes: Some(
                "Serial Windows + Linux E01 import isolation regression fixture".to_string(),
            ),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .expect("insert case");

    let import_windows = || {
        import_fixture_serially(
            &case_conn,
            &case_root,
            &case_id,
            &windows_e01,
            DataSourcePlatform::Windows,
            "windows-e01",
        )
    };
    let import_linux = || {
        import_fixture_serially(
            &case_conn,
            &case_root,
            &case_id,
            &linux_e01,
            DataSourcePlatform::Linux,
            "linux-e01",
        )
    };
    let (windows_ds, linux_ds) = if linux_first {
        let linux_ds = import_linux();
        assert_source_storage(&case_conn, &case_root, &linux_ds, "linux");
        assert_app_db_does_not_store_file_tree(&case_conn);
        let windows_ds = import_windows();
        (windows_ds, linux_ds)
    } else {
        let windows_ds = import_windows();
        assert_source_storage(&case_conn, &case_root, &windows_ds, "windows");
        assert_app_db_does_not_store_file_tree(&case_conn);
        let linux_ds = import_linux();
        (windows_ds, linux_ds)
    };
    assert_source_storage(&case_conn, &case_root, &windows_ds, "windows");
    assert_source_storage(&case_conn, &case_root, &linux_ds, "linux");
    assert_app_db_does_not_store_file_tree(&case_conn);
    run_platform_analysis(&case_conn, &case_root, &case_id, &windows_ds);
    run_platform_analysis(&case_conn, &case_root, &case_id, &linux_ds);

    assert_ne!(windows_ds, linux_ds, "data sources must remain distinct");
    assert_case_data_sources(&case_conn, &case_root, &case_id, &windows_ds, &linux_ds);
    assert_file_tree_aggregates_source_scoped_roots(
        &case_conn,
        &case_root,
        &case_id,
        &windows_ds,
        &linux_ds,
    );
    assert_preview_smoke(&case_conn, &case_root, &case_id, &windows_ds);
    assert_preview_smoke(&case_conn, &case_root, &case_id, &linux_ds);
    assert_source_scoped_analysis_ids(&case_conn, &case_root, &case_id, &windows_ds, &linux_ds);
}

fn run_platform_analysis(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) {
    app_services::analysis_service::run_source_analysis_extraction(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        &[],
    )
    .expect("run platform-scoped real-sample analysis");
}

fn import_fixture_serially(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    source_path: &Path,
    platform: DataSourcePlatform,
    profile: &str,
) -> DataSourceId {
    let before = data_source_ids(case_conn, case_id);
    let source_path = source_path
        .to_str()
        .expect("real-sample fixture path must be valid UTF-8");
    let config = app_services::import_precheck::prepare_import_source_config(
        source_path,
        platform,
        Some(profile.to_string()),
    )
    .expect("precheck");
    let job_id = JobRepo::new(case_conn)
        .create(&case_id.0, "import")
        .expect("create import job");
    let cancel_token = Arc::new(AtomicBool::new(false));
    let options = ImportJobOptions {
        event_sink: None,
        cancel_token: &cancel_token,
        max_import_workers: Some(1),
        max_analysis_workers: Some(1),
        analysis_mode: ImportAnalysisMode::BudgetedContent,
    };

    execute_import_job_with_counts(case_conn, case_id, case_root, config, &job_id, options)
        .expect("serial import should complete");

    let after = DataSourceRepo::new(case_conn)
        .find_by_case(case_id)
        .expect("list data sources");
    let created = after
        .into_iter()
        .filter(|source| !before.contains(&source.id.0))
        .collect::<Vec<_>>();
    assert_eq!(
        created.len(),
        1,
        "each serial import should register exactly one new data source"
    );
    created[0].id.clone()
}

fn fixture_path(variable: &str) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {variable} before running the ignored real-sample gate"))
}

fn assert_fixture_exists(path: &Path) {
    assert!(path.exists(), "fixture missing: {}", path.display());
    assert!(path.is_file(), "fixture is not a file: {}", path.display());
}

fn data_source_ids(case_conn: &rusqlite::Connection, case_id: &CaseId) -> HashSet<String> {
    DataSourceRepo::new(case_conn)
        .find_by_case(case_id)
        .expect("list data sources")
        .into_iter()
        .map(|source| source.id.0)
        .collect()
}

fn assert_source_storage(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    data_source_id: &DataSourceId,
    expected_platform: &str,
) {
    let storage = DataSourceRepo::new(case_conn)
        .find_storage(data_source_id)
        .expect("load source storage")
        .expect("source storage row");
    assert_eq!(storage.storage_model, "source_db");
    assert_eq!(storage.platform, expected_platform);
    assert_eq!(storage.import_state, "ready");
    assert_eq!(
        storage.source_db_rel_path.as_deref(),
        Some(format!("sources/{}/source.db", data_source_id.0).as_str())
    );

    let source_db = source_db::source_db_path(case_root, data_source_id);
    assert!(
        source_db.exists(),
        "source DB missing: {}",
        source_db.display()
    );

    let source_conn = source_db::open_registered_source_db(case_conn, case_root, data_source_id)
        .expect("open registered source db");
    let local_sources = DataSourceRepo::new(&source_conn)
        .find_by_case(&CaseId("dual-source-isolation".to_string()))
        .expect("load source-local metadata");
    assert_eq!(local_sources.len(), 1);
    assert_eq!(local_sources[0].id, *data_source_id);

    let file_count = FileRepo::new(&source_conn)
        .count_by_data_source(data_source_id)
        .expect("count source files");
    assert!(file_count > 0, "source DB has no file entries");
}

fn assert_app_db_does_not_store_file_tree(case_conn: &rusqlite::Connection) {
    let app_file_entries: i64 = case_conn
        .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
        .expect("count app file entries");
    assert_eq!(
        app_file_entries, 0,
        "app.db must remain a control database; file tree rows belong in source.db"
    );
}

fn assert_case_data_sources(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    windows_ds: &DataSourceId,
    linux_ds: &DataSourceId,
) {
    let summaries =
        app_services::file_service::get_data_sources_for_case(case_conn, case_root, case_id)
            .expect("get case data source summaries");
    assert_eq!(summaries.len(), 2);

    let windows = summaries
        .iter()
        .find(|source| source.id == windows_ds.0)
        .expect("windows source summary");
    assert_eq!(windows.platform, "windows");
    assert_eq!(windows.storage_model.as_deref(), Some("source_db"));
    assert!(windows.file_count.unwrap_or_default() > 0);
    assert_partition_family(
        windows,
        &["ntfs", "fat", "exfat"],
        &["xfs", "lvm", "ext2", "ext3", "ext4", "btrfs"],
    );

    let linux = summaries
        .iter()
        .find(|source| source.id == linux_ds.0)
        .expect("linux source summary");
    assert_eq!(linux.platform, "linux");
    assert_eq!(linux.storage_model.as_deref(), Some("source_db"));
    assert!(linux.file_count.unwrap_or_default() > 0);
    assert_partition_family(linux, &["xfs", "lvm", "ext2", "ext3", "ext4", "btrfs"], &[]);
}

fn assert_partition_family(
    source: &transport::dto::DataSourceSummaryDto,
    required: &[&str],
    forbidden: &[&str],
) {
    let descriptors = source
        .partitions
        .iter()
        .map(|partition| {
            format!(
                "{} {} {}",
                partition.kind_label,
                partition.filesystem.as_deref().unwrap_or_default(),
                partition.name
            )
            .to_ascii_lowercase()
        })
        .collect::<Vec<_>>();
    assert!(
        descriptors
            .iter()
            .any(|value| required.iter().any(|needle| value.contains(needle))),
        "source {} lacks its expected partition family: {:?}",
        source.id,
        descriptors
    );
    assert!(
        descriptors
            .iter()
            .all(|value| forbidden.iter().all(|needle| !value.contains(needle))),
        "source {} contains a cross-platform partition classification: {:?}",
        source.id,
        descriptors
    );
}

fn assert_file_tree_aggregates_source_scoped_roots(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    windows_ds: &DataSourceId,
    linux_ds: &DataSourceId,
) {
    let roots =
        app_services::file_service::get_file_tree_for_case(case_conn, case_root, case_id, false)
            .expect("get file tree");
    assert!(
        !roots.is_empty(),
        "case file tree should expose source roots"
    );
    assert!(
        roots.iter().all(|node| node.id.starts_with("ds:")),
        "all root ids must be source-scoped"
    );
    assert!(
        roots
            .iter()
            .any(|node| node.id.starts_with(&format!("ds:{}:", windows_ds.0))),
        "missing Windows source roots"
    );
    assert!(
        roots
            .iter()
            .any(|node| node.id.starts_with(&format!("ds:{}:", linux_ds.0))),
        "missing Linux source roots"
    );
}

fn assert_preview_smoke(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
) {
    let source_conn = source_db::open_registered_source_db(case_conn, case_root, data_source_id)
        .expect("open source db");
    let candidates = previewable_file_ids(&source_conn, data_source_id);
    assert!(
        !candidates.is_empty(),
        "source DB has no positive-size file candidates"
    );

    let mut failures = Vec::new();
    for local_file_id in candidates {
        let global_file_id = GlobalFileId::new(data_source_id.clone(), local_file_id.clone())
            .encode()
            .0;
        let handle = match app_services::file_service::open_file_handle_for_case(
            case_conn,
            case_root,
            case_id,
            &global_file_id,
        ) {
            Ok(handle) => handle,
            Err(error) => {
                failures.push(format!("{}: open: {error}", local_file_id.0));
                continue;
            }
        };
        assert!(handle.handle_id.starts_with("file:ds:"));

        let mut request = ViewerRangeRequestDto {
            handle_id: handle.handle_id,
            offset: 0,
            length: 16,
        };
        request.validate().expect("viewer range request is valid");
        match app_services::file_service::read_file_range_for_source_case(
            case_conn, case_root, case_id, &request,
        ) {
            Ok(response) if response.raw_bytes.is_some() || !response.lines.is_empty() => return,
            Ok(_) => failures.push(format!("{}: empty preview response", local_file_id.0)),
            Err(error) => failures.push(format!("{}: range: {error}", local_file_id.0)),
        }
    }

    panic!(
        "no previewable file resolved for source {}; first failures: {}",
        data_source_id.0,
        failures.into_iter().take(8).collect::<Vec<_>>().join(" | ")
    );
}

fn previewable_file_ids(
    source_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
) -> Vec<FileEntryId> {
    let mut stmt = source_conn
        .prepare(
            "SELECT f.id
             FROM file_entries AS f
             WHERE f.data_source_id = ?1
               AND lower(f.entry_type) = 'file'
               AND COALESCE(f.size, 0) > 0
               AND (
                   f.parent_id IS NULL
                   OR EXISTS (
                       SELECT 1
                       FROM file_entries AS p
                       WHERE p.id = f.parent_id
                         AND p.data_source_id = f.data_source_id
                   )
               )
             ORDER BY
               CASE WHEN f.parent_id IS NULL THEN 1 ELSE 0 END,
               COALESCE(f.size, 0) ASC,
               f.path ASC
             LIMIT 128",
        )
        .expect("prepare preview candidate query");
    let rows = stmt
        .query_map([data_source_id.0.as_str()], |row| {
            Ok(FileEntryId(row.get::<_, String>(0)?))
        })
        .expect("query preview candidates");
    rows.collect::<Result<Vec<_>, _>>()
        .expect("collect preview candidates")
}

fn assert_source_scoped_analysis_ids(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    windows_ds: &DataSourceId,
    linux_ds: &DataSourceId,
) {
    let artifacts = app_services::artifact_service::get_artifact_rows_for_case(
        case_conn, case_root, case_id, None,
    )
    .expect("get artifact rows");
    assert!(
        artifacts
            .iter()
            .all(|artifact| artifact.id.starts_with("ds:")),
        "artifact rows returned from case-level APIs must be source-scoped"
    );
    assert!(
        artifacts.iter().all(|artifact| artifact
            .source_object_id
            .as_deref()
            .is_none_or(|source_object_id| same_source_prefix(&artifact.id, source_object_id))),
        "artifact source-object references must remain within their owning data source"
    );

    let timeline = app_services::timeline_service::query_timeline_for_case(
        case_conn, case_root, case_id, 0, 100,
    )
    .expect("get timeline rows");
    assert!(
        timeline
            .items
            .iter()
            .all(|event| event.id.starts_with("ds:")
                && (event.source_object_id.is_empty()
                    || event.source_object_id.starts_with("ds:"))),
        "timeline rows returned from case-level APIs must be source-scoped"
    );
    assert!(
        timeline
            .items
            .iter()
            .all(|event| event.source_object_id.is_empty()
                || same_source_prefix(&event.id, &event.source_object_id)),
        "timeline source-object references must remain within their owning data source"
    );

    let windows = source_analysis_baseline(case_conn, case_root, windows_ds);
    let linux = source_analysis_baseline(case_conn, case_root, linux_ds);
    assert_source_contributes_analysis("Windows", &windows);
    assert_source_contributes_analysis("Linux", &linux);
    assert_source_analysis_is_visible(case_conn, case_root, case_id, windows_ds, &windows);
    assert_source_analysis_is_visible(case_conn, case_root, case_id, linux_ds, &linux);
    assert_platform_integrity_gate(case_conn, case_root, case_id, &windows, &linux);

    let correlation =
        app_services::correlation::get_correlation_snapshot_for_case(case_conn, case_root, case_id)
            .expect("get correlation snapshot");
    assert!(
        correlation
            .leads
            .iter()
            .all(|lead| lead.primary_file_id.starts_with("ds:")),
        "correlation lead file ids must be source-scoped"
    );
    let prefixes = [windows_ds, linux_ds].map(|data_source_id| format!("ds:{}:", data_source_id.0));
    assert!(artifacts.iter().all(|artifact| prefixes
        .iter()
        .any(|prefix| artifact.id.starts_with(prefix))));
    assert!(timeline
        .items
        .iter()
        .all(|event| prefixes.iter().any(|prefix| event.id.starts_with(prefix))));
}

struct SourceAnalysisBaseline {
    artifact_families: BTreeMap<String, u64>,
    artifact_local_id: String,
    artifact_family: String,
    timeline_types: BTreeSet<String>,
    timeline_local_id: String,
    timeline_type: String,
}

fn source_analysis_baseline(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    data_source_id: &DataSourceId,
) -> SourceAnalysisBaseline {
    let source_conn = source_db::open_registered_source_db(case_conn, case_root, data_source_id)
        .expect("open source database for analysis attribution");
    let artifact_repo = ArtifactRepo::new(&source_conn);
    let artifact_families = artifact_repo
        .count_by_family()
        .expect("count source artifact families")
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    let artifact = artifact_repo
        .list_by_family(None)
        .expect("list source artifacts")
        .into_iter()
        .next()
        .expect("real-sample baseline must contain a structured artifact");

    let timeline_repo = TimelineRepo::new(&source_conn);
    let timeline_types = source_timeline_types(&source_conn);
    let timeline_event = timeline_repo
        .query(0, 1)
        .expect("list source timeline events")
        .into_iter()
        .next()
        .expect("real-sample baseline must contain a timeline event");

    SourceAnalysisBaseline {
        artifact_families,
        artifact_local_id: artifact.id.0,
        artifact_family: artifact.family,
        timeline_types,
        timeline_local_id: timeline_event.id.0,
        timeline_type: timeline_event.event_type,
    }
}

fn source_timeline_types(source_conn: &rusqlite::Connection) -> BTreeSet<String> {
    let mut statement = source_conn
        .prepare("SELECT DISTINCT event_type FROM timeline_events ORDER BY event_type")
        .expect("prepare source timeline type query");
    statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query source timeline types")
        .collect::<Result<BTreeSet<_>, _>>()
        .expect("collect source timeline types")
}

fn assert_source_contributes_analysis(platform: &str, baseline: &SourceAnalysisBaseline) {
    assert!(
        !baseline.artifact_families.is_empty(),
        "{platform} real-sample baseline must contribute structured artifact families"
    );
    assert!(
        baseline.artifact_families.values().all(|count| *count > 0),
        "{platform} artifact family counts must be positive"
    );
    assert!(
        !baseline.timeline_types.is_empty(),
        "{platform} real-sample baseline must contribute timeline event types"
    );
}

fn assert_source_analysis_is_visible(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    baseline: &SourceAnalysisBaseline,
) {
    let artifact_id =
        source_db::encode_source_scoped_id(data_source_id, &baseline.artifact_local_id);
    let artifact = app_services::artifact_service::get_artifact_row_by_id_for_case(
        case_conn,
        case_root,
        case_id,
        &artifact_id,
    )
    .expect("route source-scoped artifact detail")
    .expect("source-scoped artifact must be investigator-visible");
    assert_eq!(artifact.id, artifact_id);
    assert_eq!(artifact.artifact_type, baseline.artifact_family);
    assert!(artifact
        .source_object_id
        .as_deref()
        .is_none_or(|value| value.starts_with(&format!("ds:{}:", data_source_id.0))));

    let timeline_id =
        source_db::encode_source_scoped_id(data_source_id, &baseline.timeline_local_id);
    let event = app_services::timeline_service::get_timeline_event_by_id_for_case(
        case_conn,
        case_root,
        case_id,
        &timeline_id,
    )
    .expect("route source-scoped timeline detail")
    .expect("source-scoped timeline event must be investigator-visible");
    assert_eq!(event.id, timeline_id);
    assert_eq!(event.event_type, baseline.timeline_type);
    assert!(
        event.source_object_id.is_empty()
            || event
                .source_object_id
                .starts_with(&format!("ds:{}:", data_source_id.0))
    );
}

fn assert_platform_integrity_gate(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    windows: &SourceAnalysisBaseline,
    linux: &SourceAnalysisBaseline,
) {
    let snapshot = app_services::v3_governance_service::get_v3_governance_snapshot_for_case(
        case_conn, case_root, &case_id.0,
    )
    .expect("build source-aware V3 governance snapshot");
    let gate = snapshot
        .v2
        .release_gates
        .iter()
        .find(|gate| gate.gate_id == "source-platform-artifact-integrity")
        .expect("source-platform integrity release gate");
    assert_eq!(
        gate.status,
        ReleaseGateStatusDto::Passed,
        "source-platform integrity gate failed: {}",
        gate.evidence
    );
    assert_eq!(
        snapshot.platform_coverage.cross_platform_artifact_families,
        0
    );
    assert!(snapshot
        .platform_coverage
        .cross_platform_families
        .is_empty());

    let observed_windows = windows
        .artifact_families
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let observed_linux = linux
        .artifact_families
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let governed_windows = snapshot
        .platform_coverage
        .windows_families
        .into_iter()
        .collect::<BTreeSet<_>>();
    let governed_linux = snapshot
        .platform_coverage
        .linux_families
        .into_iter()
        .collect::<BTreeSet<_>>();

    assert_eq!(observed_windows, governed_windows);
    assert_eq!(observed_linux, governed_linux);
    assert!(observed_windows.is_disjoint(&observed_linux));
}

fn same_source_prefix(record_id: &str, source_object_id: &str) -> bool {
    let record_source = record_id
        .strip_prefix("ds:")
        .and_then(|value| value.split_once(':'))
        .map(|(source, _)| source);
    let object_source = source_object_id
        .strip_prefix("ds:")
        .and_then(|value| value.split_once(':'))
        .map(|(source, _)| source);
    record_source.is_some() && record_source == object_source
}
