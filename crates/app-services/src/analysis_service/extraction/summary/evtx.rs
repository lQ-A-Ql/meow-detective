use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::extraction::artifact_query::{
    count_artifacts_by_type, query_artifact_rows, status_from_total,
};
use crate::analysis_service::extraction::attr_mapping::{
    details_attr, optional_string_attr, optional_u64_attr, string_attr, u32_attr,
};
use chrono::Utc;
use rusqlite::Connection;
use transport::dto::{
    EvtxApplicationEventDto, EvtxBootEventDto, EvtxEventSummaryDto, EvtxSecurityEventDto,
};

pub fn get_evtx_event_summary(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<EvtxEventSummaryDto, AnalysisServiceError> {
    let boot_shutdown_count = count_artifacts_by_type(conn, "EvtxBootShutdown")?;
    let security_count = count_artifacts_by_type(conn, "EvtxSecurityEvent")?;
    let application_count = count_artifacts_by_type(conn, "EvtxApplicationEvent")?;
    let boot_rows = query_artifact_rows(conn, &["EvtxBootShutdown"], offset, limit)?;
    let security_rows = query_artifact_rows(conn, &["EvtxSecurityEvent"], offset, limit)?;
    let application_rows = query_artifact_rows(conn, &["EvtxApplicationEvent"], offset, limit)?;

    let boot_events = boot_rows
        .into_iter()
        .map(|row| EvtxBootEventDto {
            timestamp: string_attr(&row.attrs, "timestamp"),
            event_id: u32_attr(&row.attrs, "eventId"),
            record_id: optional_u64_attr(&row.attrs, "recordId"),
            provider: optional_string_attr(&row.attrs, "provider"),
            kind: string_attr(&row.attrs, "eventKind"),
            source_path: string_attr(&row.attrs, "sourcePath"),
            note: string_attr(&row.attrs, "note"),
            details: details_attr(&row.attrs, "details"),
        })
        .collect::<Vec<_>>();

    let security_events = security_rows
        .into_iter()
        .map(|row| EvtxSecurityEventDto {
            timestamp: string_attr(&row.attrs, "timestamp"),
            event_id: u32_attr(&row.attrs, "eventId"),
            record_id: optional_u64_attr(&row.attrs, "recordId"),
            provider: optional_string_attr(&row.attrs, "provider"),
            kind: string_attr(&row.attrs, "kind"),
            source_path: string_attr(&row.attrs, "sourcePath"),
            target_user: optional_string_attr(&row.attrs, "targetUser"),
            subject_user: optional_string_attr(&row.attrs, "subjectUser"),
            logon_type: optional_string_attr(&row.attrs, "logonType"),
            ip_address: optional_string_attr(&row.attrs, "ipAddress"),
            workstation: optional_string_attr(&row.attrs, "workstation"),
            failure_reason: optional_string_attr(&row.attrs, "failureReason"),
            process_name: optional_string_attr(&row.attrs, "processName"),
            parent_process_name: optional_string_attr(&row.attrs, "parentProcessName"),
            task_name: optional_string_attr(&row.attrs, "taskName"),
            privilege_list: optional_string_attr(&row.attrs, "privilegeList"),
            member_name: optional_string_attr(&row.attrs, "memberName"),
            details: details_attr(&row.attrs, "details"),
        })
        .collect::<Vec<_>>();

    let application_events = application_rows
        .into_iter()
        .map(|row| EvtxApplicationEventDto {
            timestamp: string_attr(&row.attrs, "timestamp"),
            event_id: u32_attr(&row.attrs, "eventId"),
            record_id: optional_u64_attr(&row.attrs, "recordId"),
            provider: optional_string_attr(&row.attrs, "provider"),
            kind: string_attr(&row.attrs, "kind"),
            source_path: string_attr(&row.attrs, "sourcePath"),
            application: optional_string_attr(&row.attrs, "application"),
            fault_module: optional_string_attr(&row.attrs, "faultModule"),
            product_name: optional_string_attr(&row.attrs, "productName"),
            manufacturer: optional_string_attr(&row.attrs, "manufacturer"),
            details: details_attr(&row.attrs, "details"),
        })
        .collect::<Vec<_>>();

    let total_count = boot_shutdown_count + security_count + application_count;
    let logon_logoff_count = security_events
        .iter()
        .filter(|event| matches!(event.kind.as_str(), "logonSuccess" | "logonFailure"))
        .count() as u64;
    let privilege_escalation_count = security_events
        .iter()
        .filter(|event| event.kind == "explicitCredentials")
        .count() as u64;
    let process_execution_count = security_events
        .iter()
        .filter(|event| event.kind == "processCreated")
        .count() as u64;
    let account_management_count = security_events
        .iter()
        .filter(|event| matches!(event.kind.as_str(), "accountCreated" | "groupMemberAdded"))
        .count() as u64;
    let scheduled_task_count = security_events
        .iter()
        .filter(|event| {
            matches!(
                event.kind.as_str(),
                "scheduledTaskCreated" | "scheduledTaskModified"
            )
        })
        .count() as u64;
    let application_crash_count = application_events
        .iter()
        .filter(|event| {
            matches!(
                event.kind.as_str(),
                "applicationCrash" | "applicationHang" | "windowsErrorReporting"
            )
        })
        .count() as u64;
    let software_installation_count = application_events
        .iter()
        .filter(|event| event.kind == "softwareInstallation")
        .count() as u64;
    let other_count = security_count
        .saturating_sub(logon_logoff_count)
        .saturating_sub(privilege_escalation_count)
        .saturating_sub(process_execution_count)
        .saturating_sub(account_management_count)
        .saturating_sub(scheduled_task_count)
        + application_count
            .saturating_sub(application_crash_count)
            .saturating_sub(software_installation_count);

    Ok(EvtxEventSummaryDto {
        status: status_from_total(total_count),
        boot_shutdown_count,
        logon_logoff_count,
        privilege_escalation_count,
        process_execution_count,
        account_management_count,
        scheduled_task_count,
        application_crash_count,
        software_installation_count,
        other_count,
        total_count,
        boot_events,
        security_events,
        application_events,
        generated_at: Utc::now().to_rfc3339(),
        warnings: Vec::new(),
    })
}
