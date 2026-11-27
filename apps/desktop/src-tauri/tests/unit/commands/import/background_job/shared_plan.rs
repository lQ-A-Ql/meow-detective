use std::sync::{atomic::AtomicBool, Arc};

use app_services::{case_service, cluster_service, import_analysis::ImportAnalysisMode};
use persistence_sqlite::repositories::job_repo::JobRepo;

use crate::commands::import::background_job::{
    cluster_members::import_evidence_set_members, BackgroundLinuxEvidenceSetImportJob,
};

#[test]
fn shared_plan_failure_and_cancellation_close_collection_jobs() {
    for cancelled in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let input = root.path().join("images");
        std::fs::create_dir(&input).unwrap();
        for index in 0..2 {
            std::fs::File::create(input.join(format!("member-{index}.raw")))
                .unwrap()
                .set_len(512)
                .unwrap();
        }
        let plan = cluster_service::plan_linux_evidence_set_import(&input, None).unwrap();
        let collection_plan = plan
            .prepare_shared_sources(&AtomicBool::new(false))
            .unwrap();
        let case = case_service::create_case(root.path(), "Collection admission", None).unwrap();
        let connection = case.connection().unwrap();
        cluster_service::register_linux_evidence_set_import(&connection, &case.meta.id, &plan)
            .unwrap();
        let repo = JobRepo::new(&connection);
        let job_id = repo
            .create(&case.meta.id.0, "Collection admission")
            .unwrap();
        repo.start(&job_id).unwrap();
        let set_id = plan.import_set_id.clone();
        if !cancelled {
            std::fs::remove_file(&plan.members[0].source_path).unwrap();
        }
        let job = BackgroundLinuxEvidenceSetImportJob {
            db_path: case.db_path(),
            case_id: case.meta.id.clone(),
            case_root: case.case_root.clone(),
            plan,
            job_id: job_id.clone(),
            max_import_workers: Some(1),
            max_analysis_workers: Some(1),
            analysis_mode: ImportAnalysisMode::MetadataOnly,
        };
        let result = import_evidence_set_members(
            &connection,
            &repo,
            &job,
            None,
            &Arc::new(AtomicBool::new(cancelled)),
            collection_plan,
            None,
        );
        if cancelled {
            assert!(result.unwrap().is_none());
        } else {
            assert!(result.is_err());
        }
        let expected = if cancelled { "cancelled" } else { "failed" };
        assert_eq!(repo.find_by_id(&job_id).unwrap().unwrap().status, expected);
        let state: String = connection
            .query_row(
                "SELECT import_state FROM linux_import_sets WHERE id=?1",
                [&set_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(state, expected);
    }
}

#[test]
fn collection_publication_rejects_unpublished_members_and_cancellation() {
    use std::io::{Read, Seek, SeekFrom, Write};
    for cancelled in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let input = root.path().join("images");
        std::fs::create_dir(&input).unwrap();
        let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../testdata/fixtures/public-small/e01/tiny.E01");
        let mut reader = app_services::datasource_service::open_evidence_reader(
            &fixture,
            &domain::DataSourceKind::E01,
        )
        .unwrap();
        let mut content = Vec::new();
        reader.read_to_end(&mut content).unwrap();
        for index in 0..2 {
            std::fs::write(input.join(format!("source-{index}.raw")), &content).unwrap();
        }
        let plan = cluster_service::plan_linux_evidence_set_import(&input, None).unwrap();
        let configs = plan.member_import_configs(&AtomicBool::new(false)).unwrap();
        let shared = configs[0].collection_plan.as_ref().unwrap().clone();
        let case = case_service::create_case(root.path(), "Collection verification", None).unwrap();
        let connection = case.connection().unwrap();
        cluster_service::register_linux_evidence_set_import(&connection, &case.meta.id, &plan)
            .unwrap();
        let repo = JobRepo::new(&connection);
        let job_id = repo
            .create(&case.meta.id.0, "Collection verification")
            .unwrap();
        repo.start(&job_id).unwrap();
        let path = &configs[0].source_path;
        let modified = path.metadata().unwrap().modified().unwrap();
        let mut changed = std::fs::File::options()
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        changed.seek(SeekFrom::End(-1)).unwrap();
        let mut byte = [0];
        changed.read_exact(&mut byte).unwrap();
        changed.seek(SeekFrom::End(-1)).unwrap();
        changed.write_all(&[byte[0] ^ 0x80]).unwrap();
        changed.sync_all().unwrap();
        changed
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        drop(changed);
        let job = BackgroundLinuxEvidenceSetImportJob {
            db_path: case.db_path(),
            case_id: case.meta.id.clone(),
            case_root: case.case_root.clone(),
            plan,
            job_id: job_id.clone(),
            max_import_workers: Some(1),
            max_analysis_workers: Some(1),
            analysis_mode: ImportAnalysisMode::MetadataOnly,
        };
        let cancel = Arc::new(AtomicBool::new(cancelled));
        let coordinator =
            crate::commands::import::background_job::cluster_members::types::MemberCoordinator {
                connection: &connection,
                job_repo: &repo,
                job: &job,
                app: None,
                cancel_token: &cancel,
                total_members: 2,
                task_manager: None,
            };
        let summary =
            crate::commands::import::background_job::types::EvidenceSetImportSummary::new();
        let result = crate::commands::import::background_job::cluster_members::verify_collection(
            &coordinator,
            Some(&shared),
            &summary,
        );
        if cancelled {
            assert!(!result.unwrap());
        } else {
            assert!(result.is_err());
        }
        assert_eq!(
            repo.find_by_id(&job_id).unwrap().unwrap().status,
            if cancelled { "cancelled" } else { "failed" }
        );
    }
}
