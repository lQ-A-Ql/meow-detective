use super::reader::{
    bounded_clean_evtx_bytes, format_evtx_warning, EVTX_CHUNK_SIZE, EVTX_FILE_HEADER_SIZE,
};
use super::records::event_data_map;
use super::*;
use crate::evtx::EvtxBootError;
use evtx::err::EvtxError;
use serde_json::json;

const SYSTEM_PATH: &str = "Windows/System32/winevt/Logs/System.evtx";
const SECURITY_PATH: &str = "Windows/System32/winevt/Logs/Security.evtx";
const APPLICATION_PATH: &str = "Windows/System32/winevt/Logs/Application.evtx";

#[test]
fn extract_eventlog_started_6005_from_json() {
    let extraction = extract_boot_shutdown_events_from_json_records(
        &[json!({"Event":{"System":{
            "Provider":{"@Name":"EventLog"},
            "EventID":6005,
            "EventRecordID":41,
            "TimeCreated":{"@SystemTime":"2026-01-01T00:00:00Z"}
        }}})],
        SYSTEM_PATH,
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
        SYSTEM_PATH,
    )
    .expect("json extraction should succeed");
    assert_eq!(
        extraction
            .events
            .iter()
            .map(|event| event.kind.as_str())
            .collect::<Vec<_>>(),
        vec!["eventLogStopped", "unexpectedShutdown", "plannedShutdown"]
    );
}

#[test]
fn extract_kernel_general_operating_system_boundaries() {
    let records = [
        json!({"Event":{"System":{
            "Provider":{"@Name":"Microsoft-Windows-Kernel-General"},
            "EventID":12,
            "EventRecordID":10,
            "TimeCreated":{"@SystemTime":"2026-01-01T00:00:00Z"}
        }}}),
        json!({"Event":{"System":{
            "Provider":{"@Name":"Microsoft-Windows-Kernel-General"},
            "EventID":13,
            "EventRecordID":11,
            "TimeCreated":{"@SystemTime":"2026-01-01T01:00:00Z"}
        }}}),
    ];
    let extraction = extract_boot_shutdown_events_from_json_records(&records, SYSTEM_PATH)
        .expect("Kernel-General records should parse");

    assert_eq!(extraction.events.len(), 2);
    assert_eq!(
        extraction.events[0].kind,
        EvtxBootEventKind::OperatingSystemStarted
    );
    assert_eq!(
        extraction.events[1].kind,
        EvtxBootEventKind::OperatingSystemShutdown
    );
    assert_eq!(extraction.events[0].timestamp, "2026-01-01T00:00:00+00:00");
}

#[test]
fn extract_kernel_boundaries_from_nested_json_attributes() {
    let records = [
        json!({"Event":{"System":{
            "Provider":{"#attributes":{"Name":"Microsoft-Windows-Kernel-General"}},
            "EventID":12,
            "TimeCreated":{"#attributes":{"SystemTime":"2026-01-01T00:00:00Z"}}
        }}}),
        json!({"Event":{"System":{
            "Provider":{"#attributes":{"Name":"Microsoft-Windows-Kernel-General"}},
            "EventID":13,
            "TimeCreated":{"#attributes":{"SystemTime":"2026-01-01T01:00:00Z"}}
        }}}),
    ];
    let extraction = extract_boot_shutdown_events_from_json_records(&records, SYSTEM_PATH)
        .expect("nested attributes should parse");

    assert_eq!(extraction.events.len(), 2);
    assert_eq!(extraction.events[0].event_id, 12);
    assert_eq!(extraction.events[1].event_id, 13);
    assert_eq!(extraction.events[0].timestamp, "2026-01-01T00:00:00+00:00");
}

