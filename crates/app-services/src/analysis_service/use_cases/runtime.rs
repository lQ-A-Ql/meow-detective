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
}

impl AnalysisSourceReadRuntime {
    #[must_use]
    pub fn with_bitlocker_runtime(runtime: Arc<BitLockerUnlockRegistry>) -> Self {
        Self {
            bitlocker_runtime: Some(runtime),
        }
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
