use std::fs;
use std::path::Path;

use winpe_maintenance::{
    apply_bypass, crosscheck_install, find_single_windows_installation, inspect_bypass,
    inspect_osdata, remove_osdata, restore_bypass, split_drive_flag, utilman_bypass_available,
    BypassState, CrosscheckMismatch, InstallTarget, MaintenanceError, OsdataState,
};

#[test]
fn finds_exactly_one_offline_windows_installation() {
    let directory = tempfile::tempdir().unwrap();
    let windows = directory.path().join("windows-volume");
    create_windows_root(&windows);

    assert_eq!(
        find_single_windows_installation(vec![directory.path().join("empty"), windows.clone()])
            .unwrap(),
        windows
    );
}

#[test]
fn removes_an_osdata_file_without_touching_the_system_hive() {
    let directory = tempfile::tempdir().unwrap();
    create_windows_root(directory.path());
    let osdata = directory.path().join("Windows/System32/config/OSDATA");
    fs::write(&osdata, []).unwrap();

    assert_eq!(remove_osdata(directory.path()).unwrap(), OsdataState::File);
    assert!(!osdata.exists());
    assert!(directory
        .path()
        .join("Windows/System32/config/SYSTEM")
        .is_file());
}

#[test]
fn removes_an_empty_osdata_directory_but_refuses_a_non_empty_one() {
    let directory = tempfile::tempdir().unwrap();
    create_windows_root(directory.path());
    let osdata = directory.path().join("Windows/System32/config/OSDATA");
    fs::create_dir(&osdata).unwrap();
    assert_eq!(
        inspect_osdata(directory.path()).unwrap(),
        OsdataState::EmptyDirectory
    );
    assert_eq!(
        remove_osdata(directory.path()).unwrap(),
        OsdataState::EmptyDirectory
    );

    fs::create_dir(&osdata).unwrap();
    fs::write(osdata.join("unexpected.bin"), b"data").unwrap();
    assert!(matches!(
        remove_osdata(directory.path()),
        Err(MaintenanceError::OsdataDirectoryNotEmpty)
    ));
}

fn create_windows_root(root: &Path) {
    let config = root.join("Windows/System32/config");
    fs::create_dir_all(&config).unwrap();
    fs::write(config.join("SYSTEM"), b"regf").unwrap();
}

fn create_utilman_pair(root: &Path) {
    let system32 = root.join("Windows/System32");
    fs::create_dir_all(&system32).unwrap();
    fs::write(system32.join("utilman.exe"), b"original-utilman").unwrap();
    fs::write(system32.join("cmd.exe"), b"command-shell").unwrap();
}

#[test]
fn bypass_apply_and_restore_round_trip() {
    let directory = tempfile::tempdir().unwrap();
    create_windows_root(directory.path());
    create_utilman_pair(directory.path());

    assert_eq!(
        inspect_bypass(directory.path()).unwrap(),
        BypassState::NotApplied
    );
    assert_eq!(
        apply_bypass(directory.path()).unwrap(),
        BypassState::Applied
    );
    assert_eq!(
        fs::read(directory.path().join("Windows/System32/utilman.exe")).unwrap(),
        b"command-shell"
    );
    assert_eq!(
        fs::read(
            directory
                .path()
                .join("Windows/System32/utilman.exe.meowbak")
        )
        .unwrap(),
        b"original-utilman"
    );
    assert!(matches!(
        apply_bypass(directory.path()),
        Err(MaintenanceError::BypassBackupExists)
    ));
    assert_eq!(
        restore_bypass(directory.path()).unwrap(),
        BypassState::NotApplied
    );
    assert_eq!(
        fs::read(directory.path().join("Windows/System32/utilman.exe")).unwrap(),
        b"original-utilman"
    );
    assert!(!directory
        .path()
        .join("Windows/System32/utilman.exe.meowbak")
        .exists());
}

#[test]
fn bypass_apply_requires_both_binaries_and_restore_requires_a_backup() {
    let directory = tempfile::tempdir().unwrap();
    create_windows_root(directory.path());

    assert!(matches!(
        apply_bypass(directory.path()),
        Err(MaintenanceError::BypassTargetMissing)
    ));
    assert!(matches!(
        restore_bypass(directory.path()),
        Err(MaintenanceError::BypassBackupMissing)
    ));
}

#[test]
fn refuses_ambiguous_windows_installations() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first");
    let second = directory.path().join("second");
    create_windows_root(&first);
    create_windows_root(&second);

    let roots = vec![first, second];
    assert!(matches!(
        find_single_windows_installation(roots),
        Err(MaintenanceError::MultipleWindowsInstallations)
    ));
}

#[test]
fn drive_flag_selects_the_installation_root_explicitly() {
    let (arguments, drive) = split_drive_flag(vec![
        "run".to_string(),
        "--drive".to_string(),
        "d:".to_string(),
    ])
    .unwrap();
    assert_eq!(arguments, vec!["run".to_string()]);
    assert_eq!(drive.unwrap(), Path::new("D:\\"));

    let (arguments, drive) = split_drive_flag(vec!["run".to_string()]).unwrap();
    assert_eq!(arguments, vec!["run".to_string()]);
    assert!(drive.is_none());

    for arguments in [
        vec!["run".to_string(), "--drive".to_string()],
        vec!["--drive".to_string(), "1:".to_string()],
        vec!["--drive".to_string(), "DD".to_string()],
        vec!["--drive".to_string(), "D:".to_string(), "run".to_string()],
    ] {
        assert!(matches!(
            split_drive_flag(arguments),
            Err(MaintenanceError::InvalidDriveArgument)
        ));
    }
}

#[test]
fn crosscheck_reports_each_field_that_contradicts_the_manifest() {
    let install = InstallTarget {
        partition_index: 2,
        osdata_present: true,
        utilman_bypass_available: false,
    };

    assert!(crosscheck_install(&install, true, false).is_empty());
    assert_eq!(
        crosscheck_install(&install, false, true),
        vec![
            CrosscheckMismatch {
                field: "osdata_present",
                expected: true,
                observed: false,
            },
            CrosscheckMismatch {
                field: "utilman_bypass_available",
                expected: false,
                observed: true,
            },
        ]
    );
}

#[test]
fn utilman_bypass_availability_requires_both_binaries() {
    let directory = tempfile::tempdir().unwrap();
    create_windows_root(directory.path());
    assert!(!utilman_bypass_available(directory.path()));

    create_utilman_pair(directory.path());
    assert!(utilman_bypass_available(directory.path()));
}
