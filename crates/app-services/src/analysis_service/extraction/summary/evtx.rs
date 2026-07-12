use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::extraction::artifact_query::{
    count_artifacts_by_type, query_artifact_rows, status_from_total, AnalysisArtifactRow,
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
    let counts = EvtxCounts::load(conn)?;
    let boot_events = map_boot_events(query_artifact_rows(
        conn,
        &["EvtxBootShutdown"],
        offset,
        limit,
    )?);
    let security_events = map_security_events(query_artifact_rows(
        conn,
        &["EvtxSecurityEvent"],
        offset,
        limit,
    )?);
    let application_events = map_application_events(query_artifact_rows(
        conn,
        &["EvtxApplicationEvent"],
        offset,
        limit,
    )?);
    let metrics = EvtxMetrics::from_events(&counts, &security_events, &application_events);
    Ok(EvtxEventSummaryDto {
        status: status_from_total(counts.total()),
        boot_shutdown_count: counts.boot_shutdown,
        logon_logoff_count: metrics.logon_logoff,
        privilege_escalation_count: metrics.privilege_escalation,
        process_execution_count: metrics.process_execution,
        account_management_count: metrics.account_management,
        scheduled_task_count: metrics.scheduled_task,
        application_crash_count: metrics.application_crash,
        software_installation_count: metrics.software_installation,
        other_count: metrics.other,
        total_count: counts.total(),
        boot_events,
        security_events,
        application_events,
        generated_at: Utc::now().to_rfc3339(),
        warnings: Vec::new(),
    })
}

struct EvtxCounts {
    boot_shutdown: u64,
    security: u64,
    application: u64,
}

impl EvtxCounts {
    fn load(conn: &Connection) -> Result<Self, AnalysisServiceError> {
        Ok(Self {
            boot_shutdown: count_artifacts_by_type(conn, "EvtxBootShutdown")?,
            security: count_artifacts_by_type(conn, "EvtxSecurityEvent")?,
            application: count_artifacts_by_type(conn, "EvtxApplicationEvent")?,
        })
    }

    fn total(&self) -> u64 {
        self.boot_shutdown + self.security + self.application
    }
}

struct EvtxMetrics {
    logon_logoff: u64,
    privilege_escalation: u64,
    process_execution: u64,
    account_management: u64,
    scheduled_task: u64,
    application_crash: u64,
    software_installation: u64,
    other: u64,
}

impl EvtxMetrics {
    fn from_events(
        counts: &EvtxCounts,
        security_events: &[EvtxSecurityEventDto],
        application_events: &[EvtxApplicationEventDto],
    ) -> Self {
        let logon_logoff = count_security_kinds(security_events, &["logonSuccess", "logonFailure"]);
        let privilege_escalation = count_security_kinds(security_events, &["explicitCredentials"]);
        let process_execution = count_security_kinds(security_events, &["processCreated"]);
        let account_management =
            count_security_kinds(security_events, &["accountCreated", "groupMemberAdded"]);
        let scheduled_task = count_security_kinds(
            security_events,
            &["scheduledTaskCreated", "scheduledTaskModified"],
        );
        let application_crash = count_application_kinds(
            application_events,
            &[
                "applicationCrash",
                "applicationHang",
                "windowsErrorReporting",
            ],
        );
        let software_installation =
            count_application_kinds(application_events, &["softwareInstallation"]);
        let other = counts
            .security
            .saturating_sub(logon_logoff)
            .saturating_sub(privilege_escalation)
            .saturating_sub(process_execution)
            .saturating_sub(account_management)
            .saturating_sub(scheduled_task)
            + counts
                .application
                .saturating_sub(application_crash)
                .saturating_sub(software_installation);
        Self {
            logon_logoff,
            privilege_escalation,
            process_execution,
            account_management,
            scheduled_task,
            application_crash,
            software_installation,
            other,
        }
    }
}

fn count_security_kinds(events: &[EvtxSecurityEventDto], kinds: &[&str]) -> u64 {
    events
        .iter()
        .filter(|event| kinds.contains(&event.kind.as_str()))
        .count() as u64
}

fn count_application_kinds(events: &[EvtxApplicationEventDto], kinds: &[&str]) -> u64 {
    events
        .iter()
        .filter(|event| kinds.contains(&event.kind.as_str()))
        .count() as u64
}

fn map_boot_events(rows: Vec<AnalysisArtifactRow>) -> Vec<EvtxBootEventDto> {
    rows.into_iter()
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
        .collect()
}

fn map_security_events(rows: Vec<AnalysisArtifactRow>) -> Vec<EvtxSecurityEventDto> {
    rows.into_iter()
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
        .collect()
}

fn map_application_events(rows: Vec<AnalysisArtifactRow>) -> Vec<EvtxApplicationEventDto> {
    rows.into_iter()
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
        .collect()
}
