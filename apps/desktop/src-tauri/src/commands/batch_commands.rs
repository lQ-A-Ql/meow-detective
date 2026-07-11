//! Batch command facade.

mod lifecycle;
mod planning;
mod query;

pub use lifecycle::{cancel_batch, pause_batch, resume_batch, start_batch};
pub use planning::create_batch_plan;
pub use query::{get_batch_job, list_batch_jobs};

#[cfg(test)]
#[path = "../../tests/unit/commands/batch_commands_test.rs"]
mod tests;
