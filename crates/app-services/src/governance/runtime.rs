use domain::DataSourceHashStatus;
use persistence_sqlite::repositories::{audit_repo::AuditRepo, datasource_repo::DataSourceRepo};
use rusqlite::Connection;
use std::path::Path;
use transport::dto::{
    CorrelationCoverageStatusDto, GovernanceRuntimeSignalsDto, SecurityAuditEntryDto,
    VerificationGuaranteeLevelDto,
};

use crate::governance::error::GovernanceError;

#[derive(Debug, Default, Clone)]
pub(crate) struct CorrelationRuntimeSnapshot {
    pub(crate) snapshot_available: bool,
    pub(crate) lead_count: u32,
    pub(crate) high_confidence_lead_count: u32,
    pub(crate) review_lead_count: u32,
    pub(crate) cluster_count: u32,
    pub(crate) rule_family_count: u32,
    pub(crate) covered_family_count: u32,
    pub(crate) high_confidence_family_count: u32,
    pub(crate) family_coverage: Vec<transport::dto::CorrelationFamilyCoverageDto>,
}

pub(crate) fn build_runtime_signals(
    conn: &Connection,
    case_id: &str,
) -> Result<GovernanceRuntimeSignalsDto, GovernanceError> {
    let correlation = correlation_runtime_snapshot(conn)?;
    build_runtime_signals_with_correlation(conn, case_id, correlation)
}

pub(crate) fn build_runtime_signals_for_case(
    conn: &Connection,
    case_root: &Path,
    case_id: &str,
) -> Result<GovernanceRuntimeSignalsDto, GovernanceError> {
    let correlation = correlation_runtime_snapshot_for_case(
        conn,
        case_root,
        &domain::CaseId(case_id.to_string()),
    )?;
    build_runtime_signals_with_correlation(conn, case_id, correlation)
}

fn build_runtime_signals_with_correlation(
    conn: &Connection,
    case_id: &str,
    correlation: CorrelationRuntimeSnapshot,
) -> Result<GovernanceRuntimeSignalsDto, GovernanceError> {
    let data_sources =
        DataSourceRepo::new(conn).find_by_case(&domain::CaseId(case_id.to_string()))?;
    let jobs = crate::job_service::get_jobs_from_db(conn)
        .map_err(|e| GovernanceError::Internal(e.to_string()))?;
    let reports = crate::report::get_report_history(conn, case_id);

    let hashed_data_source_count = data_sources
        .iter()
        .filter(|source| matches!(source.provenance.hash_status, DataSourceHashStatus::Hashed))
        .count() as u32;
    let pending_hash_data_source_count = data_sources
        .iter()
        .filter(|source| {
            matches!(
                source.provenance.hash_status,
                DataSourceHashStatus::Pending | DataSourceHashStatus::Unknown
            )
        })
        .count() as u32;
    let warning_data_source_count = data_sources
        .iter()
        .filter(|source| !source.provenance.warnings.is_empty())
        .count() as u32;
    let running_job_count = jobs.iter().filter(|job| job.status == "running").count() as u32;
    let partial_job_count = jobs.iter().filter(|job| job.partial).count() as u32;
    let failed_job_count = jobs.iter().filter(|job| job.status == "failed").count() as u32;

    Ok(GovernanceRuntimeSignalsDto {
        data_source_count: data_sources.len() as u32,
        hashed_data_source_count,
        pending_hash_data_source_count,
        warning_data_source_count,
        running_job_count,
        partial_job_count,
        failed_job_count,
        report_count: reports.len() as u32,
        correlation_snapshot_available: correlation.snapshot_available,
        correlation_lead_count: correlation.lead_count,
        correlation_high_confidence_lead_count: correlation.high_confidence_lead_count,
        correlation_review_lead_count: correlation.review_lead_count,
        correlation_cluster_count: correlation.cluster_count,
        correlation_rule_family_count: correlation.rule_family_count,
        correlation_covered_family_count: correlation.covered_family_count,
        correlation_high_confidence_family_count: correlation.high_confidence_family_count,
        correlation_family_coverage: correlation.family_coverage,
    })
}

pub(crate) fn correlation_runtime_snapshot(
    conn: &Connection,
) -> Result<CorrelationRuntimeSnapshot, GovernanceError> {
    let snapshot = crate::correlation::get_correlation_snapshot(conn)
        .map_err(|e| GovernanceError::Internal(e.to_string()))?;
    let high_confidence_lead_count = snapshot
        .leads
        .iter()
        .filter(|lead| {
            matches!(
                lead.confidence,
                transport::dto::CorrelationConfidenceDto::Direct
                    | transport::dto::CorrelationConfidenceDto::Strong
            )
        })
        .count() as u32;
    let review_lead_count = snapshot
        .leads
        .iter()
        .filter(|lead| {
            !lead.caveats.is_empty()
                || matches!(
                    lead.confidence,
                    transport::dto::CorrelationConfidenceDto::Weak
                        | transport::dto::CorrelationConfidenceDto::Heuristic
                )
                || lead.provenance.iter().any(|item| {
                    matches!(
                        item.guarantee_level,
                        VerificationGuaranteeLevelDto::Experimental
                            | VerificationGuaranteeLevelDto::NotGuaranteed
                    )
                })
        })
        .count() as u32;
    let family_coverage = snapshot.family_coverage.clone();
    let covered_family_count = family_coverage
        .iter()
        .filter(|item| item.status == CorrelationCoverageStatusDto::Covered)
        .count() as u32;
    let high_confidence_family_count = family_coverage
        .iter()
        .filter(|item| item.high_confidence_lead_count > 0)
        .count() as u32;

    Ok(CorrelationRuntimeSnapshot {
        snapshot_available: true,
        lead_count: snapshot.lead_count,
        high_confidence_lead_count,
        review_lead_count,
        cluster_count: snapshot.cluster_count,
        rule_family_count: family_coverage.len() as u32,
        covered_family_count,
        high_confidence_family_count,
        family_coverage,
    })
}

