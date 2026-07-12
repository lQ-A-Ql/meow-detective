pub mod analysis_commands;
pub mod artifact_commands;
pub mod batch_commands;
pub mod case_commands;
pub(crate) mod command_support;
pub mod file_commands;
pub mod graph_commands;
pub mod import;
pub mod job_commands;
pub mod mcp_commands;
pub mod notebook_commands;
pub mod report_commands;
pub mod rule_pack_commands;
pub mod search_commands;
pub mod settings_commands;
pub mod timeline_commands;

#[cfg(test)]
#[path = "../../tests/unit/commands/benchmarks/mod.rs"]
mod tests;
