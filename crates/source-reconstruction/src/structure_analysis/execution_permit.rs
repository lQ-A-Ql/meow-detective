use std::{io, sync::Arc};

use source_kernel::source_runtime::{verification::ValidatedReceipt, SourcePublicationLease};

use super::StructureAnalysisRun;

pub(crate) struct VerifiedStructureAnalysis {
    pub report: StructureAnalysisRun,
    pub permit: StructureVerificationPermit,
}

pub(crate) struct StructureVerificationPermit {
    pub(super) proofs: Vec<Arc<ValidatedReceipt>>,
}

impl StructureVerificationPermit {
    pub(crate) fn bind_publication(
        self,
        mut lease: SourcePublicationLease,
    ) -> io::Result<SourcePublicationLease> {
        for proof in self.proofs {
            lease = lease.with_receipt(proof)?;
        }
        Ok(lease)
    }
}
