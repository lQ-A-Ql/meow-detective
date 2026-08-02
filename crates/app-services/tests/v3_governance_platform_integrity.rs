use std::collections::BTreeMap;

use app_services::active_case::ActiveCase;
use app_services::{case_service, source_db, v3_governance_service};
use domain::{
    Artifact, ArtifactId, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance,
};
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
};
use transport::dto::{ReleaseGateEntryDto, ReleaseGateStatusDto, V3GovernanceSnapshotDto};

#[test]
fn normal_windows_and_linux_sources_pass_platform_integrity() {
    let snapshot = governance_snapshot(
        "governance-normal-dual-source",
        &[
            (
                "windows-normal",
                "windows",
                &["Prefetch", "BrowserHistory", "EmailMessage"],
            ),
            (
                "linux-normal",
                "linux",
                &["LinuxJournal", "LinuxMysqlFinding"],
            ),
        ],
    );

    assert_eq!(
        integrity_gate(&snapshot).status,
        ReleaseGateStatusDto::Passed
    );
    assert_eq!(snapshot.platform_coverage.total_families, 5);
    assert_eq!(
        snapshot.platform_coverage.cross_platform_artifact_families,
        0
    );
    assert!(snapshot
        .platform_coverage
        .windows_families
        .contains(&"BrowserHistory".to_string()));
    assert!(snapshot
        .platform_coverage
        .windows_families
        .contains(&"EmailMessage".to_string()));
    assert!(snapshot
        .platform_coverage
        .linux_families
        .contains(&"LinuxJournal".to_string()));
}

#[test]
fn windows_source_with_linux_artifact_is_blocked_and_not_counted_as_linux_coverage() {
    let snapshot = governance_snapshot(
        "governance-windows-pollution",
        &[("windows-polluted", "windows", &["Prefetch", "LinuxJournal"])],
    );

    let gate = integrity_gate(&snapshot);
    assert_eq!(gate.status, ReleaseGateStatusDto::Blocked);
    assert!(gate.evidence.contains("sourceId=windows-polluted"));
    assert!(gate.evidence.contains("persistedPlatform=windows"));
    assert!(gate.evidence.contains("family=LinuxJournal"));
    assert!(gate.evidence.contains("expectedPlatform=linux"));
    assert!(!snapshot
        .platform_coverage
        .linux_families
        .contains(&"LinuxJournal".to_string()));
    assert_eq!(snapshot.platform_coverage.total_families, 2);
    assert!(snapshot
        .v2
        .release_scorecard
        .blockers
        .iter()
        .any(|blocker| blocker.contains("Data-source artifact platform integrity")));
}

#[test]
fn linux_source_with_windows_browser_artifact_is_blocked() {
    let snapshot = governance_snapshot(
        "governance-linux-pollution",
        &[(
            "linux-polluted",
            "linux",
            &["LinuxJournal", "BrowserHistory", "EmailMessage"],
        )],
    );

    let gate = integrity_gate(&snapshot);
    assert_eq!(gate.status, ReleaseGateStatusDto::Blocked);
    assert!(gate.evidence.contains("sourceId=linux-polluted"));
    assert!(gate.evidence.contains("persistedPlatform=linux"));
    assert!(gate.evidence.contains("family=BrowserHistory"));
    assert!(gate.evidence.contains("expectedPlatform=windows"));
    assert!(gate.evidence.contains("family=EmailMessage"));
    assert!(!snapshot
        .platform_coverage
        .windows_families
        .contains(&"BrowserHistory".to_string()));
    assert_eq!(snapshot.platform_coverage.total_families, 3);
}

fn governance_snapshot(
    case_name: &str,
    sources: &[(&str, &str, &[&str])],
) -> V3GovernanceSnapshotDto {
    let parent = tempfile::TempDir::new().expect("create case parent");
    let active = case_service::create_case(parent.path(), case_name, Some("stage2-governance"))
        .expect("create case");
    let case_conn = active.connection().expect("lock case database");

    for (source_id, platform, families) in sources {
        register_source(&active, &case_conn, source_id, platform, families);
    }

    v3_governance_service::get_v3_governance_snapshot_for_case(
        &case_conn,
        &active.case_root,
        &active.meta.id.0,
    )
    .expect("build source-aware governance snapshot")
}

fn register_source(
    active: &ActiveCase,
    case_conn: &rusqlite::Connection,
    source_id: &str,
    platform: &str,
    families: &[&str],
) {
    let source = DataSource {
        id: DataSourceId(source_id.to_string()),
        name: source_id.to_string(),
        kind: DataSourceKind::E01,
        source_path: active.case_root.join(format!("{source_id}.E01")),
        imported_at: chrono::Utc::now(),
        provenance: DataSourceProvenance::unknown(),
    };
    let mut storage = DataSourceStorage::source_db(source_id, Some(platform), None);
    storage.import_state = "ready".to_string();
    DataSourceRepo::new(case_conn)
        .insert_with_storage(&active.meta.id, &source, &storage)
        .expect("register ready source");

    let source_conn =
        source_db::open_source_db(&active.case_root, &source.id).expect("create source database");
    DataSourceRepo::new(&source_conn)
        .upsert_source_local_metadata(&active.meta.id, &source)
        .expect("store source-local metadata");
    let artifacts = families
        .iter()
        .enumerate()
        .map(|(index, family)| Artifact {
            id: ArtifactId(format!("{source_id}-artifact-{index}")),
            family: (*family).to_string(),
            title: format!("{family} fixture"),
            summary: "platform integrity fixture".to_string(),
            source_object_id: None,
            extractor_id: Some("platform-integrity-test".to_string()),
            extractor_version: Some("1.0.0".to_string()),
            confidence: Some(1.0),
            source_attribution: Some(source_id.to_string()),
            created_at: chrono::Utc::now(),
            attrs: BTreeMap::new(),
        })
        .collect::<Vec<_>>();
    ArtifactRepo::new(&source_conn)
        .insert_batch(&artifacts, &active.meta.id.0, source_id)
        .expect("insert source artifacts");
}

fn integrity_gate(snapshot: &V3GovernanceSnapshotDto) -> &ReleaseGateEntryDto {
    snapshot
        .v2
        .release_gates
        .iter()
        .find(|gate| gate.gate_id == "source-platform-artifact-integrity")
        .expect("platform integrity release gate")
}
