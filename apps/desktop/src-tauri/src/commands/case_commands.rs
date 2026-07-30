//! Case lifecycle, recent-case, metrics, and deletion commands.

mod close;
mod create;
mod current;
mod deletion;
mod demo;
mod lifecycle_support;
mod metrics;
mod open;
mod open_restore;
mod recent;
mod recovery;
mod transition;

pub use close::close_case;
pub use create::create_case;
pub use current::get_current_case;
pub use deletion::{delete_case, delete_data_source};
pub use demo::create_analysis_demo_case;
pub use metrics::{get_case_metrics, get_data_sources, get_recent_objects, rename_data_source};
pub use open::open_case;
pub use recent::{get_recent_cases, remove_case_from_list};

#[cfg(test)]
#[path = "../../tests/unit/commands/case_commands/mod.rs"]
mod tests;
