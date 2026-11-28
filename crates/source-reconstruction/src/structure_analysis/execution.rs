use std::sync::atomic::AtomicBool;

use domain::structure::StructureWorkItem;
use evidence_access::structure_access::{
    verify_registered_access, RegisteredAccessRequest, BTRFS_FILE_TARGET_PREFIX,
    CEPHFS_FILE_TARGET_PREFIX,
};
use persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo;
use source_kernel::source_runtime::verification::VerificationBatch;

use super::{
    adapter::{BTRFS_ADAPTER_ID, CEPHFS_ADAPTER_ID},
    planner::plan_structure_analysis,
    request::{
        StructureAnalysisError, StructureAnalysisRequest, StructureAnalysisRun,
        StructureWorkResult, StructureWorkStatus,
    },
};

pub fn run_structure_analysis(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    request: &StructureAnalysisRequest,
    cancel: &AtomicBool,
) -> Result<StructureAnalysisRun, StructureAnalysisError> {
    Ok(execute_structure_analysis(conn, case_root, request, cancel)?.report)
}

pub(crate) fn execute_structure_analysis(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    request: &StructureAnalysisRequest,
    cancel: &AtomicBool,
) -> Result<super::execution_permit::VerifiedStructureAnalysis, StructureAnalysisError> {
    let mut plan = plan_structure_analysis(conn, case_root, request)?;
    let batch = if request.intent == domain::structure::ReadIntent::ContentRead {
        VerificationBatch::new_deep(request.attempt.clone())?
    } else {
        VerificationBatch::new(request.attempt.clone())?
    };
    let mut work_results = Vec::with_capacity(plan.work_items.len());
    let mut proofs = Vec::new();
    for item in &plan.work_items {
        check_cancel(cancel)?;
        let proof = execute_work_item(conn, case_root, item, &batch, cancel)?;
        proof.validate()?;
        work_results.push(StructureWorkResult {
            work_item_id: item.id.clone(),
            bytes_read: proof.receipt().bytes_verified,
            status: StructureWorkStatus::Verified,
            receipt: proof.receipt().clone(),
            coverage: domain::structure::CoverageReport {
                target_node: domain::structure::StructureNodeId(item.target_fingerprint.clone()),
                covered: proof.receipt().ranges.clone(),
                complete: proof.receipt().ranges.is_full(),
                missing: uncovered(&proof.receipt().ranges)?,
                notes: Vec::new(),
            },
        });
        proofs.push(proof);
    }
    plan.graph = super::query::get_structure_graph(
        conn,
        case_root,
        &super::request::StructureGraphRequest {
            case_id: request.case_id.clone(),
            source_ids: request.source_ids.clone(),
        },
    )?;
    for proof in &proofs {
        proof.validate()?;
    }
    Ok(super::execution_permit::VerifiedStructureAnalysis {
        report: StructureAnalysisRun {
            plan,
            work_results,
            verification_stats: batch.stats()?,
        },
        permit: super::execution_permit::StructureVerificationPermit { proofs },
    })
}

fn uncovered(
    ranges: &domain::structure::ByteRangeSet,
) -> Result<Vec<domain::structure::ByteRange>, StructureAnalysisError> {
    let mut cursor = 0;
    let mut missing = Vec::new();
    for range in ranges.ranges() {
        if range.start() > cursor {
            missing.push(
                domain::structure::ByteRange::new(cursor, range.start() - cursor)
                    .map_err(|error| StructureAnalysisError::Plan(error.to_string()))?,
            );
        }
        cursor = range.end();
    }
    if cursor < ranges.target_size() {
        missing.push(
            domain::structure::ByteRange::new(cursor, ranges.target_size() - cursor)
                .map_err(|error| StructureAnalysisError::Plan(error.to_string()))?,
        );
    }
    Ok(missing)
}

fn execute_work_item(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    item: &StructureWorkItem,
    batch: &VerificationBatch,
    cancel: &AtomicBool,
) -> Result<
    std::sync::Arc<source_kernel::source_runtime::verification::ValidatedReceipt>,
    StructureAnalysisError,
> {
    let demand = item
        .demand
        .clone()
        .ok_or_else(|| StructureAnalysisError::Plan("work item demand is missing".into()))?;
    let file_id = if matches!(
        item.adapter_id.as_str(),
        CEPHFS_ADAPTER_ID | BTRFS_ADAPTER_ID
    ) {
        Some(domain::FileEntryId(
            item.target_fingerprint
                .strip_prefix(CEPHFS_FILE_TARGET_PREFIX)
                .or_else(|| {
                    item.target_fingerprint
                        .strip_prefix(BTRFS_FILE_TARGET_PREFIX)
                })
                .ok_or_else(|| StructureAnalysisError::Plan("file target is missing".into()))?
                .into(),
        ))
    } else {
        None
    };
    let (_, revision) = SourceRevisionRepo::new(conn).current(
        &item.source_revision.case_id,
        &item.source_revision.source_id,
    )?;
    let expected = (item.intent == domain::structure::ReadIntent::ContentRead && file_id.is_none())
        .then_some(revision.logical_sha256.as_deref())
        .flatten();
    Ok(verify_registered_access(
        conn,
        case_root,
        RegisteredAccessRequest { demand, file_id },
        batch,
        &item.adapter_version,
        expected,
        false,
        cancel,
    )?)
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), StructureAnalysisError> {
    if cancel.load(std::sync::atomic::Ordering::Acquire) {
        return Err(StructureAnalysisError::Cancelled);
    }
    Ok(())
}
