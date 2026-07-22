use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::extraction::artifact_query::{
    count_artifacts_by_type, query_evtx_artifact_rows, query_evtx_artifact_rows_by_kinds,
    status_from_total, AnalysisArtifactRow,
};
use crate::analysis_service::extraction::attr_mapping::{
    details_attr, optional_string_attr, optional_u64_attr, string_attr, u32_attr,
};
use chrono::Utc;
use rusqlite::Connection;
use transport::dto::{
    EvtxApplicationEventDto, EvtxBootEventDto, EvtxEventSummaryDto, EvtxEventViewDto,
    EvtxSecurityEventDto,
};

pub fn get_evtx_event_summary(
    conn: &Connection,
    view: Option<EvtxEventViewDto>,
    offset: u64,
    limit: u32,
) -> Result<EvtxEventSummaryDto, AnalysisServiceError> {
    let counts = EvtxCounts::load(conn)?;
    let metrics = EvtxMetrics::load(conn, &counts)?;
    let boot_events = if view.is_none() || view == Some(EvtxEventViewDto::Boot) {
        map_boot_events(query_evtx_artifact_rows(
            conn,
            &["EvtxBootShutdown"],
            offset,
            limit,
        )?)
    } else {
        Vec::new()
    };
    let security_events = query_security_view(conn, view, offset, limit)?;
    let application_events = if view.is_none() || view == Some(EvtxEventViewDto::Application) {
        map_application_events(query_evtx_artifact_rows(
            conn,
            &["EvtxApplicationEvent"],
            offset,
            limit,
        )?)
    } else {
        Vec::new()
    };
    let page_total = match view {
        Some(EvtxEventViewDto::Boot) => counts.boot_shutdown,
        Some(EvtxEventViewDto::Logon) => metrics.logon_logoff + metrics.privilege_escalation,
        Some(EvtxEventViewDto::Process) => metrics.process_execution,
        Some(EvtxEventViewDto::Account) => metrics.account_management + metrics.scheduled_task,
        Some(EvtxEventViewDto::Application) => counts.application,
        None => counts.total(),
    };
    Ok(EvtxEventSummaryDto {
        status: status_from_total(counts.total()),
        page_total,
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

fn query_security_view(
    conn: &Connection,
    view: Option<EvtxEventViewDto>,
    offset: u64,
    limit: u32,
) -> Result<Vec<EvtxSecurityEventDto>, AnalysisServiceError> {
    let kinds: Option<&[&str]> = match view {
        Some(EvtxEventViewDto::Logon) => {
            Some(&["logonSuccess", "logonFailure", "explicitCredentials"])
        }
        Some(EvtxEventViewDto::Process) => Some(&["processCreated"]),
        Some(EvtxEventViewDto::Account) => Some(&[
            "scheduledTaskCreated",
            "scheduledTaskModified",
            "accountCreated",
            "groupMemberAdded",
        ]),
        None => None,
        Some(EvtxEventViewDto::Boot | EvtxEventViewDto::Application) => return Ok(Vec::new()),
    };
    let rows = match kinds {
        Some(kinds) => {
            query_evtx_artifact_rows_by_kinds(conn, "EvtxSecurityEvent", kinds, offset, limit)?
        }
        None => query_evtx_artifact_rows(conn, &["EvtxSecurityEvent"], offset, limit)?,
    };
    Ok(map_security_events(rows))
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

#[derive(Default)]
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
    fn load(conn: &Connection, counts: &EvtxCounts) -> Result<Self, AnalysisServiceError> {
        let mut metrics = Self::default();
        let mut stmt = conn.prepare(
            "SELECT artifact_type, COALESCE(json_extract(attrs, '$.kind'), ''), COUNT(*)
             FROM artifacts
             WHERE artifact_type IN ('EvtxSecurityEvent', 'EvtxApplicationEvent')
             GROUP BY artifact_type, COALESCE(json_extract(attrs, '$.kind'), '')",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, u64>(2)?,
            ))
        })?;
        for row in rows {
            let (artifact_type, kind, count) = row?;
            metrics.add_kind_count(&artifact_type, &kind, count);
        }
        let other = counts
            .security
            .saturating_sub(metrics.logon_logoff)
            .saturating_sub(metrics.privilege_escalation)
            .saturating_sub(metrics.process_execution)
            .saturating_sub(metrics.account_management)
            .saturating_sub(metrics.scheduled_task)
            + counts
                .application
                .saturating_sub(metrics.application_crash)
                .saturating_sub(metrics.software_installation);
        metrics.other = other;
        Ok(metrics)
    }

    fn add_kind_count(&mut self, artifact_type: &str, kind: &str, count: u64) {
        match (artifact_type, kind) {
            ("EvtxSecurityEvent", "logonSuccess" | "logonFailure") => {
                self.logon_logoff += count;
            }
            ("EvtxSecurityEvent", "explicitCredentials") => self.privilege_escalation += count,
            ("EvtxSecurityEvent", "processCreated") => self.process_execution += count,
            ("EvtxSecurityEvent", "accountCreated" | "groupMemberAdded") => {
                self.account_management += count;
            }
            ("EvtxSecurityEvent", "scheduledTaskCreated" | "scheduledTaskModified") => {
                self.scheduled_task += count;
            }
            (
                "EvtxApplicationEvent",
                "applicationCrash" | "applicationHang" | "windowsErrorReporting",
            ) => self.application_crash += count,
            ("EvtxApplicationEvent", "softwareInstallation") => {
                self.software_installation += count;
            }
            _ => {}
        }
    }
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