pub(crate) fn correlation_runtime_snapshot_for_case(
    conn: &Connection,
    case_root: &Path,
    case_id: &domain::CaseId,
) -> Result<CorrelationRuntimeSnapshot, GovernanceError> {
    let snapshot = crate::correlation::get_correlation_snapshot_for_case(conn, case_root, case_id)?;
    correlation_runtime_from_snapshot(snapshot)
}

fn correlation_runtime_from_snapshot(
    snapshot: transport::dto::CorrelationSnapshotDto,
) -> Result<CorrelationRuntimeSnapshot, GovernanceError> {
    let high_confidence_lead_count = snapshot
        .leads
        .iter()
        .filter(|lead| {
            matches!(
                lead.confidence,
                transport::dto::CorrelationConfidenceDto::Direct
                    | transport::dto::CorrelationConfidenceDto::Strong
            )
        })
        .count() as u32;
    let review_lead_count = snapshot
        .leads
        .iter()
        .filter(|lead| {
            !lead.caveats.is_empty()
                || matches!(
                    lead.confidence,
                    transport::dto::CorrelationConfidenceDto::Weak
                        | transport::dto::CorrelationConfidenceDto::Heuristic
                )
                || lead.provenance.iter().any(|item| {
                    matches!(
                        item.guarantee_level,
                        VerificationGuaranteeLevelDto::Experimental
                            | VerificationGuaranteeLevelDto::NotGuaranteed
                    )
                })
        })
        .count() as u32;
    let family_coverage = snapshot.family_coverage.clone();
    let covered_family_count = family_coverage
        .iter()
        .filter(|item| item.status == CorrelationCoverageStatusDto::Covered)
        .count() as u32;
    let high_confidence_family_count = family_coverage
        .iter()
        .filter(|item| item.high_confidence_lead_count > 0)
        .count() as u32;

    Ok(CorrelationRuntimeSnapshot {
        snapshot_available: true,
        lead_count: snapshot.lead_count,
        high_confidence_lead_count,
        review_lead_count,
        cluster_count: snapshot.cluster_count,
        rule_family_count: family_coverage.len() as u32,
        covered_family_count,
        high_confidence_family_count,
        family_coverage,
    })
}

pub(crate) struct SecurityAuditSnapshot {
    pub(crate) audit_log_required: bool,
    pub(crate) audit_event_count: u32,
    pub(crate) sensitive_audit_event_count: u32,
    pub(crate) recent_audit_entries: Vec<SecurityAuditEntryDto>,
}

pub(crate) fn security_audit_snapshot(
    conn: &Connection,
    case_id: &str,
) -> Result<SecurityAuditSnapshot, GovernanceError> {
    let repo = AuditRepo::new(conn);
    let entries = repo.query(Some(case_id), None, 5, 0)?;
    let audit_event_count = repo.count(Some(case_id))? as u32;

    let recent_audit_entries = entries
        .iter()
        .map(|entry| SecurityAuditEntryDto {
            action: entry.action.clone(),
            resource_type: entry.resource_type.clone(),
            resource_id: entry.resource_id.clone(),
            created_at: entry.created_at.clone(),
            summary: audit_summary(entry),
            sensitive: is_sensitive_audit_entry(&entry.action),
        })
        .collect::<Vec<_>>();
    let sensitive_audit_event_count = entries
        .iter()
        .filter(|entry| is_sensitive_audit_entry(&entry.action))
        .count() as u32;

    Ok(SecurityAuditSnapshot {
        audit_log_required: true,
        audit_event_count,
        sensitive_audit_event_count,
        recent_audit_entries,
    })
}

pub(crate) fn is_sensitive_audit_entry(action: &str) -> bool {
    matches!(
        action,
        "file.extract"
            | "mcp.connect"
            | "mcp.disconnect"
            | "mcp.test"
            | "mcp.resource.list"
            | "mcp.resource.read"
            | "mcp.tool.list"
            | "mcp.tool.call"
            | "mcp.prompt.list"
            | "mcp.prompt.get"
            | "report.export"
    )
}

pub(crate) fn audit_summary(
    entry: &persistence_sqlite::repositories::audit_repo::AuditLogEntry,
) -> Option<String> {
    let details: serde_json::Value = serde_json::from_str(&entry.details).ok()?;
    if let Some(status) = details.get("status").and_then(|value| value.as_str()) {
        if let Some(tool_name) = details.get("toolName").and_then(|value| value.as_str()) {
            return Some(format!("status={status}; toolName={tool_name}"));
        }
        if let Some(file_name) = details
            .get("destinationFileName")
            .and_then(|value| value.as_str())
        {
            return Some(format!("status={status}; destinationFileName={file_name}"));
        }
        return Some(format!("status={status}"));
    }
    if let Some(prompt_name) = details.get("promptName").and_then(|value| value.as_str()) {
        return Some(format!("promptName={prompt_name}"));
    }
    if let Some(server_id) = details.get("serverId").and_then(|value| value.as_str()) {
        return Some(format!("serverId={server_id}"));
    }
    None
}
