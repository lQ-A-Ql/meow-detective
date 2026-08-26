use super::{encode_hex, EmulationProvenance, ProvenanceGuest, ProvenanceIds, SessionWorkspace};

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
fn provenance_records_the_selected_disk_adapter_and_reason() {
    let identity = evidence_emulation::ParentIdentity::new(512, [0xabu8; 32]).unwrap();
    let value = EmulationProvenance::new(
        ProvenanceIds {
            session_id: "session",
            case_id: "case",
            data_source_id: "source",
        },
        &identity,
        ProvenanceGuest {
            firmware: evidence_emulation::VmwareFirmware::Efi,
            options: evidence_emulation::VmOptions::default(),
            disk_adapter: evidence_emulation::VmdkAdapter::Ide,
            disk_adapter_reason: "initramfs contains ata_piix; selected IDE",
            maintenance_media: false,
        },
        None,
    );
    let json = serde_json::to_value(value).unwrap();
    assert_eq!(json["diskAdapter"], "ide");
    assert_eq!(
        json["diskAdapterReason"],
        "initramfs contains ata_piix; selected IDE"
    );
}

#[test]
fn session_workspace_rejects_non_uuid_directory_names() {
    let case = tempfile::tempdir().unwrap();
    assert!(SessionWorkspace::create(case.path(), "emulation-..\\escape").is_err());
    assert!(SessionWorkspace::create(case.path(), "unscoped-session").is_err());
}

#[test]
fn remove_best_effort_deletes_the_session_directory() {
    let case = tempfile::tempdir().unwrap();
    let workspace = SessionWorkspace::create(
        case.path(),
        "emulation-00000000-0000-4000-8000-000000000000",
    )
    .unwrap();
    workspace.write_vmdk("descriptor").unwrap();
    let root = workspace.root().to_path_buf();
    let base = root.parent().unwrap().to_path_buf();

    workspace.remove_best_effort();

    assert!(!root.exists());
    assert!(base.is_dir());
}
