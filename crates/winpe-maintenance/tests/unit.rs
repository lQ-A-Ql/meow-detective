use std::fs;
use std::path::Path;

use winpe_maintenance::{
    apply_bypass, find_single_windows_installation, inspect_bypass, inspect_osdata, remove_osdata,
    restore_bypass, BypassState, MaintenanceError, OsdataState,
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
