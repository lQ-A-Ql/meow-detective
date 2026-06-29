//! Bounded EVTX candidate extraction.
//!
//! This is intentionally not a general event log platform. It extracts a
//! targeted set of events from supported EVTX channels that can be shown as
//! candidates with provenance.
//!
//! Supported channels and event IDs:
//! - System.evtx: 6005, 6006, 6008, 1074 (boot/shutdown)
//! - Security.evtx, Application.evtx: channel-aware extraction
//! - PowerShell/Operational: 4104 (script block logging)
//! - Sysmon/Operational: 1 (process creation)
//! - TerminalServices-LocalSessionManager/Operational: 21 (RDP session)
//! - Windows Defender/Operational: 1116 (threat detection)

use super::capability::supports_evtx_boot_shutdown_path;
use super::error::EvtxBootError;
use chrono::{DateTime, Utc};
use evtx::{err::EvtxError, EvtxParser};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_EVTX_ANALYSIS_BYTES: usize = 16 * 1024 * 1024;
const EVTX_FILE_HEADER_SIZE: u64 = 4096;
const EVTX_CHUNK_SIZE: u64 = 65536;
const MAX_EVTX_WARNINGS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvtxBootEventKind {
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

    fn note(&self) -> &'static str {
        match self {
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
            Self::SysmonProcessCreate => {
                "Sysmon 1 candidate; process creation event."
            }
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
    /// All `<EventData><Data Name="X">value</Data></EventData>` pairs extracted
    /// from the event XML.  The map is empty for events without EventData.
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
    /// All `<EventData><Data Name="X">value</Data></EventData>` pairs extracted
    /// from the event XML.
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
    /// All `<EventData><Data Name="X">value</Data></EventData>` pairs extracted
    /// from the event XML.
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

pub fn extract_boot_shutdown_events(
    bytes: &[u8],
    source_path: &str,
) -> Result<EvtxBootExtraction, EvtxBootError> {
    let structured = extract_structured_events(bytes, source_path)?;
    Ok(EvtxBootExtraction {
        events: structured.boot_events,
        warnings: structured.warnings,
    })
}

pub fn extract_boot_shutdown_events_from_json_records(
    records: &[Value],
    source_path: &str,
) -> Result<EvtxBootExtraction, EvtxBootError> {
    let structured = extract_structured_events_from_json_records(records, source_path)?;
    Ok(EvtxBootExtraction {
        events: structured.boot_events,
        warnings: structured.warnings,
    })
}

pub fn extract_structured_events(
    bytes: &[u8],
    source_path: &str,
) -> Result<EvtxStructuredExtraction, EvtxBootError> {
    if !supports_evtx_boot_shutdown_path(source_path) {
        return Err(EvtxBootError::UnsupportedPath {
            path: source_path.to_string(),
        });
    }

    if bytes.len() > MAX_EVTX_ANALYSIS_BYTES {
        return Err(EvtxBootError::InputTooLarge {
            path: source_path.to_string(),
            size: bytes.len(),
            max: MAX_EVTX_ANALYSIS_BYTES,
        });
    }

    let parser_bytes = bounded_clean_evtx_bytes(bytes);
    let mut parser = EvtxParser::from_buffer(parser_bytes.to_vec()).map_err(|err| {
        EvtxBootError::ParserInit {
            path: source_path.to_string(),
            detail: err.to_string(),
        }
    })?;

    let mut raw_warnings = Vec::new();
    let mut extraction = EvtxStructuredExtraction::default();
    for record in parser.records_json_value() {
        match record {
            Ok(record) => {
                structured_event_from_json(
                    &record.data,
                    Some(record.event_record_id),
                    Some(record.timestamp.to_string()),
                    source_path,
                    &mut extraction,
                );
            }
            Err(err) => raw_warnings.push(format_evtx_warning(source_path, &err)),
        }
    }

    extraction.warnings = govern_evtx_warnings(source_path, raw_warnings);
    Ok(extraction)
}

pub fn extract_structured_events_from_json_records(
    records: &[Value],
    source_path: &str,
) -> Result<EvtxStructuredExtraction, EvtxBootError> {
    if !supports_evtx_boot_shutdown_path(source_path) {
        return Err(EvtxBootError::UnsupportedPath {
            path: source_path.to_string(),
        });
    }

    let mut extraction = EvtxStructuredExtraction::default();
    for record in records {
        structured_event_from_json(record, None, None, source_path, &mut extraction);
    }
    extraction.warnings = govern_evtx_warnings(source_path, Vec::new());
    Ok(extraction)
}

fn bounded_clean_evtx_bytes(bytes: &[u8]) -> &[u8] {
    if bytes.len() < EVTX_FILE_HEADER_SIZE as usize + 128 || !bytes.starts_with(b"ElfFile\0") {
        return bytes;
    }

    let chunk_count = u16::from_le_bytes(bytes[42..44].try_into().unwrap_or([0; 2])) as usize;
    let flags = u32::from_le_bytes(bytes[120..124].try_into().unwrap_or([0; 4]));
    let is_dirty = flags & 0x1 != 0;
    if is_dirty || chunk_count == 0 {
        return bytes;
    }

    let declared_len = (EVTX_FILE_HEADER_SIZE as usize)
        .saturating_add(chunk_count.saturating_mul(EVTX_CHUNK_SIZE as usize));
    if declared_len > EVTX_FILE_HEADER_SIZE as usize && declared_len < bytes.len() {
        &bytes[..declared_len]
    } else {
        bytes
    }
}

fn format_evtx_warning(source_path: &str, err: &EvtxError) -> String {
    match err {
        EvtxError::FailedToParseChunk { chunk_id, source } => {
            let offset = EVTX_FILE_HEADER_SIZE + (*chunk_id).saturating_mul(EVTX_CHUNK_SIZE);
            EvtxBootError::ChunkParse {
                path: source_path.to_string(),
                chunk_id: *chunk_id,
                offset,
                detail: source.to_string(),
            }
            .to_string()
        }
        EvtxError::FailedToParseRecord { record_id, source } => EvtxBootError::RecordParse {
            path: source_path.to_string(),
            record_id: Some(*record_id),
            detail: source.to_string(),
        }
        .to_string(),
        other => EvtxBootError::RecordParse {
            path: source_path.to_string(),
            record_id: None,
            detail: other.to_string(),
        }
        .to_string(),
    }
}

fn govern_evtx_warnings(path: &str, raw: Vec<String>) -> Vec<String> {
    let sanitized = sanitize_evtx_path(path);
    let mut seen = BTreeSet::new();
    let mut governed = Vec::with_capacity(raw.len().min(MAX_EVTX_WARNINGS));
    for message in raw {
        let code = evtx_warning_code_for(&message);
        let entry = format!("[{code}] {sanitized}: {message}");
        if !seen.insert(entry.clone()) {
            continue;
        }
        if governed.len() >= MAX_EVTX_WARNINGS {
            let cap = format!("[EVTX-WARN-CAP] {sanitized}: additional EVTX warnings suppressed");
            if seen.insert(cap.clone()) {
                governed.push(cap);
            }
            break;
        }
        governed.push(entry);
    }
    governed
}

fn evtx_warning_code_for(message: &str) -> &'static str {
    if message.contains("parser initialization failed") {
        "EVTX-INIT"
    } else if message.contains("chunk parse warning") {
        "EVTX-CHUNK"
    } else if message.contains("record parse warning") {
        "EVTX-RECORD"
    } else if message.contains("no supported") {
        "EVTX-EMPTY"
    } else if message.contains("exceeds bounded EVTX parser limit") {
        "EVTX-LIMIT"
    } else if message.contains("outside bounded") {
        "EVTX-SCOPE"
    } else {
        "EVTX-WARN"
    }
}

fn sanitize_evtx_path(path: &str) -> String {
    path.replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .to_string()
}

fn structured_event_from_json(
    record: &Value,
    fallback_record_id: Option<u64>,
    fallback_timestamp: Option<String>,
    source_path: &str,
    extraction: &mut EvtxStructuredExtraction,
) {
    let wrapper = record.get("Event").unwrap_or(record);
    let Some(system) = wrapper.get("System") else {
        return;
    };
    let Some(event_id_value) = system.get("EventID") else {
        return;
    };
    let Some(event_id) = event_id(event_id_value) else {
        return;
    };
    let provider = provider_name(system);
    let channel = event_channel(system);
    let timestamp = event_timestamp(system)
        .or(fallback_timestamp)
        .unwrap_or_else(|| "unknown".to_string());
    let record_id = event_record_id(system).or(fallback_record_id);

    match classify_event(event_id, provider.as_deref(), channel.as_deref()) {
        EventClass::Boot(kind) => {
            let note = kind.note().to_string();
            let details = event_data_map(wrapper);
            extraction.boot_events.push(EvtxBootEvent {
                timestamp,
                event_id,
                record_id,
                provider,
                kind,
                source_path: source_path.to_string(),
                note,
                details,
            });
        }
        EventClass::Security(kind) => {
            if let Some(event) = security_event_from_json(
                wrapper,
                kind,
                timestamp,
                event_id,
                record_id,
                provider,
                source_path,
            ) {
                extraction.security_events.push(event);
            }
        }
        EventClass::Application(kind) => {
            if let Some(event) = application_event_from_json(
                wrapper,
                kind,
                timestamp,
                event_id,
                record_id,
                provider,
                source_path,
            ) {
                extraction.application_events.push(event);
            }
        }
        EventClass::Ignore => {}
    }
}

enum EventClass {
    Boot(EvtxBootEventKind),
    Security(EvtxSecurityEventKind),
    Application(EvtxApplicationEventKind),
    Ignore,
}

/// Classify an event by ID with optional provider/channel filtering for ambiguous IDs.
fn classify_event(event_id: u32, provider: Option<&str>, channel: Option<&str>) -> EventClass {
    let channel = channel.unwrap_or("");
    match event_id {
        6005 => EventClass::Boot(EvtxBootEventKind::EventLogStarted),
        6006 => EventClass::Boot(EvtxBootEventKind::EventLogStopped),
        6008 => EventClass::Boot(EvtxBootEventKind::UnexpectedShutdown),
        1074 => EventClass::Boot(EvtxBootEventKind::PlannedShutdown),
        4104 if provider_matches(provider, "microsoft-windows-powershell") => {
            EventClass::Boot(EvtxBootEventKind::PowerShellScriptBlock)
        }
        1 if provider_matches(provider, "microsoft-windows-sysmon") => {
            EventClass::Boot(EvtxBootEventKind::SysmonProcessCreate)
        }
        21 if provider_matches(
            provider,
            "microsoft-windows-terminalservices-localsessionmanager",
        ) =>
        {
            EventClass::Boot(EvtxBootEventKind::RdpSessionConnect)
        }
        1116 if provider_matches(provider, "microsoft-windows-windows defender")
            || provider_matches(provider, "Microsoft-Windows-Windows Defender") =>
        {
            EventClass::Boot(EvtxBootEventKind::DefenderThreatDetected)
        }
        4624 if channel.eq_ignore_ascii_case("security") => {
            EventClass::Security(EvtxSecurityEventKind::LogonSuccess)
        }
        4625 if channel.eq_ignore_ascii_case("security") => {
            EventClass::Security(EvtxSecurityEventKind::LogonFailure)
        }
        4648 if channel.eq_ignore_ascii_case("security") => {
            EventClass::Security(EvtxSecurityEventKind::ExplicitCredentials)
        }
        4688 if channel.eq_ignore_ascii_case("security") => {
            EventClass::Security(EvtxSecurityEventKind::ProcessCreated)
        }
        4698 if channel.eq_ignore_ascii_case("security") => {
            EventClass::Security(EvtxSecurityEventKind::ScheduledTaskCreated)
        }
        4702 if channel.eq_ignore_ascii_case("security") => {
            EventClass::Security(EvtxSecurityEventKind::ScheduledTaskModified)
        }
        4720 if channel.eq_ignore_ascii_case("security") => {
            EventClass::Security(EvtxSecurityEventKind::AccountCreated)
        }
        4732 if channel.eq_ignore_ascii_case("security") => {
            EventClass::Security(EvtxSecurityEventKind::GroupMemberAdded)
        }
        1000..=1002 if channel.eq_ignore_ascii_case("application") => {
            EventClass::Application(match event_id {
                1000 => EvtxApplicationEventKind::ApplicationCrash,
                1001 => EvtxApplicationEventKind::WindowsErrorReporting,
                _ => EvtxApplicationEventKind::ApplicationHang,
            })
        }
        1033 | 11707 | 11708 if channel.eq_ignore_ascii_case("application") => {
            EventClass::Application(EvtxApplicationEventKind::SoftwareInstallation)
        }
        _ => EventClass::Ignore,
    }
}

fn security_event_from_json(
    wrapper: &Value,
    kind: EvtxSecurityEventKind,
    timestamp: String,
    event_id: u32,
    record_id: Option<u64>,
    provider: Option<String>,
    source_path: &str,
) -> Option<EvtxSecurityEvent> {
    let details = event_data_map(wrapper);
    Some(EvtxSecurityEvent {
        timestamp,
        event_id,
        record_id,
        provider,
        kind,
        source_path: source_path.to_string(),
        target_user: details.get("TargetUserName").cloned(),
        subject_user: details.get("SubjectUserName").cloned(),
        logon_type: details.get("LogonType").cloned(),
        ip_address: details.get("IpAddress").cloned(),
        workstation: details.get("WorkstationName").cloned(),
        failure_reason: details.get("Status").cloned(),
        process_name: details.get("NewProcessName").cloned(),
        parent_process_name: details
            .get("ParentProcessName")
            .or_else(|| details.get("CreatorProcessName"))
            .cloned(),
        task_name: details.get("TaskName").cloned(),
        privilege_list: details.get("PrivilegeList").cloned(),
        member_name: details.get("MemberName").cloned(),
        details,
    })
}

fn application_event_from_json(
    wrapper: &Value,
    kind: EvtxApplicationEventKind,
    timestamp: String,
    event_id: u32,
    record_id: Option<u64>,
    provider: Option<String>,
    source_path: &str,
) -> Option<EvtxApplicationEvent> {
    let details = event_data_map(wrapper);
    let mut application = details
        .get("AppName")
        .or_else(|| details.get("P1"))
        .cloned();
    let mut fault_module = details
        .get("ModuleName")
        .or_else(|| details.get("P4"))
        .cloned();
    let mut product_name = details.get("ProductName").cloned();
    let mut manufacturer = details.get("Manufacturer").cloned();

    // Older Application channel events (and some WER records) use unnamed
    // `<Data>` elements. Fall back to well-known positional indices when the
    // named keys above are absent.
    if application.is_none()
        || fault_module.is_none()
        || product_name.is_none()
        || manufacturer.is_none()
    {
        let values = event_data_values(wrapper);
        match event_id {
            1000 => {
                application = application.or_else(|| values.first().cloned());
                fault_module = fault_module.or_else(|| values.get(3).cloned());
            }
            1001 => {
                application = application.or_else(|| values.get(5).cloned());
                fault_module = fault_module.or_else(|| values.get(8).cloned());
            }
            1002 => {
                application = application.or_else(|| values.first().cloned());
            }
            1033 | 11707 | 11708 => {
                product_name = product_name.or_else(|| values.first().cloned());
                manufacturer = manufacturer.or_else(|| values.get(4).cloned());
            }
            _ => {}
        }
    }

    Some(EvtxApplicationEvent {
        timestamp,
        event_id,
        record_id,
        provider,
        kind,
        source_path: source_path.to_string(),
        application,
        fault_module,
        product_name,
        manufacturer,
        details,
    })
}

fn event_data_items(wrapper: &Value) -> Vec<Value> {
    let Some(event_data) = wrapper.get("EventData") else {
        return Vec::new();
    };
    match event_data.get("Data") {
        Some(Value::Array(items)) => items.clone(),
        Some(single) => vec![single.clone()],
        None => Vec::new(),
    }
}

/// Extract the raw text values of `<EventData><Data>...</Data></EventData>`
/// elements in document order. This is used as a positional fallback for
/// events whose `<Data>` elements do not carry a `Name` attribute (e.g. older
/// Application Error / MsiInstaller records).
fn event_data_values(wrapper: &Value) -> Vec<String> {
    event_data_items(wrapper)
        .into_iter()
        .filter_map(|item| {
            if let Some(text) = item.as_str() {
                return Some(text.to_string());
            }
            if let Some(obj) = item.as_object() {
                if obj.contains_key("@Name") || obj.contains_key("Name") {
                    // Named items are handled by event_data_map; including them
                    // here would shift positional indices for mixed schemas.
                    return None;
                }
                return obj
                    .get("#text")
                    .or_else(|| obj.get("Text"))
                    .and_then(value_as_string);
            }
            None
        })
        .collect()
}

/// Extract all named `<EventData>` values into a generic key/value map.
///
/// Handles both JSON shapes the evtx library may produce:
/// - Modern flattened named data:
///   `{"EventData": {"TargetUserName": "…", "LogonType": 3}}`
/// - Legacy array form:
///   `{"EventData": {"Data": [{"@Name": "…", "#text": "value"}, …]}}`
fn event_data_map(wrapper: &Value) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let Some(event_data) = wrapper.get("EventData") else {
        return map;
    };

    // Modern flattened shape: EventData is an object whose keys are the Data names.
    if let Some(obj) = event_data.as_object() {
        for (key, value) in obj {
            if key == "Data" || key.starts_with('#') || key.starts_with('@') {
                continue;
            }
            if let Some(text) = value_as_string(value) {
                map.insert(key.clone(), text);
            }
        }
    }

    // Legacy array shape: EventData.Data is an array of {Name, #text} objects.
    for item in event_data_items(wrapper) {
        let Some((name, value)) = extract_named_data_item(&item) else {
            continue;
        };
        map.insert(name, value);
    }

    map
}

