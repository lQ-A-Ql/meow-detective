use super::ntuser::user_assist_artifacts;
use super::sam::sam_user_artifacts;
use super::system::{network_adapter_artifacts, shutdown_time_artifacts};
use super::warnings::govern_registry_warnings;
use super::*;
use crate::analysis_service::candidates::EvidenceCandidate;
use chrono::Utc;
use domain::FileEntryId;
use serde_json::Value;

fn candidate(path: &str) -> EvidenceCandidate {
    EvidenceCandidate {
        file_id: FileEntryId(path.to_string()),
        data_source_id: "ds-1".to_string(),
        partition_index: None,
        path: path.to_string(),
        size: 1024,
        encrypted: false,
        content_identity: format!("test:{path}"),
        modified_at: None,
        evidence_kind: "registry_hive".to_string(),
        parser: "registry".to_string(),
        category: "Registry".to_string(),
    }
}

#[test]
fn sam_user_artifact_carries_subject_attribution() {
    let ts = Utc::now();
    let user = artifacts_windows::SamUser {
        username: "alice".to_string(),
        rid: 1001,
        sid: "S-1-5-21-1-2-3-1001".to_string(),
        full_name: "Alice Liddell".to_string(),
        comment: String::new(),
        home_dir: String::new(),
        profile_path: "C:\\Users\\alice".to_string(),
        last_login: Some(ts),
        password_last_set: Some(ts),
        account_disabled: false,
        account_locked: false,
        admin_count: 0,
        login_count: 42,
        group_memberships: vec!["Users".to_string()],
        password_hash: Some(
            "aad3b435b51404eeaad3b435b51404ee:31d6cfe0d16ae931b73c59d7e0c089c0".to_string(),
        ),
        password_hash_type: Some("NTLM".to_string()),
    };
    let info = artifacts_windows::SamInfo {
        users: vec![user],
        groups: Vec::new(),
        password_policy: None,
        txlog_applied: false,
        txlog_timestamps: Vec::new(),
        warnings: Vec::new(),
    };
    let outcome = sam_user_artifacts(&candidate("Windows/System32/config/SAM"), &info);
    assert_eq!(outcome.artifacts.len(), 1);
    let art = &outcome.artifacts[0];
    assert_eq!(art.family, "RegistrySamUser");
    assert_eq!(
        art.source_object_id.as_ref().map(|id| id.0.as_str()),
        Some("Windows/System32/config/SAM")
    );
    assert_eq!(
        art.attrs.get("subjectSid"),
        Some(&Value::String("S-1-5-21-1-2-3-1001".to_string()))
    );
    assert_eq!(
        art.attrs.get("subjectUsername"),
        Some(&Value::String("alice".to_string()))
    );
    assert!(outcome.timeline_events.len() >= 2);
    assert!(outcome
        .timeline_events
        .iter()
        .any(|e| e.event_type == "REGISTRY_SAM_LAST_LOGIN"));
    assert!(outcome
        .timeline_events
        .iter()
        .any(|e| e.event_type == "REGISTRY_SAM_PASSWORD_LAST_SET"));
}

#[test]
fn ntuser_user_assist_derives_subject_username_and_emits_timeline() {
    let info = artifacts_windows::NtuserInfo {
        run_keys: Vec::new(),
        recent_docs: Vec::new(),
        ua_entries: vec![artifacts_windows::UserAssistEntry {
            executable_path: "C:\\Windows\\notepad.exe".to_string(),
            run_count: 3,
            last_run: Some("2026-06-01T10:00:00+00:00".to_string()),
            focus_time_ms: 1200,
            session_id: 0,
        }],
        typed_urls: Vec::new(),
        word_wheel_query: Vec::new(),
        mount_points: Vec::new(),
        open_save_mru: Vec::new(),
        last_visited_mru: Vec::new(),
        run_mru: Vec::new(),
        default_browser: None,
        txlog_applied: false,
        txlog_timestamps: Vec::new(),
        warnings: Vec::new(),
    };
    let outcome = user_assist_artifacts(&candidate("Users/alice/NTUSER.DAT"), &info);
    assert_eq!(outcome.artifacts.len(), 1);
    let art = &outcome.artifacts[0];
    assert_eq!(art.family, "RegistryUserAssist");
    assert_eq!(
        art.attrs.get("subjectUsername"),
        Some(&Value::String("alice".to_string()))
    );
    assert_eq!(outcome.timeline_events.len(), 1);
    assert_eq!(outcome.timeline_events[0].event_type, "FILE_EXECUTED");
    assert_eq!(
        outcome.timeline_events[0].attrs.get("executionEvidence"),
        Some(&Value::String("registry.user_assist".to_string()))
    );
}

