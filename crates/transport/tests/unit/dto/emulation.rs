use super::{EmulationControlModeDto, PrepareEmulationRequestDto};
#[test]
fn prepare_request_validates_the_source_and_optional_iso() {
    let request = PrepareEmulationRequestDto {
        data_source_id: "source-1".to_string(),
        recovery_iso_path: Some("C:\\Tools\\LaoMaoTao.iso".to_string()),
        allow_direct_boot: false,
        options: Default::default(),
    };
    assert!(request.validate().is_ok());
    assert!(PrepareEmulationRequestDto {
        data_source_id: "".to_string(),
        recovery_iso_path: None,
        allow_direct_boot: false,
        options: Default::default(),
    }
    .validate()
    .is_err());
}

#[test]
fn prepare_request_requires_one_unambiguous_boot_authorization() {
    let direct_boot = PrepareEmulationRequestDto {
        data_source_id: "source-1".to_string(),
        recovery_iso_path: None,
        allow_direct_boot: true,
        options: Default::default(),
    };
    assert!(direct_boot.validate().is_ok());

    let silent_direct_boot = PrepareEmulationRequestDto {
        allow_direct_boot: false,
        ..direct_boot.clone()
    };
    assert!(silent_direct_boot.validate().is_err());

    let ambiguous = PrepareEmulationRequestDto {
        recovery_iso_path: Some("C:\\Tools\\WinPE.iso".to_string()),
        allow_direct_boot: true,
        ..direct_boot
    };
    assert!(ambiguous.validate().is_err());
}

#[test]
fn prepare_request_rejects_out_of_range_guest_resources() {
    use super::{EmulationNetworkModeDto, EmulationOptionsDto};

    let base = PrepareEmulationRequestDto {
        data_source_id: "source-1".to_string(),
        recovery_iso_path: None,
        allow_direct_boot: true,
        options: Default::default(),
    };
    let zero_cores = PrepareEmulationRequestDto {
        options: EmulationOptionsDto {
            processor_count: 0,
            ..Default::default()
        },
        ..base.clone()
    };
    assert!(zero_cores.validate().is_err());
    let tiny_memory = PrepareEmulationRequestDto {
        options: EmulationOptionsDto {
            memory_mib: 128,
            ..Default::default()
        },
        ..base
    };
    assert!(tiny_memory.validate().is_err());

    let value = serde_json::to_value(EmulationNetworkModeDto::HostOnly).unwrap();
    assert_eq!(value, "hostOnly");
    let options = EmulationOptionsDto::default();
    let value = serde_json::to_value(options).unwrap();
    assert_eq!(value["networkMode"], "off");
    assert_eq!(value["processorCount"], 2);
    assert_eq!(value["memoryMib"], 4096);
}

#[test]
fn control_mode_serializes_as_interactive_only() {
    let value = serde_json::to_value(EmulationControlModeDto::InteractiveOnly).unwrap();
    assert_eq!(value, "interactiveOnly");
}

#[test]
fn install_dto_omits_unknown_osdata_emptiness() {
    use super::EmulationInstallDto;

    let install = EmulationInstallDto {
        partition_index: 2,
        platform: Default::default(),
        osdata_present: true,
        sam_present: true,
        utilman_bypass_available: true,
        osdata_empty: None,
        os_release_pretty_name: None,
        kernel_present: None,
        fstab_present: None,
        boot_risk_notes: Vec::new(),
    };
    let value = serde_json::to_value(&install).unwrap();
    assert_eq!(value["partitionIndex"], 2);
    assert_eq!(value["platform"], "windows");
    assert!(value.get("osdataEmpty").is_none());
    let restored: EmulationInstallDto = serde_json::from_value(value).unwrap();
    assert_eq!(restored, install);

    let value = serde_json::to_value(EmulationInstallDto {
        osdata_empty: Some(false),
        ..install.clone()
    })
    .unwrap();
    assert_eq!(value["osdataEmpty"], false);
}

