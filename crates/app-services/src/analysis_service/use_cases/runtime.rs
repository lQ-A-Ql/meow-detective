use std::sync::Arc;

use crate::bitlocker_runtime::BitLockerUnlockRegistry;
use crate::file_service::SourceReadContext;

/// Runtime-only evidence capabilities available to an interactive analysis run.
///
/// Verified BitLocker keys remain in the process registry; this value never
/// contains credentials or persists plaintext material.
#[derive(Clone, Default)]
pub struct AnalysisSourceReadRuntime {
    bitlocker_runtime: Option<Arc<BitLockerUnlockRegistry>>,
    publication: Option<crate::source_runtime::SourcePublicationLease>,
}

impl AnalysisSourceReadRuntime {
    #[must_use]
    pub fn with_bitlocker_runtime(runtime: Arc<BitLockerUnlockRegistry>) -> Self {
        Self {
            bitlocker_runtime: Some(runtime),
            ..Self::default()
        }
    }

    pub(crate) fn with_publication(
        mut self,
        lease: crate::source_runtime::SourcePublicationLease,
    ) -> Self {
        self.publication = Some(lease);
        self
    }
    pub(crate) fn resource_pool(&self) -> crate::resource_budget::ResourcePool {
        crate::resource_budget::pool_for(self.publication.as_ref())
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
    ) -> Option<crate::source_runtime::lease::SourcePublicationFence<'a>> {
        self.publication
            .as_ref()
            .map(|lease| lease.fence_connection(conn))
    }

    pub(crate) fn bind<'a>(&self, reader: SourceReadContext<'a>) -> SourceReadContext<'a> {
        match &self.bitlocker_runtime {
            Some(runtime) => reader.with_bitlocker_runtime(Arc::clone(runtime)),
            None => reader,
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/runtime.rs"]
mod tests;
