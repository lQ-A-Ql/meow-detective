//! End-to-end Kubernetes E01 regression against the private four-member sample.
//!
//! This test intentionally stays ignored and requires FORENSICS_K8S_CLUSTER_ROOT.
//! It validates the production import path, automatic Linux post-import artifact
//! analysis, topology projection, and the evidence facts consumed by the UI.

use app_services::{
    cluster_service::{
        get_linux_evidence_set_summary, plan_linux_evidence_set_import,
        project_import_set_topology, register_linux_evidence_set_import,
        update_linux_evidence_set_import_state,
    },
    import_analysis::ImportAnalysisMode,
    import_pipeline::{execute_import_job_with_counts, ImportJobOptions},
};
use persistence_sqlite::repositories::job_repo::JobRepo;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[test]
#[ignore = "requires FORENSICS_K8S_CLUSTER_ROOT external Kubernetes E01 sample"]
fn real_kubernetes_e01_import_matches_external_identity_and_auto_analysis() {
    let root = PathBuf::from(
        std::env::var_os("FORENSICS_K8S_CLUSTER_ROOT").expect("set FORENSICS_K8S_CLUSTER_ROOT"),
    );
    let plan = plan_linux_evidence_set_import(&root, Some("k8s-external-regression".to_string()))
        .expect("plan sample");
    assert_eq!(
        plan.members.len(),
        4,
        "external sample has four E01 members"
    );

    let retained_root = std::env::var_os("FORENSICS_K8S_CASE_OUTPUT_ROOT").map(PathBuf::from);
    let temp = retained_root
        .is_none()
        .then(|| tempfile::TempDir::new().expect("case temp"));
    let case_parent = retained_root
        .as_deref()
        .unwrap_or_else(|| temp.as_ref().expect("temp root").path());
    let active = app_services::case_service::create_case(
        case_parent,
        "k8s-external-regression",
        Some("regression"),
    )
    .expect("create case");
    let case_id = active.meta.id.clone();
    let conn = app_services::connection::open_case_db(&active.db_path()).expect("open case DB");
    register_linux_evidence_set_import(&conn, &case_id, &plan).expect("register evidence set");

    let member_limit = std::env::var("FORENSICS_K8S_MEMBER_LIMIT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(usize::MAX);
    for config in plan.member_import_configs().into_iter().take(member_limit) {
        let job_id = JobRepo::new(&conn)
            .create(&case_id.0, "k8s sample member import")
            .expect("create member job");
        let cancel_token = Arc::new(AtomicBool::new(false));
        let (_message, counts) = execute_import_job_with_counts(
            &conn,
            &case_id,
            &active.case_root,
            config,
            &job_id,
            ImportJobOptions {
                event_sink: None,
                cancel_token: &cancel_token,
                max_import_workers: Some(1),
                max_analysis_workers: Some(1),
                analysis_mode: ImportAnalysisMode::MetadataOnly,
            },
        )
        .expect("production member import");
        assert_eq!(counts.failed_count, 0, "member import should not fail");
    }
    let imported_count = plan.members.len().min(member_limit) as u32;
    update_linux_evidence_set_import_state(
        &conn,
        &plan.import_set_id,
        "ready",
        imported_count,
        0,
        None,
    )
    .expect("mark set ready");
    if imported_count == plan.members.len() as u32 {
        project_import_set_topology(&conn, &active.case_root, &case_id, &plan.import_set_id)
            .expect("project topology");
    }
    drop(conn);

    let conn = app_services::connection::open_case_db(&active.db_path()).expect("reopen case DB");
    let summary =
        get_linux_evidence_set_summary(&conn, &active.case_root, &case_id, &plan.import_set_id)
            .expect("load summary");
    let expected_hosts: &[&str] = if imported_count == 1 {
        &["master"]
    } else {
        &["master", "node1", "node2", "localhost.localdomain"]
    };
    for expected in expected_hosts {
        assert!(
            summary
                .members
                .iter()
                .any(|member| member.hostname.as_deref() == Some(expected)),
            "expected hostname {expected} in summary: {:?}",
            summary
                .members
                .iter()
                .map(|member| (&member.source_name, &member.hostname, &member.diagnostics))
                .collect::<Vec<_>>()
        );
    }
    assert!(
        summary
            .members
            .iter()
            .take(imported_count as usize)
            .all(|member| member
                .operating_system
                .as_deref()
                .is_some_and(|value| value.contains("CentOS") || value.contains("Debian"))),
        "OS summaries: {:?}",
        summary
            .members
            .iter()
            .map(|member| (&member.source_name, &member.operating_system))
            .collect::<Vec<_>>()
    );
    assert!(summary
        .members
        .iter()
        .take(imported_count as usize)
        .all(|member| member
            .os_version
            .as_deref()
            .is_some_and(|value| value == "7" || value == "10" || value == "12")));
    assert!(summary.members.iter().any(|member| member
        .kernel_version
        .as_deref()
        .is_some_and(|value| value.contains("3.10.0-1160"))));
    assert!(summary.members.iter().any(|member| member
        .addresses
        .iter()
        .any(|value| value == "192.168.50.80")));
    assert!(summary
        .members
        .iter()
        .any(|member| member.roles.iter().any(|role| role == "control_plane")));
    assert!(summary
        .members
        .iter()
        .any(|member| member.roles.iter().any(|role| role == "worker")));
    assert!(
        summary
            .members
            .iter()
            .any(|member| !member.services.is_empty()),
        "automatic post-import analysis should expose Kubernetes services"
    );
    assert!(
        summary
            .members
            .iter()
            .any(|member| !member.containers.is_empty()),
        "automatic post-import analysis should expose Docker containers"
    );

    let mut automatic_artifacts = 0i64;
    for member in &summary.members {
        let Some(source_id) = member.data_source_id.as_deref() else {
            continue;
        };
        let source_conn = app_services::source_db::open_registered_source_db_read_only(
            &conn,
            &active.case_root,
            &domain::DataSourceId(source_id.to_string()),
        )
        .expect("open source DB");
        automatic_artifacts += source_conn
            .query_row("SELECT COUNT(*) FROM artifacts", [], |row| {
                row.get::<_, i64>(0)
            })
            .expect("count artifacts");
    }
    assert!(
        automatic_artifacts > 0,
        "Linux artifact analysis should run automatically after image import"
    );
}

#[test]
#[ignore = "requires FORENSICS_K8S_RETAINED_CASE_ROOT generated by the full sample regression"]
fn retained_kubernetes_case_matches_external_facts_and_auto_artifacts() {
    let case_root = PathBuf::from(
        std::env::var_os("FORENSICS_K8S_RETAINED_CASE_ROOT")
            .expect("set FORENSICS_K8S_RETAINED_CASE_ROOT"),
    );
    let conn = app_services::connection::open_case_db(&case_root.join("app.db"))
        .expect("open retained case DB");
    let case_id = domain::CaseId(
        conn.query_row("SELECT id FROM cases ORDER BY id LIMIT 1", [], |row| {
            row.get::<_, String>(0)
        })
        .expect("case id"),
    );
    let import_set_id: String = conn
        .query_row(
            "SELECT id FROM linux_import_sets ORDER BY id LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("import set");
    let summary = get_linux_evidence_set_summary(&conn, &case_root, &case_id, &import_set_id)
        .expect("summary");
    assert_eq!(summary.member_count, 4);
    assert_eq!(summary.ready_count, 4);
    for expected in ["master", "node1", "node2", "localhost.localdomain"] {
        assert!(
            summary
                .members
                .iter()
                .any(|member| member.hostname.as_deref() == Some(expected)),
            "missing {expected}: {:?}",
            summary
                .members
                .iter()
                .map(|member| (&member.source_name, &member.hostname, &member.diagnostics))
                .collect::<Vec<_>>()
        );
    }
    let member = |hostname: &str| {
        summary
            .members
            .iter()
            .find(|member| member.hostname.as_deref() == Some(hostname))
            .expect("expected member")
    };
    let master = member("master");
    assert!(master
        .operating_system
        .as_deref()
        .is_some_and(|value| value.contains("CentOS")));
    assert_eq!(master.os_version.as_deref(), Some("7"));
    assert!(master
        .kernel_version
        .as_deref()
        .is_some_and(|value| value.starts_with("3.10.0-1160")));
    assert!(master
        .addresses
        .iter()
        .any(|value| value == "192.168.50.80"));
    assert!(master.roles.iter().any(|role| role == "control_plane"));
    assert!(master
        .services
        .iter()
        .any(|service| service.contains("kube-apiserver")));
    assert!(!master.containers.is_empty());
    for worker_name in ["node1", "node2"] {
        let worker = member(worker_name);
        assert!(worker
            .operating_system
            .as_deref()
            .is_some_and(|value| value.contains("CentOS")));
        assert_eq!(worker.os_version.as_deref(), Some("7"));
        assert!(worker.roles.iter().any(|role| role == "worker"));
        assert!(worker.services.iter().any(|service| service == "kubelet"));
        assert!(!worker.containers.is_empty());
    }
    let fourth = member("localhost.localdomain");
    assert!(fourth
        .operating_system
        .as_deref()
        .is_some_and(|value| value.contains("CentOS")));
    assert_eq!(fourth.os_version.as_deref(), Some("7"));
    assert!(summary.members.iter().all(|member| member
        .operating_system
        .as_deref()
        .is_some_and(|value| value.contains("CentOS"))));
    assert!(summary
        .members
        .iter()
        .all(|member| member.os_version.as_deref() == Some("7")));
    assert!(summary.members.iter().any(|member| member
        .kernel_version
        .as_deref()
        .is_some_and(|value| value.contains("3.10.0-1160"))));
    assert!(summary.members.iter().any(|member| member
        .addresses
        .iter()
        .any(|value| value == "192.168.50.80")));
    assert!(summary
        .members
        .iter()
        .any(|member| member.roles.iter().any(|role| role == "control_plane")));
    assert!(summary
        .members
        .iter()
        .any(|member| member.roles.iter().any(|role| role == "worker")));
    assert!(summary
        .members
        .iter()
        .any(|member| !member.services.is_empty()));
    assert!(summary
        .members
        .iter()
        .any(|member| !member.containers.is_empty()));
    let mut artifacts = 0i64;
    for member in &summary.members {
        let source_id = member.data_source_id.as_ref().expect("member source");
        let source = app_services::source_db::open_registered_source_db_read_only(
            &conn,
            &case_root,
            &domain::DataSourceId(source_id.clone()),
        )
        .expect("source DB");
        artifacts += source
            .query_row("SELECT COUNT(*) FROM artifacts", [], |row| {
                row.get::<_, i64>(0)
            })
            .expect("artifact count");
    }
    assert!(
        artifacts > 0,
        "automatic Linux artifact analysis should persist artifacts"
    );
}