#[test]
fn nested_event_data_attributes_preserve_named_values() {
    let records = [json!({"Event":{
        "System":{
            "Provider":{"#attributes":{"Name":"Microsoft-Windows-Kernel-General"}},
            "EventID":13,
            "TimeCreated":{"#attributes":{"SystemTime":"2026-01-01T01:00:00Z"}}
        },
        "EventData":{"Data":{"#attributes":{"Name":"StopTime"},"#text":"2026-01-01 01:00:00.0000000"}}
    }})];
    let extraction = extract_boot_shutdown_events_from_json_records(&records, SYSTEM_PATH)
        .expect("nested event data attributes should parse");

    assert_eq!(
        extraction.events[0]
            .details
            .get("StopTime")
            .map(String::as_str),
        Some("2026-01-01 01:00:00.0000000")
    );
}

#[test]
fn kernel_general_ids_require_the_expected_provider() {
    let records = [
        json!({"Event":{"System":{
            "Provider":{"@Name":"Unrelated-Provider"},
            "EventID":12,
            "TimeCreated":{"@SystemTime":"2026-01-01T00:00:00Z"}
        }}}),
        json!({"Event":{"System":{
            "Provider":{"@Name":"Unrelated-Provider"},
            "EventID":13,
            "TimeCreated":{"@SystemTime":"2026-01-01T01:00:00Z"}
        }}}),
    ];
    let extraction = extract_boot_shutdown_events_from_json_records(&records, SYSTEM_PATH)
        .expect("unrelated records should be ignored");

    assert!(extraction.events.is_empty());
}

#[test]
fn ignores_unsupported_event_ids() {
    let extraction = extract_boot_shutdown_events_from_json_records(
        &[json!({"Event":{"System":{"EventID":7045}}})],
        SYSTEM_PATH,
    )
    .expect("json extraction should succeed");
    assert!(extraction.events.is_empty());
    assert!(extraction.warnings.is_empty());
}

#[test]
fn malformed_evtx_returns_error_not_panic() {
    let error = extract_boot_shutdown_events(b"not an evtx", SYSTEM_PATH)
        .expect_err("malformed input must fail");
    assert!(error.to_string().contains("parser initialization failed"));
}

#[test]
fn truncated_evtx_magic_returns_error_not_panic() {
    assert!(extract_boot_shutdown_events(b"ElfFile\0", SYSTEM_PATH).is_err());
}

