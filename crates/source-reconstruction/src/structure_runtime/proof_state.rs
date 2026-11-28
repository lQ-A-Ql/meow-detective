use std::io;

use domain::structure::{CancellationScope, ConflictReason, MetadataFailure, TargetFailure};

pub(super) fn proof_failure(error: &io::Error) -> Option<TargetFailure> {
    use evidence_access::{
        source_runtime::content::ContentError, structure_access::TargetAccessError,
    };
    use source_kernel::source_runtime::{lease::SourceInvalidated, range_view::RangeReadError};
    if let Some(TargetAccessError::Io(inner)) = error
        .get_ref()
        .and_then(|cause| cause.downcast_ref::<TargetAccessError>())
    {
        return proof_failure(inner);
    }
    if let Some(content) = error
        .get_ref()
        .and_then(|cause| cause.downcast_ref::<ContentError>())
    {
        return match content {
            ContentError::Io(inner) => proof_failure(inner),
            ContentError::Changed => Some(TargetFailure::Conflicted {
                reason: ConflictReason::InputChanged,
            }),
            ContentError::Cancelled => Some(TargetFailure::Cancelled {
                scope: CancellationScope::Target,
            }),
            _ => None,
        };
    }
    if error
        .get_ref()
        .is_some_and(|cause| cause.is::<SourceInvalidated>())
    {
        return Some(TargetFailure::Conflicted {
            reason: ConflictReason::InputChanged,
        });
    }
    if error
        .get_ref()
        .is_some_and(|cause| cause.is::<RangeReadError>())
    {
        return Some(TargetFailure::Cancelled {
            scope: CancellationScope::Target,
        });
    }
    Some(match error.kind() {
        io::ErrorKind::Interrupted => TargetFailure::Cancelled {
            scope: CancellationScope::Target,
        },
        io::ErrorKind::Other => return None,
        io::ErrorKind::InvalidData => TargetFailure::CorruptMetadata {
            reason: MetadataFailure::InvalidStructure,
        },
        io::ErrorKind::UnexpectedEof => TargetFailure::CorruptMetadata {
            reason: MetadataFailure::Truncated,
        },
        io::ErrorKind::TimedOut => TargetFailure::DeadlineExceeded,
        _ => TargetFailure::IoUnavailable,
    })
}

pub(super) fn classify(error: io::Error) -> super::RuntimeError {
    match proof_failure(&error) {
        Some(failure) => super::RuntimeError::Target(failure),
        None => super::RuntimeError::Io(error),
    }
}
