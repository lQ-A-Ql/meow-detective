use widestring::U16CString;
use winapi::shared::ntstatus::{STATUS_DISK_FULL, STATUS_OBJECT_NAME_NOT_FOUND};
use winapi::um::winnt;

use super::{bounded_read_length, extent_node, forbidden_access, map_emulation_error, ExtentNode};

#[test]
fn extent_namespace_contains_only_root_and_disk() {
    for value in ["\\", "\\\\", "\\disk.raw", "/DISK.RAW"] {
        let path = U16CString::from_str(value).expect("valid fixture path");
        assert!(extent_node(path.as_ucstr()).is_ok(), "{value}");
    }
    let extra = U16CString::from_str("\\other.raw").expect("valid fixture path");
    assert_eq!(
        extent_node(extra.as_ucstr()),
        Err(STATUS_OBJECT_NAME_NOT_FOUND)
    );
}

#[test]
fn root_write_and_destructive_disk_access_are_rejected() {
    assert!(forbidden_access(ExtentNode::Root, winnt::GENERIC_WRITE));
    assert!(!forbidden_access(ExtentNode::Disk, winnt::GENERIC_WRITE));
    assert!(forbidden_access(ExtentNode::Disk, winnt::GENERIC_ALL));
    assert!(forbidden_access(ExtentNode::Disk, winnt::DELETE));
    assert!(forbidden_access(ExtentNode::Disk, winnt::WRITE_DAC));
}

#[test]
fn reads_at_the_fixed_extent_end_are_shortened() {
    assert_eq!(bounded_read_length(900, 200, 1024).unwrap(), 124);
    assert_eq!(bounded_read_length(1024, 200, 1024).unwrap(), 0);
    assert!(bounded_read_length(-1, 1, 1024).is_err());
}

#[test]
fn out_of_bounds_writes_map_to_disk_full() {
    let status = map_emulation_error(evidence_emulation::EmulationError::OutOfBounds {
        offset: 1000,
        end: 1100,
        length: 1024,
    });
    assert_eq!(status, STATUS_DISK_FULL);
}
