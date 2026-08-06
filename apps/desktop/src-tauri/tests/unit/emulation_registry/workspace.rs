use super::{encode_hex, EmulationProvenance, SessionWorkspace};

#[test]
fn session_workspace_contains_only_derived_paths() {
    let case = tempfile::tempdir().unwrap();
    let workspace = SessionWorkspace::create(
        case.path(),
        "emulation-00000000-0000-4000-8000-000000000000",
    )
    .unwrap();

    assert!(workspace
        .root()
        .starts_with(case.path().canonicalize().unwrap()));
    assert!(workspace.mount_point().is_dir());
    assert!(!workspace.overlay_path().exists());
    workspace.write_vmdk("descriptor").unwrap();
    assert!(workspace.write_vmdk("replacement").is_err());
}

#[test]
fn provenance_hex_encoding_is_lowercase_and_exact() {
    assert_eq!(encode_hex(&[0x00, 0xab, 0xff]), "00abff");
    let _ = std::mem::size_of::<EmulationProvenance<'_>>();
}

#[test]
fn session_workspace_rejects_non_uuid_directory_names() {
    let case = tempfile::tempdir().unwrap();
    assert!(SessionWorkspace::create(case.path(), "emulation-..\\escape").is_err());
    assert!(SessionWorkspace::create(case.path(), "unscoped-session").is_err());
}
