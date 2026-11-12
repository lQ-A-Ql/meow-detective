use std::sync::atomic::{AtomicBool, Ordering};

use super::{DerivedSourceError, DerivedSourceResult};

pub(super) fn ensure_not_cancelled(cancel_token: &AtomicBool) -> DerivedSourceResult<()> {
    if cancel_token.load(Ordering::Relaxed) {
        Err(DerivedSourceError::ProcessingCancelled)
    } else {
        Ok(())
    }
}
