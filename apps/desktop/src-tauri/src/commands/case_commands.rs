//! Case lifecycle, recent-case, metrics, and deletion commands.

mod close;
mod deletion;
mod lifecycle;
mod metrics;
mod recent;

pub use close::close_case;
pub use deletion::{delete_case, delete_data_source};
pub use lifecycle::{create_analysis_demo_case, create_case, get_current_case, open_case};
pub use metrics::{get_case_metrics, get_data_sources, get_recent_objects, rename_data_source};
pub use recent::{get_recent_cases, remove_case_from_list};

#[cfg(test)]
#[path = "../../tests/unit/commands/case_commands/mod.rs"]
mod tests;
