//! Dedicated workers for stack-intensive emulation operations.
//!
//! Evidence probing and host-side filesystem edits can traverse several
//! parser layers. They must not consume Tokio's shared blocking-worker stack:
//! an exhausted worker terminates the command before it can return a typed
//! error to the investigator.

use std::panic::{catch_unwind, AssertUnwindSafe};

use transport::CommandError;

const EMULATION_WORKER_STACK_BYTES: usize = 16 * 1024 * 1024;

/// Run one synchronous emulation operation on an isolated, large-stack
/// thread while the Tauri runtime only awaits its result.
pub(super) async fn run_emulation_blocking<T, F>(
    operation: &'static str,
    work: F,
) -> Result<T, CommandError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, CommandError> + Send + 'static,
{
    let (sender, receiver) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name(format!("meow-emulation-{operation}"))
        .stack_size(EMULATION_WORKER_STACK_BYTES)
        .spawn(move || {
            let result = catch_unwind(AssertUnwindSafe(work)).unwrap_or_else(|_| {
                tracing::error!(operation, "emulation worker panicked");
                Err(CommandError::internal(
                    "The emulation operation failed unexpectedly",
                ))
            });
            let _ = sender.send(result);
        })
        .map_err(|error| {
            tracing::error!(operation, error = %error, "failed to start emulation worker");
            CommandError::internal("Unable to start the emulation operation")
        })?;
    receiver.await.map_err(|_| {
        tracing::error!(
            operation,
            "emulation worker stopped before returning a result"
        );
        CommandError::internal("The emulation operation stopped unexpectedly")
    })?
}

#[cfg(test)]
#[path = "../../../tests/unit/commands/emulation_worker.rs"]
mod tests;
