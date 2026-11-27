use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

use domain::{
    structure::{ByteRange, ByteRangeError, RangeReadCapability, ReadDemand, ReadIntent},
    DataSourceKind,
};
use evidence_core::{EvidenceReader, PartitionWindowReader};
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

#[derive(Debug, thiserror::Error)]
pub enum TargetAccessError {
    #[error("target range is invalid: {0}")]
    Range(#[from] ByteRangeError),
    #[error("collection target is unavailable: {0}")]
    Collection(#[from] CollectionPlanError),
    #[error("MD target is unavailable: {0}")]
    MdRaid(#[from] MdRaidPlanError),
    #[error("target reader failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("target access was cancelled")]
    Cancelled,
}

impl From<TargetAccessError> for MdRaidPlanError {
    fn from(error: TargetAccessError) -> Self {
        match error {
            TargetAccessError::MdRaid(error) => error,
            TargetAccessError::Collection(error) => Self::Collection(error),
            TargetAccessError::Io(error) => Self::Io(error),
            TargetAccessError::Cancelled => Self::Cancelled,
            TargetAccessError::Range(error) => {
                Self::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, error))
            }
        }
    }
}

/// Owns the verified reader admitted by planning. Opening consumes the plan,
/// so callers cannot substitute a different target or widen its range.
pub struct TargetAccessPlan {
    demand: ReadDemand,
    reader: Box<dyn EvidenceReader>,
}

impl TargetAccessPlan {
    pub fn demand(&self) -> &ReadDemand {
        &self.demand
    }
}

/// Plans one bounded range, or the entire native view when range is absent.
/// Native readers retain authority over mapping, degradation and verification.
pub fn plan_target_access(
    target: AccessTarget<'_>,
    range: Option<ByteRange>,
    intent: ReadIntent,
    cancel: &AtomicBool,
) -> Result<TargetAccessPlan, TargetAccessError> {
    check_cancel(cancel)?;
    let (fingerprint, reader): (String, Box<dyn EvidenceReader>) = match target {
        AccessTarget::CollectionSource {
            collection,
            path,
            kind,
        } => {
            let content = collection.source_content(path, kind)?;
            if let Some(range) = range {
                range.validate(content.logical_size)?;
            }
            let mut identity = Sha256::new();
            identity.update(collection.content_fingerprint().as_bytes());
            identity.update(content.logical_sha256.as_bytes());
            identity.update(content.source_path.as_bytes());
            let fingerprint = format!("collection-source-v1:{}", hex::encode(identity.finalize()));
            let factory = collection.verified_factory(path, kind, cancel)?;
            (fingerprint, factory.open_reader_with_cancel(cancel)?)
        }
        AccessTarget::MdRaid(plan) => (
            format!("mdraid-v1:{}", mdraid_array_fingerprint(plan)?),
            Box::new(open_mdraid_reader(plan)?),
        ),
    };
    check_cancel(cancel)?;
    let target_size = reader.info().size;
    let range = range.unwrap_or(ByteRange::new(0, target_size)?);
    range.validate(target_size)?;
    Ok(TargetAccessPlan {
        demand: ReadDemand {
            target_fingerprint: fingerprint,
            intent,
            capability: RangeReadCapability { range, target_size },
        },
        reader,
    })
}

pub fn open_verified_view(
    plan: TargetAccessPlan,
    cancel: &AtomicBool,
) -> Result<VerifiedView<'_>, TargetAccessError> {
    check_cancel(cancel)?;
    let range = plan.demand.capability.range;
    let window = PartitionWindowReader::new(plan.reader, range.start(), Some(range.length()))?;
    Ok(VerifiedView::new(window, cancel))
}

pub(super) fn check_cancel(cancel: &AtomicBool) -> Result<(), TargetAccessError> {
    if cancel.load(Ordering::Acquire) {
        return Err(TargetAccessError::Cancelled);
    }
    Ok(())
}
