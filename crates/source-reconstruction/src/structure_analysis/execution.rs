use std::io::Read;
use std::sync::atomic::AtomicBool;

use domain::structure::StructureWorkItem;
use evidence_access::structure_access::{
    open_verified_view, plan_registered_mdraid_access, RegisteredMdRaidAccessRequest,
};
use persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo;

use super::{
    adapter::{FILESYSTEM_ADAPTER_ID, MDRAID_ADAPTER_ID},
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
    let plan = plan_structure_analysis(conn, case_root, request)?;
    let revisions = SourceRevisionRepo::new(conn);
    let mut work_results = Vec::with_capacity(plan.work_items.len());
    for item in &plan.work_items {
        check_cancel(cancel)?;
        revisions.require_current(&item.source_revision)?;
        let result = execute_work_item(conn, case_root, item, cancel)?;
        revisions.require_current(&item.source_revision)?;
        work_results.push(result);
    }
    Ok(StructureAnalysisRun { plan, work_results })
}

fn execute_work_item(
    conn: &rusqlite::Connection,
    case_root: &std::path::Path,
    item: &StructureWorkItem,
    cancel: &AtomicBool,
) -> Result<StructureWorkResult, StructureAnalysisError> {
    if item.adapter_id != MDRAID_ADAPTER_ID && item.adapter_id != FILESYSTEM_ADAPTER_ID {
        return Err(StructureAnalysisError::UnsupportedAdapter(
            item.adapter_id.clone(),
        ));
    }
    let demand = item
        .demand
        .as_ref()
        .ok_or_else(|| StructureAnalysisError::Plan("work item demand is missing".into()))?;
    let access = plan_registered_mdraid_access(
        conn,
        case_root,
        RegisteredMdRaidAccessRequest {
            source_id: item.source_revision.source_id.clone(),
            ranges: Some(item.ranges.ranges().to_vec()),
            intent: item.intent,
            priority: item.priority,
            deadline_unix_ms: demand.deadline_unix_ms,
            reorderable: demand.reorderable,
            consumers: item.consumers.clone(),
            verification: demand.verification,
            attempt: demand.attempt.clone(),
        },
        cancel,
    )?;
    let mut view = open_verified_view(access, cancel)?;
    let mut bytes_read = 0u64;
    let mut buffer = [0u8; 4096];
    while bytes_read < 4096 {
        check_cancel(cancel)?;
        let remaining = usize::try_from(4096 - bytes_read).unwrap_or(4096);
        let count = view.read(&mut buffer[..remaining])?;
        if count == 0 {
            break;
        }
        bytes_read += count as u64;
    }
    Ok(StructureWorkResult {
        work_item_id: item.id.clone(),
        bytes_read,
        status: StructureWorkStatus::Verified,
    })
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), StructureAnalysisError> {
    if cancel.load(std::sync::atomic::Ordering::Acquire) {
        return Err(StructureAnalysisError::Cancelled);
    }
    Ok(())
}
