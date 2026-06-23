pub mod error;
pub mod fact_loader;
pub mod runtime;
pub mod scoring;

pub use error::GovernanceError;

use chrono::Utc;
use rusqlite::Connection;
use transport::dto::{
    BenchmarkRequirementStatusDto, BenchmarkSummaryDto, SecurityAuditSummaryDto,
    V2GovernanceSnapshotDto,
};

use crate::governance::{
    fact_loader::*,
    runtime::{build_runtime_signals, security_audit_snapshot},
    scoring::{benchmark_required_checks, release_gates, release_scorecard},
};

pub fn get_v2_governance_snapshot(
    conn: &Connection,
    case_id: &str,
) -> Result<V2GovernanceSnapshotDto, GovernanceError> {
    let runtime_signals = build_runtime_signals(conn, case_id)?;
    let audit_snapshot = security_audit_snapshot(conn, case_id)?;
    let chain_catalog = governance_chain_catalog();
    let verification_chains = verification_chains(chain_catalog);
    let support_matrix_entries = support_matrix_entries(chain_catalog);
    let support_matrix = support_matrix_summary(&support_matrix_entries);
    let benchmark_file = benchmark_baseline();
    let security_file = security_taxonomy();
    let release_policy = release_policy();
    let benchmark_scenarios = benchmark_file.scenarios.clone();
    let benchmark_required_checks =
        benchmark_required_checks(&benchmark_file.required_checks, &benchmark_scenarios);
    let covered_required_count = benchmark_required_checks
        .iter()
        .filter(|item| item.status == BenchmarkRequirementStatusDto::Covered)
        .count() as u32;
    let missing_required_count = benchmark_required_checks
        .iter()
        .filter(|item| item.status == BenchmarkRequirementStatusDto::Missing)
        .count() as u32;
    let exceeded_required_count = benchmark_required_checks
        .iter()
        .filter(|item| item.status == BenchmarkRequirementStatusDto::Exceeded)
        .count() as u32;
    let benchmark = BenchmarkSummaryDto {
        host_profile: benchmark_file.host_profile.clone(),
        baseline_version: benchmark_file.baseline_version.clone(),
        last_verified_at: benchmark_file.last_verified_at.clone(),
        scenarios: benchmark_scenarios,
        required_checks: benchmark_required_checks,
        covered_required_count,
        missing_required_count,
        exceeded_required_count,
    };
    let security = SecurityAuditSummaryDto {
        export_overwrite_default: security_file.security_defaults.export_overwrite_default,
        export_path_guard_enabled: security_file.security_defaults.export_path_guard_enabled,
        stdio_command_whitelist_enforced: security_file
            .security_defaults
            .stdio_command_whitelist_enforced,
        sse_https_only: security_file.security_defaults.sse_https_only,
        embedded_credentials_blocked: security_file.security_defaults.embedded_credentials_blocked,
        media_handle_scoped: security_file.security_defaults.media_handle_scoped,
        error_redaction_enabled: security_file.security_defaults.error_redaction_enabled,
        audit_log_required: audit_snapshot.audit_log_required,
        audit_event_count: audit_snapshot.audit_event_count,
        sensitive_audit_event_count: audit_snapshot.sensitive_audit_event_count,
        recent_audit_entries: audit_snapshot.recent_audit_entries,
        notes: security_file.security_defaults.notes.clone(),
    };
    let release_gates = release_gates(
        &verification_chains,
        &support_matrix,
        &benchmark,
        &security,
        &runtime_signals,
        release_policy,
    );

    Ok(V2GovernanceSnapshotDto {
        generated_at: Utc::now().to_rfc3339(),
        fact_sources: governance_fact_sources(),
        runtime_results: governance_runtime_results_dto(),
        verification_chains,
        support_matrix,
        support_matrix_entries,
        known_limitations: known_limitations(),
        benchmark,
        security,
        error_taxonomy_entries: security_file.error_taxonomy_entries.clone(),
        release_gates: release_gates.clone(),
        release_scorecard: release_scorecard(&release_gates, &runtime_signals),
        runtime_signals,
    })
}

#[cfg(test)]
mod tests;
