use crate::analysis_service::cancellation::ensure_not_cancelled;
use crate::analysis_service::error::AnalysisServiceError;
use std::sync::atomic::AtomicBool;
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant};

const EXTRACTION_GATE_POLL_INTERVAL: Duration = Duration::from_millis(100);
const EXTRACTION_GATE_REPORT_INTERVAL: Duration = Duration::from_secs(1);

static EXTRACTION_GATE: ExtractionGate = ExtractionGate::new();

pub(in crate::analysis_service::extraction) fn acquire_extraction_slot(
    cancel_token: &AtomicBool,
    on_wait: impl FnMut(Duration),
) -> Result<MutexGuard<'static, ()>, AnalysisServiceError> {
    EXTRACTION_GATE.acquire(cancel_token, on_wait)
}

pub(in crate::analysis_service::extraction) struct ExtractionGate {
    mutex: Mutex<()>,
}

impl ExtractionGate {
    pub(in crate::analysis_service::extraction) const fn new() -> Self {
        Self {
            mutex: Mutex::new(()),
        }
    }

    pub(in crate::analysis_service::extraction) fn acquire<'a>(
        &'a self,
        cancel_token: &AtomicBool,
        mut on_wait: impl FnMut(Duration),
    ) -> Result<MutexGuard<'a, ()>, AnalysisServiceError> {
        let started = Instant::now();
        let mut last_report = None::<Instant>;
        loop {
            ensure_not_cancelled(cancel_token)?;
            match self.mutex.try_lock() {
                Ok(guard) => return Ok(guard),
                Err(TryLockError::Poisoned(poisoned)) => {
                    tracing::warn!("Recovering poisoned analysis extraction serial gate");
                    return Ok(poisoned.into_inner());
                }
                Err(TryLockError::WouldBlock) => {
                    if last_report
                        .is_none_or(|last| last.elapsed() >= EXTRACTION_GATE_REPORT_INTERVAL)
                    {
                        on_wait(started.elapsed());
                        last_report = Some(Instant::now());
                    }
                    std::thread::sleep(EXTRACTION_GATE_POLL_INTERVAL);
                }
            }
        }
    }
}
