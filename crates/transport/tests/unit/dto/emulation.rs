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
fn control_mode_serializes_as_interactive_only() {
    let value = serde_json::to_value(EmulationControlModeDto::InteractiveOnly).unwrap();
    assert_eq!(value, "interactiveOnly");
}
