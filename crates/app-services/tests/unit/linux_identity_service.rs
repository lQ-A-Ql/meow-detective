use super::*;

fn filesystem(bytes: &[u8]) -> (fs_ext4::Ext4Reader, tempfile::TempDir) {
    let workspace = tempfile::tempdir().unwrap();
    let path = workspace.path().join("identity.raw");
    std::fs::write(&path, bytes).unwrap();
    let reader = evidence_core::RawImageReader::open(&path).unwrap();
    (
        fs_ext4::Ext4Reader::open(Box::new(reader), 0).unwrap(),
        workspace,
    )
}

#[test]
fn selected_identity_obeys_priority_and_resolves_evidence_symlinks() {
    let image = testing::builders::ext4::linux_root_ext4_image_os_release_files(
        b"ID=primary\nPRETTY_NAME='Primary Linux'\n",
        b"ID=vendor\nPRETTY_NAME='Vendor Linux'\n",
    );
    let (fs, _workspace) = filesystem(&image);
    assert_eq!(
        from_filesystem(&fs).unwrap().unwrap().id.as_deref(),
        Some("primary")
    );
    let image =
        testing::builders::ext4::linux_root_ext4_image_os_release_symlink(b"../usr/lib/os-release");
    let (fs, _workspace) = filesystem(&image);
    assert_eq!(
        from_filesystem(&fs).unwrap().unwrap().id.as_deref(),
        Some("centos")
    );
}

#[test]
fn selected_invalid_file_and_broken_link_return_errors() {
    let image = testing::builders::ext4::linux_root_ext4_image_os_release_files(
        b"ID=\"invalid",
        b"ID=vendor\n",
    );
    let (fs, _workspace) = filesystem(&image);
    assert!(from_filesystem(&fs).is_err());
    for target in [b"../../outside".as_slice(), b"os-release", b"../missing"] {
        let image = testing::builders::ext4::linux_root_ext4_image_os_release_symlink(target);
        let (fs, _workspace) = filesystem(&image);
        assert!(from_filesystem(&fs).is_err(), "{target:?}");
    }
}