fn value_as_string(value: &Value) -> Option<String> {
    if value.is_string() {
        value.as_str().map(str::to_string)
    } else if value.is_number() {
        Some(value.to_string())
    } else {
        None
    }
}

fn extract_named_data_item(item: &Value) -> Option<(String, String)> {
    let obj = item.as_object()?;
    let name = obj
        .get("@Name")
        .or_else(|| obj.get("Name"))
        .and_then(Value::as_str)?;
    let value = obj
        .get("#text")
        .or_else(|| obj.get("Text"))
        .or_else(|| obj.get("Value"))
        .and_then(value_as_string)
        .unwrap_or_default();
    Some((name.to_string(), value))
}

fn provider_matches(provider: Option<&str>, target: &str) -> bool {
    match provider {
        Some(p) => p.eq_ignore_ascii_case(target),
        None => false,
    }
}

fn event_id(value: &Value) -> Option<u32> {
    match value {
        Value::Number(number) => number.as_u64().and_then(|value| u32::try_from(value).ok()),
        Value::String(text) => text.parse().ok(),
        Value::Object(map) => map
            .get("#text")
            .or_else(|| map.get("Text"))
            .or_else(|| map.get("Value"))
            .and_then(event_id),
        _ => None,
    }
}

fn event_record_id(system: &Value) -> Option<u64> {
    match system.get("EventRecordID")? {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn event_timestamp(system: &Value) -> Option<String> {
    let time_created = system.get("TimeCreated")?;
    let raw = match time_created {
        Value::Object(map) => map
            .get("@SystemTime")
            .or_else(|| map.get("SystemTime"))
            .and_then(Value::as_str),
        Value::String(text) => Some(text.as_str()),
        _ => None,
    }?;
    normalize_timestamp(raw)
}

fn normalize_timestamp(raw: &str) -> Option<String> {
    DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc).to_rfc3339())
        .ok()
        .or_else(|| {
            DateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S%.f %z")
                .map(|dt| dt.with_timezone(&Utc).to_rfc3339())
                .ok()
        })
        .or_else(|| Some(raw.to_string()).filter(|value| !value.trim().is_empty()))
}

