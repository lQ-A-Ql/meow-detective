use std::{io, sync::Arc};

use domain::structure::{SourceRevisionRef, StructureAttemptRef, VerificationReceipt};
use source_kernel::source_runtime::{
    snapshot_analysis::SnapshotAnalysisProof, verification::ValidatedReceipt,
};

pub enum ActivityProof {
    Range(Arc<ValidatedReceipt>),
    Snapshot(Box<SnapshotAnalysisProof>),
}

impl ActivityProof {
    pub fn validate(&self) -> io::Result<()> {
        match self {
            Self::Range(proof) => proof.validate(),
            Self::Snapshot(proof) => proof.validate(),
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Range(proof) => &proof.receipt().id,
            Self::Snapshot(proof) => &proof.receipt().id,
        }
    }

    pub fn source_revision(&self) -> &SourceRevisionRef {
        match self {
            Self::Range(proof) => &proof.receipt().source_revision,
            Self::Snapshot(proof) => &proof.receipt().source_revision,
        }
    }

    pub fn attempt(&self) -> &StructureAttemptRef {
        match self {
            Self::Range(proof) => &proof.receipt().attempt,
            Self::Snapshot(proof) => &proof.receipt().attempt,
        }
    }

    pub fn output_sha256(&self) -> &str {
        match self {
            Self::Range(proof) => &proof.receipt().output_sha256,
            Self::Snapshot(proof) => &proof.receipt().output_sha256,
        }
    }

    pub fn read_receipt(&self) -> Option<&VerificationReceipt> {
        match self {
            Self::Range(proof) => Some(proof.receipt()),
            Self::Snapshot(_) => None,
        }
    }
}
