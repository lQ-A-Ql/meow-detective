//! Stack isolation for synchronous registry operations.
//!
//! Session release can synchronously stop VMware, flush the copy-on-write
//! overlay and join the Dokan mount worker. Keep that sequence off Tokio's
//! shared worker stack, including when cleanup is triggered by case
//! lifecycle commands rather than an explicit emulation command.

use std::panic::{catch_unwind, AssertUnwindSafe};

use super::{EmulationRegistry, EmulationRegistryError};

const CLEANUP_STACK_BYTES: usize = 16 * 1024 * 1024;

impl EmulationRegistry {
    pub(crate) fn cleanup_case_on_large_stack(
        &self,
        case_id: &str,
    ) -> Result<(), EmulationRegistryError> {
        if isolated_worker_thread() {
            return self.cleanup_case_inner(case_id);
        }
        let registry = self.clone();
        let case_id = case_id.to_owned();
        run_on_large_stack("cleanup-case", move || {
            registry.cleanup_case_inner(&case_id)
        })
    }

    pub(crate) fn cleanup_source_on_large_stack(
        &self,
        case_id: &str,
        data_source_id: &str,
    ) -> Result<(), EmulationRegistryError> {
        if isolated_worker_thread() {
            return self.cleanup_source_inner(case_id, data_source_id);
        }
        let registry = self.clone();
        let case_id = case_id.to_owned();
        let data_source_id = data_source_id.to_owned();
        run_on_large_stack("cleanup-source", move || {
            registry.cleanup_source_inner(&case_id, &data_source_id)
        })
    }
}

pub(crate) fn isolated_worker_thread() -> bool {
    std::thread::current()
        .name()
        .is_some_and(|name| name.starts_with("meow-emulation-"))
}

pub(crate) fn run_on_large_stack<T, F>(
    operation: &'static str,
    work: F,
) -> Result<T, EmulationRegistryError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, EmulationRegistryError> + Send + 'static,
{
    let worker = std::thread::Builder::new()
        .name(format!("meow-emulation-{operation}"))
        .stack_size(CLEANUP_STACK_BYTES)
        .spawn(move || {
            catch_unwind(AssertUnwindSafe(work)).unwrap_or_else(|_| {
                tracing::error!(operation, "emulation worker panicked");
                Err(EmulationRegistryError::Backend(format!(
                    "emulation {operation} worker panicked"
                )))
            })
        })
        .map_err(|error| {
            tracing::error!(operation, error = %error, "failed to start emulation worker");
            EmulationRegistryError::Backend(format!("failed to start emulation worker: {error}"))
        })?;

    worker.join().unwrap_or_else(|_| {
        tracing::error!(operation, "emulation worker stopped unexpectedly");
        Err(EmulationRegistryError::Backend(format!(
            "emulation {operation} worker stopped unexpectedly"
        )))
    })
}
