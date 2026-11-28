use std::{path::Path, sync::atomic::AtomicBool};

use domain::structure::{ByteRangeSet, ReadDemand, ReadVerification, SourceRevisionRef};
use persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo;
use source_kernel::source_runtime::verification::VerificationBatch;

use super::{registered::RegisteredAccessRequest, TargetAccessError};

pub(super) fn verify_inputs(
    conn: &rusqlite::Connection,
    root: &Path,
    demand: &ReadDemand,
    batch: &VerificationBatch,
    cancel: &AtomicBool,
) -> Result<(), TargetAccessError> {
    let repo = SourceRevisionRepo::new(conn);
    for input in repo.dependency_revisions(&demand.source_revision)? {
        if input == demand.source_revision
            || !repo.inputs(&input.case_id, &input.source_id)?.is_empty()
        {
            continue;
        }
        verify_input(conn, root, &input, demand, batch, cancel)?;
    }
    Ok(())
}

fn verify_input(
    conn: &rusqlite::Connection,
    root: &Path,
    input: &SourceRevisionRef,
    requested: &ReadDemand,
    batch: &VerificationBatch,
    cancel: &AtomicBool,
) -> Result<(), TargetAccessError> {
    let (_, content) = SourceRevisionRepo::new(conn).current(&input.case_id, &input.source_id)?;
    let expected = content
        .logical_sha256
        .as_deref()
        .ok_or(TargetAccessError::UnsupportedTarget)?;
    let mut demand = ReadDemand::new(
        input.clone(),
        format!("source-revision:{}:{}", input.source_id, input.revision),
        requested.intent,
        ByteRangeSet::full(content.logical_size)?,
        requested.priority,
        requested.deadline_unix_ms,
        requested.reorderable,
        requested.consumers.clone(),
        ReadVerification::ContentDigest,
    )?;
    if let Some(attempt) = &requested.attempt {
        demand = demand.with_attempt(attempt.clone())?;
    }
    super::registered::verify_registered_access(
        conn,
        root,
        RegisteredAccessRequest {
            demand,
            file_id: None,
        },
        batch,
        "registered-source/1",
        Some(expected),
        false,
        cancel,
    )?
    .validate()?;
    Ok(())
}