#[test]
fn chunk_warning_includes_chunk_id_offset_and_reason() {
    let warning = format_evtx_warning(
        SYSTEM_PATH,
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
    let mut bytes = vec![0; declared_len + EVTX_CHUNK_SIZE as usize];
    bytes[0..8].copy_from_slice(b"ElfFile\0");
    bytes[42..44].copy_from_slice(&1u16.to_le_bytes());
    bytes[120..124].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(bounded_clean_evtx_bytes(&bytes).len(), declared_len);
}

#[test]
fn dirty_evtx_keeps_tail_for_recovery_scan() {
    let mut bytes = vec![0; EVTX_FILE_HEADER_SIZE as usize + EVTX_CHUNK_SIZE as usize * 2];
    bytes[0..8].copy_from_slice(b"ElfFile\0");
    bytes[42..44].copy_from_slice(&1u16.to_le_bytes());
    bytes[120..124].copy_from_slice(&1u32.to_le_bytes());
    assert_eq!(bounded_clean_evtx_bytes(&bytes).len(), bytes.len());
}

#[test]
fn oversized_evtx_returns_error() {
    let bytes = vec![0; MAX_EVTX_ANALYSIS_BYTES + 1];
    assert!(matches!(
        extract_boot_shutdown_events(&bytes, SYSTEM_PATH),
        Err(EvtxBootError::InputTooLarge { .. })
    ));
}

#[test]
fn parses_real_system_evtx_fixture_boot_candidates() {
    let bytes = std::fs::read(testing::fixtures::tiny_system_evtx())
        .expect("read tiny System.evtx fixture");
    let extraction = extract_boot_shutdown_events(&bytes, SYSTEM_PATH)
        .expect("fixture extraction should succeed");
    assert!(
        extraction
            .events
            .iter()
            .any(|event| matches!(event.event_id, 6005 | 6006 | 6008 | 1074)),
        "warnings: {:?}",
        extraction.warnings
    );
    assert!(extraction.events.iter().all(|event| {
        event.source_path == SYSTEM_PATH
            && !event.timestamp.trim().is_empty()
            && !event.note.trim().is_empty()
    }));
}

#[test]
fn unsupported_path_returns_error() {
    assert!(matches!(
        extract_boot_shutdown_events(b"ElfFile\0", "Windows/Temp/UnknownChannel.evtx"),
        Err(EvtxBootError::UnsupportedPath { .. })
    ));
}

#[test]
#[ignore = "manual fixture regeneration helper"]
fn dump_fixture_to_expected_json() {
    let output = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/evtx/expected.json");
    let bytes = std::fs::read(testing::fixtures::tiny_system_evtx())
        .expect("read tiny System.evtx fixture");
    let extraction = extract_boot_shutdown_events(&bytes, SYSTEM_PATH)
        .expect("fixture extraction should succeed");
    std::fs::write(&output, serde_json::to_string_pretty(&extraction).unwrap())
        .expect("write expected.json");
}

fn operational_event(
    provider: &str,
    event_id: u32,
    record_id: u64,
    data: serde_json::Value,
) -> serde_json::Value {
    json!({"Event":{
        "System":{
            "Provider":{"@Name":provider},
            "EventID":event_id,
            "EventRecordID":record_id,
            "TimeCreated":{"@SystemTime":"2026-03-15T10:30:00Z"}
        },
        "EventData":{"Data":data}
    }})
}

#[test]
fn extract_powershell_4104_script_block_from_json() {
    let record = operational_event(
        "Microsoft-Windows-PowerShell",
        4104,
        100,
        json!([
            {"@Name":"ScriptBlockText","#text":"Get-Process | Where-Object {$_.CPU -gt 10}"},
            {"@Name":"Path","#text":"C:\\Scripts\\audit.ps1"}
        ]),
    );
    let extraction = extract_boot_shutdown_events_from_json_records(
        &[record],
        "Microsoft-Windows-PowerShell%4Operational.evtx",
    )
    .expect("json extraction should succeed");
    let event = &extraction.events[0];
    assert_eq!(event.kind, EvtxBootEventKind::PowerShellScriptBlock);
    assert_eq!(
        event.details.get("ScriptBlockText").map(String::as_str),
        Some("Get-Process | Where-Object {$_.CPU -gt 10}")
    );
}

#[test]
fn extract_sysmon_1_process_create_from_json() {
    let record = operational_event(
        "Microsoft-Windows-Sysmon",
        1,
        200,
        json!([
            {"@Name":"Image","#text":"C:\\Windows\\System32\\cmd.exe"},
            {"@Name":"CommandLine","#text":"cmd.exe /c whoami"}
        ]),
    );
    let extraction = extract_boot_shutdown_events_from_json_records(
        &[record],
        "Microsoft-Windows-Sysmon%4Operational.evtx",
    )
    .expect("json extraction should succeed");
    assert_eq!(
        extraction.events[0].kind,
        EvtxBootEventKind::SysmonProcessCreate
    );
    assert_eq!(
        extraction.events[0]
            .details
            .get("CommandLine")
            .map(String::as_str),
        Some("cmd.exe /c whoami")
    );
}

#[test]
fn extract_rdp_21_session_connect_from_json() {
    let record = operational_event(
        "Microsoft-Windows-TerminalServices-LocalSessionManager",
        21,
        300,
        json!([
            {"@Name":"User","#text":"DOMAIN\\jsmith"},
            {"@Name":"Address","#text":"192.168.1.100"}
        ]),
    );
    let extraction = extract_boot_shutdown_events_from_json_records(
        &[record],
        "Microsoft-Windows-TerminalServices-LocalSessionManager%4Operational.evtx",
    )
    .expect("json extraction should succeed");
    assert_eq!(
        extraction.events[0].kind,
        EvtxBootEventKind::RdpSessionConnect
    );
    assert_eq!(
        extraction.events[0]
            .details
            .get("Address")
            .map(String::as_str),
        Some("192.168.1.100")
    );
}

#[test]
fn extract_defender_1116_threat_from_json() {
    let record = operational_event(
        "Microsoft-Windows-Windows Defender",
        1116,
        400,
        json!([
            {"@Name":"Threat","#text":"Trojan:Win32/Malware"},
            {"@Name":"Severity","#text":"Severe"}
        ]),
    );
    let extraction = extract_boot_shutdown_events_from_json_records(
        &[record],
        "Microsoft-Windows-Windows Defender%4Operational.evtx",
    )
    .expect("json extraction should succeed");
    assert_eq!(
        extraction.events[0].kind,
        EvtxBootEventKind::DefenderThreatDetected
    );
    assert_eq!(
        extraction.events[0]
            .details
            .get("Threat")
            .map(String::as_str),
        Some("Trojan:Win32/Malware")
    );
}

#[test]
fn event_data_map_handles_various_json_shapes() {
    let legacy = json!({"EventData":{"Data":[
        {"@Name":"Image","#text":"cmd.exe"},
        {"Name":"CommandLine","#text":"/c dir"},
        {"@Name":"ThReAt","#text":"malware"}
    ]}});
    let map = event_data_map(&legacy);
    assert_eq!(map.get("Image").map(String::as_str), Some("cmd.exe"));
    assert_eq!(map.get("CommandLine").map(String::as_str), Some("/c dir"));

    let flattened = json!({"EventData":{"Image":"cmd.exe","CommandLine":"/c dir","LogonType":3}});
    let map = event_data_map(&flattened);
    assert_eq!(map.get("LogonType").map(String::as_str), Some("3"));
}

fn structured_record(
    channel: &str,
    event_id: u32,
    record_id: u64,
    event_data: serde_json::Value,
) -> serde_json::Value {
    json!({"Event":{
        "System":{
            "Provider":{"@Name":format!("{channel} Provider")},
            "EventID":event_id,
            "EventRecordID":record_id,
            "Channel":channel,
            "TimeCreated":{"@SystemTime":"2026-03-15T08:00:00Z"}
        },
        "EventData":event_data
    }})
}

#[test]
fn extract_security_4624_flattened_event_data() {
    let record = structured_record(
        "Security",
        4624,
        1,
        json!({
            "TargetUserName":"jdoe",
            "LogonType":3,
            "IpAddress":"192.168.1.10",
            "WorkstationName":"DESKTOP-ABC",
            "Status":"0x0"
        }),
    );
    let extraction = extract_structured_events_from_json_records(&[record], SECURITY_PATH)
        .expect("json extraction should succeed");
    let event = &extraction.security_events[0];
    assert_eq!(event.kind, EvtxSecurityEventKind::LogonSuccess);
    assert_eq!(event.target_user.as_deref(), Some("jdoe"));
    assert_eq!(event.logon_type.as_deref(), Some("3"));
    assert_eq!(event.ip_address.as_deref(), Some("192.168.1.10"));
}

#[test]
fn extract_security_4625_failure_from_flattened_event_data() {
    let record = structured_record(
        "Security",
        4625,
        2,
        json!({"TargetUserName":"admin","LogonType":10,"Status":"0xC000006D"}),
    );
    let extraction = extract_structured_events_from_json_records(&[record], SECURITY_PATH)
        .expect("json extraction should succeed");
    let event = &extraction.security_events[0];
    assert_eq!(event.kind, EvtxSecurityEventKind::LogonFailure);
    assert_eq!(event.failure_reason.as_deref(), Some("0xC000006D"));
}

#[test]
fn extract_security_event_data_map_merges_legacy_and_flattened_shapes() {
    let wrapper = json!({"EventData":{
        "TargetUserName":"flattened",
        "Data":[
            {"@Name":"LogonType","#text":"2"},
            {"@Name":"MissingFromFlat","#text":"legacy"}
        ]
    }});
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
    let record = structured_record(
        "Application",
        1000,
        10,
        json!({"AppName":"chrome.exe","ModuleName":"ntdll.dll"}),
    );
    let extraction = extract_structured_events_from_json_records(&[record], APPLICATION_PATH)
        .expect("json extraction should succeed");
    let event = &extraction.application_events[0];
    assert_eq!(event.kind, EvtxApplicationEventKind::ApplicationCrash);
    assert_eq!(event.application.as_deref(), Some("chrome.exe"));
    assert_eq!(event.fault_module.as_deref(), Some("ntdll.dll"));
}

#[test]
fn extract_security_4688_prefers_parent_process_name() {
    let record = structured_record(
        "Security",
        4688,
        101,
        json!({
            "NewProcessName":"C:\\Windows\\System32\\cmd.exe",
            "ParentProcessName":"C:\\Windows\\explorer.exe",
            "CreatorProcessName":"ignored.exe"
        }),
    );
    let extraction = extract_structured_events_from_json_records(&[record], SECURITY_PATH)
        .expect("json extraction should succeed");
    assert_eq!(
        extraction.security_events[0].parent_process_name.as_deref(),
        Some("C:\\Windows\\explorer.exe")
    );
}

#[test]
fn extract_security_4688_falls_back_to_creator_process_name() {
    let record = structured_record(
        "Security",
        4688,
        102,
        json!({
            "NewProcessName":"powershell.exe",
            "CreatorProcessName":"C:\\Windows\\System32\\cmd.exe"
        }),
    );
    let extraction = extract_structured_events_from_json_records(&[record], SECURITY_PATH)
        .expect("json extraction should succeed");
    assert_eq!(
        extraction.security_events[0].parent_process_name.as_deref(),
        Some("C:\\Windows\\System32\\cmd.exe")
    );
}

#[test]
fn extract_application_1000_unnamed_data_positions() {
    let record = structured_record(
        "Application",
        1000,
        11,
        json!({"Data":["chrome.exe","12.0.0.0","63a1b2c3","ntdll.dll","10.0.19041.0"]}),
    );
    let extraction = extract_structured_events_from_json_records(&[record], APPLICATION_PATH)
        .expect("json extraction should succeed");
    assert_eq!(
        extraction.application_events[0].application.as_deref(),
        Some("chrome.exe")
    );
    assert_eq!(
        extraction.application_events[0].fault_module.as_deref(),
        Some("ntdll.dll")
    );
}

#[test]
fn extract_application_1001_named_wer_p1_p4_fields() {
    let record = structured_record(
        "Application",
        1001,
        12,
        json!({"P1":"notepad.exe","P4":"kernelbase.dll"}),
    );
    let extraction = extract_structured_events_from_json_records(&[record], APPLICATION_PATH)
        .expect("json extraction should succeed");
    let event = &extraction.application_events[0];
    assert_eq!(event.kind, EvtxApplicationEventKind::WindowsErrorReporting);
    assert_eq!(event.application.as_deref(), Some("notepad.exe"));
    assert_eq!(event.fault_module.as_deref(), Some("kernelbase.dll"));
}

#[test]
fn extract_application_1033_unnamed_data_positions() {
    let record = structured_record(
        "Application",
        1033,
        13,
        json!({"Data":["ForensicsWorkbench","1.0.0","1033","0","Contoso Inc.","(none)"]}),
    );
    let extraction = extract_structured_events_from_json_records(&[record], APPLICATION_PATH)
        .expect("json extraction should succeed");
    assert_eq!(
        extraction.application_events[0].product_name.as_deref(),
        Some("ForensicsWorkbench")
    );
    assert_eq!(
        extraction.application_events[0].manufacturer.as_deref(),
        Some("Contoso Inc.")
    );
}

#[test]
fn boot_events_dont_have_details() {
    let extraction = extract_boot_shutdown_events_from_json_records(
        &[json!({"Event":{"System":{
            "Provider":{"@Name":"EventLog"},
            "EventID":6005,
            "TimeCreated":{"@SystemTime":"2026-01-01T00:00:00Z"}
        }}})],
        SYSTEM_PATH,
    )
    .expect("json extraction should succeed");
    assert!(extraction.events[0].details.is_empty());
}
