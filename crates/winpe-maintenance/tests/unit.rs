use std::fs;
use std::path::Path;

use winpe_maintenance::{
    find_single_windows_installation, inspect_osdata, remove_osdata, MaintenanceError, OsdataState,
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