#[test]
fn system_shutdown_emits_timeline_event() {
    let entries = vec![artifacts_windows::ShutdownTimeEntry {
        key_path: "ControlSet001\\Control\\Windows\\ShutdownTime".to_string(),
        shutdown_time: "2026-06-02T18:30:00+00:00".to_string(),
    }];
    let outcome = shutdown_time_artifacts(&candidate("Windows/System32/config/SYSTEM"), &entries);
    assert_eq!(outcome.artifacts.len(), 1);
    assert_eq!(outcome.artifacts[0].family, "RegistryShutdownTime");
    assert_eq!(outcome.timeline_events.len(), 1);
    assert_eq!(
        outcome.timeline_events[0].event_type,
        "REGISTRY_SYSTEM_SHUTDOWN"
    );
}

#[test]
fn network_adapter_artifact_preserves_physical_and_tcpip_fields() {
    let adapters = vec![artifacts_windows::NetworkAdapterInfo {
        guid: "{ADAPTER-GUID}".to_string(),
        name: Some("Ethernet".to_string()),
        description: Some("Intel Ethernet Controller".to_string()),
        mac_address: Some("00:11:22:33:44:55".to_string()),
        permanent_mac_address: Some("00:11:22:33:44:56".to_string()),
        ip_addresses: vec!["192.0.2.10".to_string()],
        subnet_masks: vec!["255.255.255.0".to_string()],
        gateways: vec!["192.0.2.1".to_string()],
        dhcp_server: Some("192.0.2.2".to_string()),
        dhcp_enabled: Some(true),
        dns_servers: vec!["192.0.2.53".to_string()],
        pnp_instance_id: Some("PCI\\VEN_8086&DEV_1234".to_string()),
        service_name: Some("e1dexpress".to_string()),
    }];

    let artifacts =
        network_adapter_artifacts(&candidate("Windows/System32/config/SYSTEM"), &adapters);
    let attrs = &artifacts[0].attrs;
    assert_eq!(attrs["description"], "Intel Ethernet Controller");
    assert_eq!(attrs["permanentMacAddress"], "00:11:22:33:44:56");
    assert_eq!(attrs["ipAddresses"][0], "192.0.2.10");
    assert_eq!(attrs["subnetMasks"][0], "255.255.255.0");
    assert_eq!(attrs["gateways"][0], "192.0.2.1");
    assert_eq!(attrs["pnpInstanceId"], "PCI\\VEN_8086&DEV_1234");
}

#[test]
fn invalid_hive_warning_is_governed_and_prefix_path_is_redacted() {
    let outcome = extract_registry_candidate(
        &candidate("C:\\evidence\\Windows\\System32\\config\\SYSTEM"),
        b"not-a-hive",
        None,
        None,
        None,
    );

    assert_eq!(
        outcome.warnings,
        vec!["[REG-SYSTEM] SYSTEM: SYSTEM is not a regf registry hive"]
    );
}

#[test]
fn warning_governance_preserves_order_deduplicates_and_caps() {
    let mut raw = vec![
        "SYSTEM txlog parse failed: first".to_string(),
        "SYSTEM txlog parse failed: first".to_string(),
    ];
    raw.extend((0..64).map(|index| format!("warning {index:02}")));

    let governed = govern_registry_warnings("Windows/System32/config/SYSTEM", raw);

    assert_eq!(governed.len(), 64);
    assert_eq!(
        governed.first().map(String::as_str),
        Some("[REG-TXLOG] Windows/System32/config/SYSTEM: SYSTEM txlog parse failed: first")
    );
    assert_eq!(
        governed.get(1).map(String::as_str),
        Some("[REG-WARN] Windows/System32/config/SYSTEM: warning 00")
    );
    assert_eq!(
        governed.last().map(String::as_str),
        Some("[REG-CAP] Windows/System32/config/SYSTEM: additional registry warnings suppressed")
    );
}

#[test]
fn warning_governance_keeps_exact_capacity_without_false_cap() {
    let raw = (0..64).map(|index| format!("warning {index:02}")).collect();

    let governed = govern_registry_warnings("Windows/System32/config/SYSTEM", raw);

    assert_eq!(governed.len(), 64);
    assert_eq!(
        governed.last().map(String::as_str),
        Some("[REG-WARN] Windows/System32/config/SYSTEM: warning 63")
    );
    assert!(governed
        .iter()
        .all(|warning| !warning.starts_with("[REG-CAP]")));
}

#[test]
fn valid_magic_with_corrupt_hive_preserves_primary_parse_error() {
    let outcome = extract_registry_candidate(
        &candidate("C:\\evidence\\Windows\\System32\\config\\SYSTEM"),
        b"regf-corrupt",
        None,
        None,
        None,
    );

    assert!(outcome
        .warnings
        .iter()
        .any(|warning| warning.contains("registry parse failed")));
    assert!(outcome
        .warnings
        .iter()
        .all(|warning| !warning.contains("C:\\evidence")));
}
