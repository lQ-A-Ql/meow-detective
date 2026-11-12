use std::sync::Arc;

use evidence_access::bitlocker_runtime::BitLockerUnlockRegistry;
use evidence_access::file_service::SourceReadContext;

/// Runtime-only evidence capabilities available to an interactive analysis run.
///
/// Verified BitLocker keys remain in the process registry; this value never
/// contains credentials or persists plaintext material.
#[derive(Clone, Default)]
pub struct AnalysisSourceReadRuntime {
    bitlocker_runtime: Option<Arc<BitLockerUnlockRegistry>>,
    publication: Option<evidence_access::source_runtime::SourcePublicationLease>,
}

impl AnalysisSourceReadRuntime {
    #[must_use]
    pub fn with_bitlocker_runtime(runtime: Arc<BitLockerUnlockRegistry>) -> Self {
        Self {
            bitlocker_runtime: Some(runtime),
            ..Self::default()
        }
    }

    /// Attaches the publication lease of the source being analysed so reads
    /// are fenced against concurrent generation replacement.
    pub fn with_publication(
        mut self,
        lease: evidence_access::source_runtime::SourcePublicationLease,
    ) -> Self {
        self.publication = Some(lease);
        self
    }
    pub(crate) fn resource_pool(&self) -> source_kernel::resource_budget::ResourcePool {
        source_kernel::resource_budget::pool_for(self.publication.as_ref())
    }
    pub(crate) fn validate_publication(&self) -> std::io::Result<()> {
        if let Some(lease) = &self.publication {
            lease.validate()?;
        }
        Ok(())
    }
    pub(crate) fn fence<'a>(
        &self,
        conn: &'a rusqlite::Connection,
    ) -> Option<evidence_access::source_runtime::lease::SourcePublicationFence<'a>> {
        self.publication
            .as_ref()
            .map(|lease| lease.fence_connection(conn))
    }

    /// Binds the runtime-only BitLocker capabilities to a source read context.
    ///
    /// Hosted by callers outside this crate (for example the app-services
    /// evidence scan) that perform their own artifact extraction loop.
    pub fn bind<'a>(&self, reader: SourceReadContext<'a>) -> SourceReadContext<'a> {
        match &self.bitlocker_runtime {
            Some(runtime) => reader.with_bitlocker_runtime(Arc::clone(runtime)),
            None => reader,
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/runtime.rs"]
mod tests;
