//! Notebook service: create/update/list notebook entries, manage evidence citations,
//! and record/list investigation steps.

mod citation_operations;
mod dto_conversion;
mod entry_operations;
pub mod error;
mod investigation_step_operations;
mod request_filters;

pub use citation_operations::add_citation;
pub use entry_operations::{create_entry, get_thread, list_entries, update_entry};
pub use error::NotebookError;
pub use investigation_step_operations::{list_steps, record_step};
pub use request_filters::{list_entries_for_request, list_steps_for_request};

#[cfg(test)]
#[path = "../../tests/unit/notebook_service/mod.rs"]
mod tests;
