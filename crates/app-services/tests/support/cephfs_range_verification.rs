use super::*;
use domain::structure::{ReadIntent, ReadPriority, StructureAttemptRef};
use source_reconstruction::structure_analysis::{
    run_structure_analysis, StructureAnalysisRequest, CEPHFS_ADAPTER_ID,
};

#[test]
fn cephfs_structure_execution_verifies_actual_file_bytes_and_records_complete_coverage() {
    let fixture = fixture();
    let source = materialize(&fixture, true);
    let request = StructureAnalysisRequest {
        case_id: fixture.case_id.clone(),
        source_ids: vec![source.data_source.id.0],
        intent: ReadIntent::MetadataInspection,
        priority: ReadPriority::Interactive,
        consumers: vec!["cephfs-range-test".into()],
        required_features: Default::default(),
        attempt: Some(StructureAttemptRef {
            attempt_id: "cephfs-verification".into(),
            owner: "range-owner".into(),
        }),
    };
    let run = run_structure_analysis(
        &fixture.case_conn,
        fixture.root.path(),
        &request,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(run.work_results.len(), 1);
    assert!(run
        .plan
        .work_items
        .iter()
        .all(|item| item.adapter_id == CEPHFS_ADAPTER_ID));
    let result = &run.work_results[0];
    assert_eq!(result.bytes_read, FILE_CONTENT.len() as u64);
    assert_eq!(
        result.receipt.output_sha256,
        hex::encode(sha2::Sha256::digest(FILE_CONTENT))
    );
    assert_eq!(result.receipt.dependency_revisions.len(), 4);
    assert_eq!(result.receipt.attempt.owner, "range-owner");
    assert_eq!(
        run.verification_stats.verified_bytes,
        FILE_CONTENT.len() as u64
    );
    let mut deep = request;
    deep.intent = ReadIntent::ContentRead;
    assert!(run_structure_analysis(
        &fixture.case_conn,
        fixture.root.path(),
        &deep,
        &std::sync::atomic::AtomicBool::new(false)
    )
    .is_err());
}

#[test]
fn cached_cephfs_file_reader_rejects_a_changed_parent_after_its_first_success() {
    let fixture = fixture();
    let source = materialize(&fixture, true);
    let control = app_services::source_db::open_registered_source_db_read_only(
        &fixture.case_conn,
        fixture.root.path(),
        &source.data_source.id,
    )
    .unwrap();
    let mut context = app_services::file_service::SourceReadContext::new(
        &control,
        &fixture.case_conn,
        fixture.root.path(),
        &fixture.case_id,
        &source.data_source.id,
    );
    let file_id = domain::FileEntryId(FILE_ENTRY_ID.into());
    assert_eq!(
        context
            .read_extraction_chunk_by_id(&file_id, 0, 4096)
            .unwrap(),
        FILE_CONTENT
    );
    let revisions = SourceRevisionRepo::new(&fixture.case_conn);
    let (_, mut parent) = revisions.current(&fixture.case_id.0, "osd-0").unwrap();
    parent.metadata_identity = Some("changed-parent".into());
    parent.metadata_identity_status = domain::structure::DigestVerification::Recorded;
    revisions
        .bind(&fixture.case_id.0, "osd-0", &parent, true)
        .unwrap();
    assert!(context
        .read_extraction_chunk_by_id(&file_id, 0, 4096)
        .is_err());
}
