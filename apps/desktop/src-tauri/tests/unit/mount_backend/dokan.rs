use super::{dokan_drive_mount_point, parse_drive_letter, validate_drive_letter};

#[test]
fn dokan_drive_mount_point_uses_a_root_path() {
    assert_eq!(dokan_drive_mount_point(b'M'), "M:\\");
}

#[test]
fn parse_drive_letter_accepts_display_and_root_forms() {
    for value in ["M:", "m:", "M:\\"] {
        assert_eq!(
            parse_drive_letter(value).expect("drive letter must parse"),
            b'M'
        );
    }
}

#[test]
fn validate_drive_letter_rejects_non_drive_paths_before_host_queries() {
    let error = validate_drive_letter("M:\\folder").expect_err("folder paths are not v1 inputs");
    assert!(error.to_string().contains("v1 accepts a drive letter"));
}
