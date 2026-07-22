use super::types::{
    EvtxApplicationEvent, EvtxApplicationEventKind, EvtxBootEvent, EvtxBootEventKind,
    EvtxSecurityEvent, EvtxSecurityEventKind, EvtxStructuredExtraction,
};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) fn structured_event_from_json(
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
    let Some(event_id) = system.get("EventID").and_then(parse_event_id) else {
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
            extraction.boot_events.push(EvtxBootEvent {
                timestamp,
                event_id,
                record_id,
                provider,
                kind,
                source_path: source_path.to_string(),
                note,
                details: event_data_map(wrapper),
            });
        }
        EventClass::Security(kind) => extraction.security_events.push(security_event_from_json(
            wrapper,
            kind,
            timestamp,
            event_id,
            record_id,
            provider,
            source_path,
        )),
        EventClass::Application(kind) => {
            extraction
                .application_events
                .push(application_event_from_json(
                    wrapper,
                    kind,
                    timestamp,
                    event_id,
                    record_id,
                    provider,
                    source_path,
                ));
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

fn classify_event(event_id: u32, provider: Option<&str>, channel: Option<&str>) -> EventClass {
    let channel = channel.unwrap_or("");
    match event_id {
        12 if provider_matches(provider, "microsoft-windows-kernel-general") => {
            EventClass::Boot(EvtxBootEventKind::OperatingSystemStarted)
        }
        13 if provider_matches(provider, "microsoft-windows-kernel-general") => {
            EventClass::Boot(EvtxBootEventKind::OperatingSystemShutdown)
        }
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
) -> EvtxSecurityEvent {
    let details = event_data_map(wrapper);
    EvtxSecurityEvent {
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
    }
}

fn application_event_from_json(
    wrapper: &Value,
    kind: EvtxApplicationEventKind,
    timestamp: String,
    event_id: u32,
    record_id: Option<u64>,
    provider: Option<String>,
    source_path: &str,
) -> EvtxApplicationEvent {
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
            1002 => application = application.or_else(|| values.first().cloned()),
            1033 | 11707 | 11708 => {
                product_name = product_name.or_else(|| values.first().cloned());
                manufacturer = manufacturer.or_else(|| values.get(4).cloned());
            }
            _ => {}
        }
    }
    EvtxApplicationEvent {
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
    }
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

fn event_data_values(wrapper: &Value) -> Vec<String> {
    event_data_items(wrapper)
        .into_iter()
        .filter_map(|item| {
            if let Some(text) = item.as_str() {
                return Some(text.to_string());
            }
            let obj = item.as_object()?;
            if obj.contains_key("@Name") || obj.contains_key("Name") {
                return None;
            }
            obj.get("#text")
                .or_else(|| obj.get("Text"))
                .and_then(value_as_string)
        })
        .collect()
}

pub(super) fn event_data_map(wrapper: &Value) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let Some(event_data) = wrapper.get("EventData") else {
        return map;
    };
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
    for item in event_data_items(wrapper) {
        if let Some((name, value)) = extract_named_data_item(&item) {
            map.insert(name, value);
        }
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
        .or_else(|| nested_attribute(obj, "Name"))
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
    provider.is_some_and(|provider| provider.eq_ignore_ascii_case(target))
}

fn parse_event_id(value: &Value) -> Option<u32> {
    match value {
        Value::Number(number) => number.as_u64().and_then(|value| u32::try_from(value).ok()),
        Value::String(text) => text.parse().ok(),
        Value::Object(map) => map
            .get("#text")
            .or_else(|| map.get("Text"))
            .or_else(|| map.get("Value"))
            .and_then(parse_event_id),
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
    let raw = match system.get("TimeCreated")? {
        Value::Object(map) => map
            .get("@SystemTime")
            .or_else(|| map.get("SystemTime"))
            .or_else(|| nested_attribute(map, "SystemTime"))
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
    match system.get("Provider")? {
        Value::Object(map) => map
            .get("@Name")
            .or_else(|| map.get("Name"))
            .or_else(|| nested_attribute(map, "Name"))
            .and_then(Value::as_str)
            .map(str::to_string),
        Value::String(text) => Some(text.clone()),
        _ => None,
    }
}

fn nested_attribute<'a>(map: &'a serde_json::Map<String, Value>, name: &str) -> Option<&'a Value> {
    map.get("#attributes")
        .and_then(Value::as_object)
        .and_then(|attributes| attributes.get(name))
}

fn event_channel(system: &Value) -> Option<String> {
    match system.get("Channel")? {
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
