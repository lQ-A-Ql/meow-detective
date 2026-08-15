mod builder;
pub mod error;
pub mod fact_loader;
pub mod runtime;
pub mod scoring;

pub use error::GovernanceError;

use rusqlite::Connection;
use std::path::Path;
use transport::dto::V2GovernanceSnapshotDto;

use crate::governance::runtime::build_runtime_signals;

pub fn get_v2_governance_snapshot(
    conn: &Connection,
    case_id: &str,
) -> Result<V2GovernanceSnapshotDto, GovernanceError> {
    let runtime_signals = build_runtime_signals(conn, case_id)?;
    builder::build_v2_governance_snapshot_with_runtime(conn, case_id, runtime_signals)
}

pub fn get_v2_governance_snapshot_for_case(
    conn: &Connection,
    case_root: &Path,
    case_id: &str,
) -> Result<V2GovernanceSnapshotDto, GovernanceError> {
    let runtime_signals =
        crate::governance::runtime::build_runtime_signals_for_case(conn, case_root, case_id)?;
    builder::build_v2_governance_snapshot_with_runtime(conn, case_id, runtime_signals)
}

#[cfg(test)]
#[path = "../../tests/unit/governance/mod.rs"]
mod tests;