fn provider_name(system: &Value) -> Option<String> {
    let provider = system.get("Provider")?;
    match provider {
        Value::Object(map) => map
            .get("@Name")
            .or_else(|| map.get("Name"))
            .and_then(Value::as_str)
            .map(str::to_string),
        Value::String(text) => Some(text.clone()),
        _ => None,
    }
}

fn event_channel(system: &Value) -> Option<String> {
    let channel = system.get("Channel")?;
    match channel {
        Value::String(text) => Some(text.clone()),
        Value::Object(map) => map
            .get("#text")
            .or_else(|| map.get("Text"))
            .or_else(|| map.get("Value"))
            .and_then(Value::as_str)
            .map(str::to_string),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_eventlog_started_6005_from_json() {
        let extraction = extract_boot_shutdown_events_from_json_records(
            &[json!({
                "Event": {
                    "System": {
                        "Provider": { "@Name": "EventLog" },
                        "EventID": 6005,
                        "EventRecordID": 41,
                        "TimeCreated": { "@SystemTime": "2026-01-01T00:00:00Z" }
                    }
                }
            })],
            "Windows/System32/winevt/Logs/System.evtx",
        )
        .expect("json extraction should succeed");

        assert!(extraction.warnings.is_empty());
        assert_eq!(extraction.events.len(), 1);
        assert_eq!(extraction.events[0].event_id, 6005);
        assert_eq!(
            extraction.events[0].kind,
            EvtxBootEventKind::EventLogStarted
        );
        assert_eq!(extraction.events[0].record_id, Some(41));
        assert_eq!(extraction.events[0].provider.as_deref(), Some("EventLog"));
    }

    #[test]
    fn extract_shutdown_candidates_from_json_string_event_id() {
        let extraction = extract_boot_shutdown_events_from_json_records(
            &[
                json!({"Event":{"System":{"EventID":"6006","TimeCreated":{"SystemTime":"2026-01-01T01:00:00Z"}}}}),
                json!({"Event":{"System":{"EventID":{"#text":"6008"},"TimeCreated":"2026-01-01T02:00:00Z"}}}),
                json!({"Event":{"System":{"EventID":"1074","TimeCreated":{"@SystemTime":"2026-01-01T03:00:00Z"}}}}),
            ],
            "Windows/System32/winevt/Logs/System.evtx",
        )
        .expect("json extraction should succeed");

        let kinds = extraction
            .events
            .iter()
            .map(|event| event.kind.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec!["eventLogStopped", "unexpectedShutdown", "plannedShutdown"]
        );
    }

    #[test]
    fn ignores_unsupported_event_ids() {
        let extraction = extract_boot_shutdown_events_from_json_records(
            &[json!({"Event":{"System":{"EventID":7045}}})],
            "Windows/System32/winevt/Logs/System.evtx",
        )
        .expect("json extraction should succeed");

        assert!(extraction.events.is_empty());
        assert!(extraction.warnings.is_empty());
    }

    #[test]
    fn malformed_evtx_returns_error_not_panic() {
        let result = extract_boot_shutdown_events(
            b"not an evtx",
            "Windows/System32/winevt/Logs/System.evtx",
        );
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("parser initialization failed"));
    }

    #[test]
    fn truncated_evtx_magic_returns_error_not_panic() {
        let result =
            extract_boot_shutdown_events(b"ElfFile\0", "Windows/System32/winevt/Logs/System.evtx");
        assert!(result.is_err());
    }

    #[test]
    fn chunk_warning_includes_chunk_id_offset_and_reason() {
        let warning = format_evtx_warning(
            "Windows/System32/winevt/Logs/System.evtx",
            &EvtxError::FailedToParseChunk {
                chunk_id: 29,
                source: Box::new(evtx::err::ChunkError::IncompleteChunk),
            },
        );

        assert!(warning.contains("chunk=29"));
        assert!(warning.contains("offset=0x001D1000"));
        assert!(warning.contains("Reached EOF"));
    }

    #[test]
    fn clean_evtx_uses_declared_chunk_count_to_ignore_tail_slack() {
        let declared_len = EVTX_FILE_HEADER_SIZE as usize + EVTX_CHUNK_SIZE as usize;
        let mut bytes = vec![0u8; declared_len + EVTX_CHUNK_SIZE as usize];
        bytes[0..8].copy_from_slice(b"ElfFile\0");
        bytes[42..44].copy_from_slice(&1u16.to_le_bytes());
        bytes[120..124].copy_from_slice(&0u32.to_le_bytes());

        let bounded = bounded_clean_evtx_bytes(&bytes);
        assert_eq!(bounded.len(), declared_len);
    }

    #[test]
    fn dirty_evtx_keeps_tail_for_recovery_scan() {
        let mut bytes = vec![0u8; EVTX_FILE_HEADER_SIZE as usize + EVTX_CHUNK_SIZE as usize * 2];
        bytes[0..8].copy_from_slice(b"ElfFile\0");
        bytes[42..44].copy_from_slice(&1u16.to_le_bytes());
        bytes[120..124].copy_from_slice(&1u32.to_le_bytes());

        let bounded = bounded_clean_evtx_bytes(&bytes);
        assert_eq!(bounded.len(), bytes.len());
    }

    #[test]
    fn oversized_evtx_returns_error() {
        let bytes = vec![0u8; MAX_EVTX_ANALYSIS_BYTES + 1];
        let result =
            extract_boot_shutdown_events(&bytes, "Windows/System32/winevt/Logs/System.evtx");

        assert!(matches!(result, Err(EvtxBootError::InputTooLarge { .. })));
    }

    #[test]
    fn parses_real_system_evtx_fixture_boot_candidates() {
        let path = testing::fixtures::tiny_system_evtx();
        let bytes = std::fs::read(&path).expect("read tiny System.evtx fixture");
        let extraction =
            extract_boot_shutdown_events(&bytes, "Windows/System32/winevt/Logs/System.evtx")
                .expect("fixture extraction should succeed");

        assert!(
            !extraction.events.is_empty(),
            "expected at least one boot/shutdown candidate; warnings: {:?}",
            extraction.warnings
        );
        assert!(
            extraction
                .events
                .iter()
                .any(|event| matches!(event.event_id, 6005 | 6006 | 6008 | 1074)),
            "expected EventLog/User32 boot/shutdown candidate in fixture"
        );
        assert!(extraction.events.iter().all(|event| {
            event.source_path == "Windows/System32/winevt/Logs/System.evtx"
                && !event.timestamp.trim().is_empty()
                && !event.note.trim().is_empty()
        }));
    }

    #[test]
    fn unsupported_path_returns_error() {
        let result = extract_boot_shutdown_events(b"ElfFile\0", "Windows/Temp/UnknownChannel.evtx");
        assert!(matches!(result, Err(EvtxBootError::UnsupportedPath { .. })));
    }

    #[test]
    #[ignore = "manual fixture regeneration helper"]
    fn dump_fixture_to_expected_json() {
        use std::path::Path;

        let manifest = env!("CARGO_MANIFEST_DIR");
        let out =
            Path::new(manifest).join("../../testdata/fixtures/public-small/evtx/expected.json");
        let path = testing::fixtures::tiny_system_evtx();
        let bytes = std::fs::read(&path).expect("read tiny System.evtx fixture");
        let extraction =
            extract_boot_shutdown_events(&bytes, "Windows/System32/winevt/Logs/System.evtx")
                .expect("fixture extraction should succeed");
        std::fs::write(&out, serde_json::to_string_pretty(&extraction).unwrap())
            .expect("write expected.json");
        println!("written expected.json to {}", out.display());
    }

    #[test]
    fn extract_powershell_4104_script_block_from_json() {
        let extraction = extract_boot_shutdown_events_from_json_records(
            &[json!({
                "Event": {
                    "System": {
                        "Provider": { "@Name": "Microsoft-Windows-PowerShell" },
                        "EventID": 4104,
                        "EventRecordID": 100,
                        "TimeCreated": { "@SystemTime": "2026-03-15T10:30:00Z" }
                    },
                    "EventData": {
                        "Data": [
                            { "@Name": "ScriptBlockText", "#text": "Get-Process | Where-Object {$_.CPU -gt 10}" },
                            { "@Name": "Path", "#text": "C:\\Scripts\\audit.ps1" }
                        ]
                    }
                }
            })],
            "Microsoft-Windows-PowerShell%4Operational.evtx",
        )
        .expect("json extraction should succeed");

        assert!(extraction.warnings.is_empty());
        assert_eq!(extraction.events.len(), 1);
        let event = &extraction.events[0];
        assert_eq!(event.event_id, 4104);
        assert_eq!(event.kind, EvtxBootEventKind::PowerShellScriptBlock);
        assert!(event.details.contains_key("ScriptBlockText"));
        assert_eq!(
            event.details.get("ScriptBlockText").map(String::as_str),
            Some("Get-Process | Where-Object {$_.CPU -gt 10}")
        );
        assert_eq!(
            event.details.get("Path").map(String::as_str),
            Some("C:\\Scripts\\audit.ps1")
        );
    }

    #[test]
    fn extract_sysmon_1_process_create_from_json() {
        let extraction = extract_boot_shutdown_events_from_json_records(
            &[json!({
                "Event": {
                    "System": {
                        "Provider": { "@Name": "Microsoft-Windows-Sysmon" },
                        "EventID": 1,
                        "EventRecordID": 200,
                        "TimeCreated": { "@SystemTime": "2026-03-15T11:00:00Z" }
                    },
                    "EventData": {
                        "Data": [
                            { "@Name": "Image", "#text": "C:\\Windows\\System32\\cmd.exe" },
                            { "@Name": "CommandLine", "#text": "cmd.exe /c whoami" },
                            { "@Name": "User", "#text": "DOMAIN\\User" }
                        ]
                    }
                }
            })],
            "Microsoft-Windows-Sysmon%4Operational.evtx",
        )
        .expect("json extraction should succeed");

        assert!(extraction.warnings.is_empty());
        assert_eq!(extraction.events.len(), 1);
        let event = &extraction.events[0];
        assert_eq!(event.event_id, 1);
        assert_eq!(event.kind, EvtxBootEventKind::SysmonProcessCreate);
        assert_eq!(
            event.details.get("Image").map(String::as_str),
            Some("C:\\Windows\\System32\\cmd.exe")
        );
        assert_eq!(
            event.details.get("CommandLine").map(String::as_str),
            Some("cmd.exe /c whoami")
        );
    }

    #[test]
    fn extract_rdp_21_session_connect_from_json() {
        let extraction = extract_boot_shutdown_events_from_json_records(
            &[json!({
                "Event": {
                    "System": {
                        "Provider": { "@Name": "Microsoft-Windows-TerminalServices-LocalSessionManager" },
                        "EventID": 21,
                        "EventRecordID": 300,
                        "TimeCreated": { "@SystemTime": "2026-03-15T12:00:00Z" }
                    },
                    "EventData": {
                        "Data": [
                            { "@Name": "User", "#text": "DOMAIN\\jsmith" },
                            { "@Name": "Address", "#text": "192.168.1.100" }
                        ]
                    }
                }
            })],
            "Microsoft-Windows-TerminalServices-LocalSessionManager%4Operational.evtx",
        )
        .expect("json extraction should succeed");

        assert!(extraction.warnings.is_empty());
        assert_eq!(extraction.events.len(), 1);
        let event = &extraction.events[0];
        assert_eq!(event.event_id, 21);
        assert_eq!(event.kind, EvtxBootEventKind::RdpSessionConnect);
        assert_eq!(
            event.details.get("User").map(String::as_str),
            Some("DOMAIN\\jsmith")
        );
        assert_eq!(
            event.details.get("Address").map(String::as_str),
            Some("192.168.1.100")
        );
    }

    #[test]
    fn extract_defender_1116_threat_from_json() {
        let extraction = extract_boot_shutdown_events_from_json_records(
            &[json!({
                "Event": {
                    "System": {
                        "Provider": { "@Name": "Microsoft-Windows-Windows Defender" },
                        "EventID": 1116,
                        "EventRecordID": 400,
                        "TimeCreated": { "@SystemTime": "2026-03-15T13:00:00Z" }
                    },
                    "EventData": {
                        "Data": [
                            { "@Name": "Threat", "#text": "Trojan:Win32/Malware" },
                            { "@Name": "Severity", "#text": "Severe" },
                            { "@Name": "Category", "#text": "Trojan" }
                        ]
                    }
                }
            })],
            "Microsoft-Windows-Windows Defender%4Operational.evtx",
        )
        .expect("json extraction should succeed");

        assert!(extraction.warnings.is_empty());
        assert_eq!(extraction.events.len(), 1);
        let event = &extraction.events[0];
        assert_eq!(event.event_id, 1116);
        assert_eq!(event.kind, EvtxBootEventKind::DefenderThreatDetected);
        assert_eq!(
            event.details.get("Threat").map(String::as_str),
            Some("Trojan:Win32/Malware")
        );
        assert_eq!(
            event.details.get("Severity").map(String::as_str),
            Some("Severe")
        );
    }

    #[test]
    fn event_data_map_handles_various_json_shapes() {
        // Legacy array shape with @Name and #text
        let wrapper = json!({
            "EventData": {
                "Data": [
                    { "@Name": "Image", "#text": "cmd.exe" },
                    { "Name": "CommandLine", "#text": "/c dir" },
                    { "@Name": "ThReAt", "#text": "malware" },
                    { "@Name": "Other", "#text": "val" }
                ]
            }
        });
        let map = event_data_map(&wrapper);
        assert_eq!(map.get("Image").map(String::as_str), Some("cmd.exe"));
        assert_eq!(map.get("CommandLine").map(String::as_str), Some("/c dir"));
        assert_eq!(map.get("ThReAt").map(String::as_str), Some("malware"));
        assert_eq!(map.get("Image2").map(String::as_str), None);

        // Modern flattened shape
        let wrapper = json!({
            "EventData": {
                "Image": "cmd.exe",
                "CommandLine": "/c dir",
                "LogonType": 3
            }
        });
        let map = event_data_map(&wrapper);
        assert_eq!(map.get("Image").map(String::as_str), Some("cmd.exe"));
        assert_eq!(map.get("CommandLine").map(String::as_str), Some("/c dir"));
        assert_eq!(map.get("LogonType").map(String::as_str), Some("3"));
    }

    #[test]
    fn extract_security_4624_flattened_event_data() {
        let extraction = extract_structured_events_from_json_records(
            &[json!({
                "Event": {
                    "System": {
                        "Provider": { "@Name": "Microsoft-Windows-Security-Auditing" },
                        "EventID": 4624,
                        "EventRecordID": 1,
                        "Channel": "Security",
                        "TimeCreated": { "@SystemTime": "2026-03-15T08:00:00Z" }
                    },
                    "EventData": {
                        "TargetUserName": "jdoe",
                        "LogonType": 3,
                        "IpAddress": "192.168.1.10",
                        "WorkstationName": "DESKTOP-ABC",
                        "Status": "0x0"
                    }
                }
            })],
            "Windows/System32/winevt/Logs/Security.evtx",
        )
        .expect("json extraction should succeed");

        assert_eq!(extraction.security_events.len(), 1);
        let event = &extraction.security_events[0];
        assert_eq!(event.kind, EvtxSecurityEventKind::LogonSuccess);
        assert_eq!(event.target_user.as_deref(), Some("jdoe"));
        assert_eq!(event.logon_type.as_deref(), Some("3"));
        assert_eq!(event.ip_address.as_deref(), Some("192.168.1.10"));
        assert_eq!(event.workstation.as_deref(), Some("DESKTOP-ABC"));
        assert_eq!(event.failure_reason.as_deref(), Some("0x0"));
    }

    #[test]
    fn extract_security_4625_failure_from_flattened_event_data() {
        let extraction = extract_structured_events_from_json_records(
            &[json!({
                "Event": {
                    "System": {
                        "Provider": { "@Name": "Microsoft-Windows-Security-Auditing" },
                        "EventID": 4625,
                        "EventRecordID": 2,
                        "Channel": "Security",
                        "TimeCreated": { "@SystemTime": "2026-03-15T08:01:00Z" }
                    },
                    "EventData": {
                        "TargetUserName": "admin",
                        "LogonType": 10,
                        "IpAddress": "10.0.0.5",
                        "Status": "0xC000006D",
                        "SubStatus": "0xC000006A"
                    }
                }
            })],
            "Windows/System32/winevt/Logs/Security.evtx",
        )
        .expect("json extraction should succeed");

        assert_eq!(extraction.security_events.len(), 1);
        let event = &extraction.security_events[0];
        assert_eq!(event.kind, EvtxSecurityEventKind::LogonFailure);
        assert_eq!(event.target_user.as_deref(), Some("admin"));
        assert_eq!(event.logon_type.as_deref(), Some("10"));
        assert_eq!(event.ip_address.as_deref(), Some("10.0.0.5"));
        assert_eq!(event.failure_reason.as_deref(), Some("0xC000006D"));
    }

    #[test]
    fn extract_security_event_data_map_merges_legacy_and_flattened_shapes() {
        let wrapper = json!({
            "EventData": {
                "TargetUserName": "flattened",
                "Data": [
                    { "@Name": "LogonType", "#text": "2" },
                    { "@Name": "MissingFromFlat", "#text": "legacy" }
                ]
            }
        });

        let map = event_data_map(&wrapper);
        assert_eq!(
            map.get("TargetUserName").map(String::as_str),
            Some("flattened")
        );
        assert_eq!(map.get("LogonType").map(String::as_str), Some("2"));
        assert_eq!(
            map.get("MissingFromFlat").map(String::as_str),
            Some("legacy")
        );
    }

    #[test]
    fn extract_application_1000_flattened_event_data() {
        let extraction = extract_structured_events_from_json_records(
            &[json!({
                "Event": {
                    "System": {
                        "Provider": { "@Name": "Application Error" },
                        "EventID": 1000,
                        "EventRecordID": 10,
                        "Channel": "Application",
                        "TimeCreated": { "@SystemTime": "2026-03-15T09:00:00Z" }
                    },
                    "EventData": {
                        "AppName": "chrome.exe",
                        "ModuleName": "ntdll.dll"
                    }
                }
            })],
            "Windows/System32/winevt/Logs/Application.evtx",
        )
        .expect("json extraction should succeed");

        assert_eq!(extraction.application_events.len(), 1);
        let event = &extraction.application_events[0];
        assert_eq!(event.kind, EvtxApplicationEventKind::ApplicationCrash);
        assert_eq!(event.application.as_deref(), Some("chrome.exe"));
        assert_eq!(event.fault_module.as_deref(), Some("ntdll.dll"));
    }

    #[test]
    fn extract_security_4688_prefers_parent_process_name() {
        let extraction = extract_structured_events_from_json_records(
            &[json!({
                "Event": {
                    "System": {
                        "Provider": { "@Name": "Microsoft-Windows-Security-Auditing" },
                        "EventID": 4688,
                        "EventRecordID": 101,
                        "Channel": "Security",
                        "TimeCreated": { "@SystemTime": "2026-03-15T10:00:00Z" }
                    },
                    "EventData": {
                        "NewProcessName": "C:\\Windows\\System32\\cmd.exe",
                        "ParentProcessName": "C:\\Windows\\explorer.exe",
                        "ProcessId": "0x1234"
                    }
                }
            })],
            "Windows/System32/winevt/Logs/Security.evtx",
        )
        .expect("json extraction should succeed");

        assert_eq!(extraction.security_events.len(), 1);
        let event = &extraction.security_events[0];
        assert_eq!(event.kind, EvtxSecurityEventKind::ProcessCreated);
        assert_eq!(
            event.process_name.as_deref(),
            Some("C:\\Windows\\System32\\cmd.exe")
        );
        assert_eq!(
            event.parent_process_name.as_deref(),
            Some("C:\\Windows\\explorer.exe")
        );
    }

    #[test]
    fn extract_security_4688_falls_back_to_creator_process_name() {
        let extraction = extract_structured_events_from_json_records(
            &[json!({
                "Event": {
                    "System": {
                        "Provider": { "@Name": "Microsoft-Windows-Security-Auditing" },
                        "EventID": 4688,
                        "EventRecordID": 102,
                        "Channel": "Security",
                        "TimeCreated": { "@SystemTime": "2026-03-15T10:01:00Z" }
                    },
                    "EventData": {
                        "NewProcessName": "C:\\Windows\\System32\\powershell.exe",
                        "CreatorProcessName": "C:\\Windows\\System32\\cmd.exe"
                    }
                }
            })],
            "Windows/System32/winevt/Logs/Security.evtx",
        )
        .expect("json extraction should succeed");

        assert_eq!(extraction.security_events.len(), 1);
        let event = &extraction.security_events[0];
        assert_eq!(
            event.parent_process_name.as_deref(),
            Some("C:\\Windows\\System32\\cmd.exe")
        );
    }

    #[test]
    fn extract_application_1000_unnamed_data_positions() {
        let extraction = extract_structured_events_from_json_records(
            &[json!({
                "Event": {
                    "System": {
                        "Provider": { "@Name": "Application Error" },
                        "EventID": 1000,
                        "EventRecordID": 11,
                        "Channel": "Application",
                        "TimeCreated": { "@SystemTime": "2026-03-15T09:01:00Z" }
                    },
                    "EventData": {
                        "Data": [
                            "chrome.exe",
                            "12.0.0.0",
                            "63a1b2c3",
                            "ntdll.dll",
                            "10.0.19041.0"
                        ]
                    }
                }
            })],
            "Windows/System32/winevt/Logs/Application.evtx",
        )
        .expect("json extraction should succeed");

        assert_eq!(extraction.application_events.len(), 1);
        let event = &extraction.application_events[0];
        assert_eq!(event.kind, EvtxApplicationEventKind::ApplicationCrash);
        assert_eq!(event.application.as_deref(), Some("chrome.exe"));
        assert_eq!(event.fault_module.as_deref(), Some("ntdll.dll"));
    }

    #[test]
    fn extract_application_1001_named_wer_p1_p4_fields() {
        let extraction = extract_structured_events_from_json_records(
            &[json!({
                "Event": {
                    "System": {
                        "Provider": { "@Name": "Windows Error Reporting" },
                        "EventID": 1001,
                        "EventRecordID": 12,
                        "Channel": "Application",
                        "TimeCreated": { "@SystemTime": "2026-03-15T09:02:00Z" }
                    },
                    "EventData": {
                        "P1": "notepad.exe",
                        "P4": "kernelbase.dll"
                    }
                }
            })],
            "Windows/System32/winevt/Logs/Application.evtx",
        )
        .expect("json extraction should succeed");

        assert_eq!(extraction.application_events.len(), 1);
        let event = &extraction.application_events[0];
        assert_eq!(event.kind, EvtxApplicationEventKind::WindowsErrorReporting);
        assert_eq!(event.application.as_deref(), Some("notepad.exe"));
        assert_eq!(event.fault_module.as_deref(), Some("kernelbase.dll"));
    }

    #[test]
    fn extract_application_1033_unnamed_data_positions() {
        let extraction = extract_structured_events_from_json_records(
            &[json!({
                "Event": {
                    "System": {
                        "Provider": { "@Name": "MsiInstaller" },
                        "EventID": 1033,
                        "EventRecordID": 13,
                        "Channel": "Application",
                        "TimeCreated": { "@SystemTime": "2026-03-15T09:03:00Z" }
                    },
                    "EventData": {
                        "Data": [
                            "ForensicsWorkbench",
                            "1.0.0",
                            "1033",
                            "0",
                            "Contoso Inc.",
                            "(none)"
                        ]
                    }
                }
            })],
            "Windows/System32/winevt/Logs/Application.evtx",
        )
        .expect("json extraction should succeed");

        assert_eq!(extraction.application_events.len(), 1);
        let event = &extraction.application_events[0];
        assert_eq!(event.kind, EvtxApplicationEventKind::SoftwareInstallation);
        assert_eq!(event.product_name.as_deref(), Some("ForensicsWorkbench"));
        assert_eq!(event.manufacturer.as_deref(), Some("Contoso Inc."));
    }

    #[test]
    fn boot_events_dont_have_details() {
        let extraction = extract_boot_shutdown_events_from_json_records(
            &[json!({
                "Event": {
                    "System": {
                        "Provider": { "@Name": "EventLog" },
                        "EventID": 6005,
                        "TimeCreated": { "@SystemTime": "2026-01-01T00:00:00Z" }
                    }
                }
            })],
            "Windows/System32/winevt/Logs/System.evtx",
        )
        .expect("json extraction should succeed");

        assert_eq!(extraction.events.len(), 1);
        assert!(extraction.events[0].details.is_empty());
    }
}
