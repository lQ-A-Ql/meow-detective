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
