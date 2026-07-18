use std::sync::atomic::{AtomicBool, Ordering};

use super::AnalysisServiceError;

pub(crate) fn ensure_not_cancelled(cancel_token: &AtomicBool) -> Result<(), AnalysisServiceError> {
    if cancel_token.load(Ordering::Relaxed) {
        Err(AnalysisServiceError::Cancelled)
    } else {
        Ok(())
    }
}
