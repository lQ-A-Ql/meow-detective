use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvtxBootEventKind {
    OperatingSystemStarted,
    OperatingSystemShutdown,
    EventLogStarted,
    EventLogStopped,
    UnexpectedShutdown,
    PlannedShutdown,
    PowerShellScriptBlock,
    SysmonProcessCreate,
    RdpSessionConnect,
    DefenderThreatDetected,
}

impl EvtxBootEventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OperatingSystemStarted => "operatingSystemStarted",
            Self::OperatingSystemShutdown => "operatingSystemShutdown",
            Self::EventLogStarted => "eventLogStarted",
            Self::EventLogStopped => "eventLogStopped",
            Self::UnexpectedShutdown => "unexpectedShutdown",
            Self::PlannedShutdown => "plannedShutdown",
            Self::PowerShellScriptBlock => "powershellScriptBlock",
            Self::SysmonProcessCreate => "sysmonProcessCreate",
            Self::RdpSessionConnect => "rdpSessionConnect",
            Self::DefenderThreatDetected => "defenderThreatDetected",
        }
    }

    pub(super) fn note(&self) -> &'static str {
        match self {
            Self::OperatingSystemStarted => {
                "Kernel-General 12 candidate; indicates Windows completed the operating-system startup phase."
            }
            Self::OperatingSystemShutdown => {
                "Kernel-General 13 candidate; indicates Windows entered the operating-system shutdown phase."
            }
            Self::EventLogStarted => {
                "EventLog 6005 candidate; indicates the Event Log service started, not a direct boot assertion."
            }
            Self::EventLogStopped => {
                "EventLog 6006 candidate; indicates the Event Log service stopped, not a direct shutdown assertion."
            }
            Self::UnexpectedShutdown => {
                "EventLog 6008 candidate; indicates an unexpected prior shutdown reported by Windows."
            }
            Self::PlannedShutdown => {
                "User32 1074 candidate; indicates a planned shutdown or restart event."
            }
            Self::PowerShellScriptBlock => {
                "PowerShell 4104 candidate; script block logging content."
            }
            Self::SysmonProcessCreate => "Sysmon 1 candidate; process creation event.",
            Self::RdpSessionConnect => {
                "TerminalServices 21 candidate; remote desktop session logon."
            }
            Self::DefenderThreatDetected => {
                "Defender 1116 candidate; malware or threat detected."
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvtxBootEvent {
    pub timestamp: String,
    pub event_id: u32,
    pub record_id: Option<u64>,
    pub provider: Option<String>,
    pub kind: EvtxBootEventKind,
    pub source_path: String,
    pub note: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub details: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvtxBootExtraction {
    pub events: Vec<EvtxBootEvent>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvtxEventCategory {
    BootShutdown,
    LogonLogoff,
    PrivilegeEscalation,
    ProcessExecution,
    AccountManagement,
    ScheduledTask,
    ApplicationCrash,
    SoftwareInstallation,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvtxSecurityEventKind {
    LogonSuccess,
    LogonFailure,
    ExplicitCredentials,
    ProcessCreated,
    ScheduledTaskCreated,
    ScheduledTaskModified,
    AccountCreated,
    GroupMemberAdded,
}

impl EvtxSecurityEventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LogonSuccess => "logonSuccess",
            Self::LogonFailure => "logonFailure",
            Self::ExplicitCredentials => "explicitCredentials",
            Self::ProcessCreated => "processCreated",
            Self::ScheduledTaskCreated => "scheduledTaskCreated",
            Self::ScheduledTaskModified => "scheduledTaskModified",
            Self::AccountCreated => "accountCreated",
            Self::GroupMemberAdded => "groupMemberAdded",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvtxApplicationEventKind {
    ApplicationCrash,
    ApplicationHang,
    WindowsErrorReporting,
    SoftwareInstallation,
}

impl EvtxApplicationEventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ApplicationCrash => "applicationCrash",
            Self::ApplicationHang => "applicationHang",
            Self::WindowsErrorReporting => "windowsErrorReporting",
            Self::SoftwareInstallation => "softwareInstallation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvtxSecurityEvent {
    pub timestamp: String,
    pub event_id: u32,
    pub record_id: Option<u64>,
    pub provider: Option<String>,
    pub kind: EvtxSecurityEventKind,
    pub source_path: String,
    pub target_user: Option<String>,
    pub subject_user: Option<String>,
    pub logon_type: Option<String>,
    pub ip_address: Option<String>,
    pub workstation: Option<String>,
    pub failure_reason: Option<String>,
    pub process_name: Option<String>,
    pub parent_process_name: Option<String>,
    pub task_name: Option<String>,
    pub privilege_list: Option<String>,
    pub member_name: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub details: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvtxApplicationEvent {
    pub timestamp: String,
    pub event_id: u32,
    pub record_id: Option<u64>,
    pub provider: Option<String>,
    pub kind: EvtxApplicationEventKind,
    pub source_path: String,
    pub application: Option<String>,
    pub fault_module: Option<String>,
    pub product_name: Option<String>,
    pub manufacturer: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub details: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvtxStructuredExtraction {
    pub boot_events: Vec<EvtxBootEvent>,
    pub security_events: Vec<EvtxSecurityEvent>,
    pub application_events: Vec<EvtxApplicationEvent>,
    pub warnings: Vec<String>,
}
