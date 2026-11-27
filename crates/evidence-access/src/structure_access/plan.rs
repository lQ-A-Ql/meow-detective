use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use domain::{
    structure::{
        ByteRange, ByteRangeSet, ReadDemand, ReadDemandError, ReadIntent, ReadPriority,
        ReadVerification, SourceRevisionRef, StructureAttemptRef,
    },
    DataSourceKind,
};
use evidence_core::EvidenceReader;
use sha2::{Digest, Sha256};

use super::VerifiedView;
use crate::{
    datasource_service::{CollectionPlanError, ImageCollectionPlan},
    mdraid::{mdraid_array_fingerprint, open_mdraid_reader, MdRaidArrayPlan, MdRaidPlanError},
};

pub enum AccessTarget<'a> {
    CollectionSource {
        collection: &'a ImageCollectionPlan,
        path: &'a Path,
        kind: &'a DataSourceKind,
    },
    MdRaid(&'a MdRaidArrayPlan),
}

pub struct TargetAccessRequest<'a> {
    pub target: AccessTarget<'a>,
    pub source_revision: SourceRevisionRef,
    pub ranges: Option<Vec<ByteRange>>,
    pub intent: ReadIntent,
    pub priority: ReadPriority,
    pub deadline_unix_ms: Option<u64>,
    pub reorderable: bool,
    pub consumers: Vec<String>,
    pub verification: ReadVerification,
    pub attempt: Option<StructureAttemptRef>,
}

#[derive(Debug, thiserror::Error)]
pub enum TargetAccessError {
    #[error("target range is invalid: {0}")]
    Range(#[from] domain::structure::ByteRangeSetError),
    #[error("read demand is invalid: {0}")]
    Demand(#[from] ReadDemandError),
    #[error("collection target is unavailable: {0}")]
    Collection(#[from] CollectionPlanError),
    #[error("MD target is unavailable: {0}")]
    MdRaid(#[from] MdRaidPlanError),
    #[error("target reader failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("target access was cancelled")]
    Cancelled,
    #[error("verified digest mismatch: expected {expected}, actual {actual}")]
    DigestMismatch { expected: String, actual: String },
}

impl From<TargetAccessError> for MdRaidPlanError {
    fn from(error: TargetAccessError) -> Self {
        match error {
            TargetAccessError::MdRaid(error) => error,
            TargetAccessError::Collection(error) => Self::Collection(error),
            TargetAccessError::Io(error) => Self::Io(error),
            TargetAccessError::Cancelled => Self::Cancelled,
            TargetAccessError::Range(_) | TargetAccessError::Demand(_) => {
                Self::InvalidContentIdentity
            }
            TargetAccessError::DigestMismatch { .. } => Self::SourceContentChanged,
        }
    }
}

pub struct TargetAccessPlan {
    demand: ReadDemand,
    reader: Box<dyn EvidenceReader>,
}

impl TargetAccessPlan {
    pub fn demand(&self) -> &ReadDemand {
        &self.demand
    }
}

pub struct DiscoveryAccessPlan {
    ranges: ByteRangeSet,
    reader: Box<dyn EvidenceReader>,
}

pub fn plan_target_access(
    request: TargetAccessRequest<'_>,
    cancel: &AtomicBool,
) -> Result<TargetAccessPlan, TargetAccessError> {
    check_cancel(cancel)?;
    let (fingerprint, reader) = open_target(request.target, cancel)?;
    let target_size = reader.info().size;
    let ranges = match request.ranges {
        Some(ranges) => ByteRangeSet::new(target_size, ranges)?,
        None => ByteRangeSet::full(target_size)?,
    };
    let demand = ReadDemand::new(
        request.source_revision,
        fingerprint,
        request.intent,
        ranges,
        request.priority,
        request.deadline_unix_ms,
        request.reorderable,
        request.consumers,
        request.verification,
    )?;
    let demand = match request.attempt {
        Some(attempt) => demand.with_attempt(attempt)?,
        None => demand,
    };
    Ok(TargetAccessPlan { demand, reader })
}

pub(crate) fn plan_discovery_access(
    target: AccessTarget<'_>,
    cancel: &AtomicBool,
) -> Result<DiscoveryAccessPlan, TargetAccessError> {
    check_cancel(cancel)?;
    let (_, reader) = open_target(target, cancel)?;
    let target_size = reader.info().size;
    Ok(DiscoveryAccessPlan {
        ranges: ByteRangeSet::full(target_size)?,
        reader,
    })
}

pub fn open_verified_view(
    plan: TargetAccessPlan,
    cancel: &AtomicBool,
) -> Result<VerifiedView<'_>, TargetAccessError> {
    check_cancel(cancel)?;
    Ok(VerifiedView::new(plan.reader, plan.demand.ranges, cancel))
}

pub(crate) fn open_discovery_view<'a>(
    plan: DiscoveryAccessPlan,
    cancel: &'a AtomicBool,
) -> Result<VerifiedView<'a>, TargetAccessError> {
    check_cancel(cancel)?;
    Ok(VerifiedView::new(plan.reader, plan.ranges, cancel))
}

pub fn verify_derived_view(
    plan: TargetAccessPlan,
    expected_sha256: &str,
    cancel: &AtomicBool,
) -> Result<String, TargetAccessError> {
    let mut view = open_verified_view(plan, cancel)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        check_cancel(cancel)?;
        let count = std::io::Read::read(&mut view, &mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    check_cancel(cancel)?;
    let actual = hex::encode(digest.finalize());
    if actual != expected_sha256 {
        return Err(TargetAccessError::DigestMismatch {
            expected: expected_sha256.to_owned(),
            actual,
        });
    }
    Ok(actual)
}

fn open_target(
    target: AccessTarget<'_>,
    cancel: &AtomicBool,
) -> Result<(String, Box<dyn EvidenceReader>), TargetAccessError> {
    match target {
        AccessTarget::CollectionSource {
            collection,
            path,
            kind,
        } => {
            let content = collection.source_content(path, kind)?;
            let mut identity = Sha256::new();
            identity.update(collection.content_fingerprint().as_bytes());
            identity.update(content.logical_sha256.as_bytes());
            identity.update(content.source_path.as_bytes());
            let fingerprint = format!("collection-source-v2:{}", hex::encode(identity.finalize()));
            let factory = collection.verified_factory(path, kind, cancel)?;
            Ok((fingerprint, factory.open_reader_with_cancel(cancel)?))
        }
        AccessTarget::MdRaid(plan) => Ok((
            format!("mdraid-v2:{}", mdraid_array_fingerprint(plan)?),
            Box::new(open_mdraid_reader(plan)?),
        )),
    }
}

pub(super) fn check_cancel(cancel: &AtomicBool) -> Result<(), TargetAccessError> {
    if cancel.load(Ordering::Acquire) {
        return Err(TargetAccessError::Cancelled);
    }
    Ok(())
}