#[test]
fn linux_install_serializes_linux_fields_and_omits_windows_ones() {
    use super::{EmulationInstallDto, EmulationInstallPlatformDto};

    let install = EmulationInstallDto {
        partition_index: 5,
        platform: EmulationInstallPlatformDto::Linux,
        osdata_present: false,
        sam_present: false,
        utilman_bypass_available: false,
        osdata_empty: None,
        os_release_pretty_name: Some("CentOS Linux 7 (Core)".to_string()),
        kernel_present: Some(true),
        fstab_present: Some(true),
        boot_risk_notes: vec!["btrfs-root".to_string()],
    };
    let value = serde_json::to_value(&install).unwrap();
    assert_eq!(value["platform"], "linux");
    assert_eq!(value["osReleasePrettyName"], "CentOS Linux 7 (Core)");
    assert_eq!(value["kernelPresent"], true);
    assert_eq!(value["fstabPresent"], true);
    assert_eq!(value["bootRiskNotes"][0], "btrfs-root");
    let restored: EmulationInstallDto = serde_json::from_value(value).unwrap();
    assert_eq!(restored, install);

    // Payloads from before the platform field existed keep parsing.
    let legacy = serde_json::json!({
        "partitionIndex": 2,
        "osdataPresent": true,
        "samPresent": true,
        "utilmanBypassAvailable": false
    });
    let restored: EmulationInstallDto = serde_json::from_value(legacy).unwrap();
    assert_eq!(restored.platform, EmulationInstallPlatformDto::Windows);
}

#[test]
fn osdata_cleanup_round_trip_uses_camel_case() {
    use super::{
        EmulationOsdataCleanupDto, EmulationOsdataCleanupRequestDto, EmulationOsdataCleanupStateDto,
    };

    let request = EmulationOsdataCleanupRequestDto {
        session_id: "emulation-1".to_string(),
        partition_index: 2,
    };
    let value = serde_json::to_value(&request).unwrap();
    assert_eq!(value["sessionId"], "emulation-1");
    assert_eq!(value["partitionIndex"], 2);
    let restored: EmulationOsdataCleanupRequestDto = serde_json::from_value(value).unwrap();
    assert_eq!(restored, request);

    let result = EmulationOsdataCleanupDto {
        session_id: "emulation-1".to_string(),
        data_source_id: "source-1".to_string(),
        partition_index: 2,
        state: EmulationOsdataCleanupStateDto::Removed,
        edits_applied: 2,
    };
    let value = serde_json::to_value(&result).unwrap();
    assert_eq!(value["state"], "removed");
    assert_eq!(value["editsApplied"], 2);
    let restored: EmulationOsdataCleanupDto = serde_json::from_value(value).unwrap();
    assert_eq!(restored, result);
}

#[test]
fn linux_bypass_dtos_round_trip_in_camel_case() {
    use super::{
        EmulationLinuxAccountDto, EmulationLinuxBypassRequestDto, EmulationLinuxBypassResultDto,
    };

    let account = EmulationLinuxAccountDto {
        username: "root".to_string(),
        has_password: true,
        locked: false,
    };
    let value = serde_json::to_value(&account).unwrap();
    assert_eq!(value["hasPassword"], true);
    let restored: EmulationLinuxAccountDto = serde_json::from_value(value).unwrap();
    assert_eq!(restored, account);

    let request = EmulationLinuxBypassRequestDto {
        session_id: "emulation-1".to_string(),
        partition_index: 5,
        username: "root".to_string(),
    };
    let value = serde_json::to_value(&request).unwrap();
    assert_eq!(value["partitionIndex"], 5);
    let restored: EmulationLinuxBypassRequestDto = serde_json::from_value(value).unwrap();
    assert_eq!(restored, request);

    let result = EmulationLinuxBypassResultDto {
        session_id: "emulation-1".to_string(),
        data_source_id: "source-1".to_string(),
        partition_index: 5,
        username: "root".to_string(),
        password_cleared: true,
        already_passwordless: false,
    };
    let value = serde_json::to_value(&result).unwrap();
    assert_eq!(value["passwordCleared"], true);
    let restored: EmulationLinuxBypassResultDto = serde_json::from_value(value).unwrap();
    assert_eq!(restored, result);
}
