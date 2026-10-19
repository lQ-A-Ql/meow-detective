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
            derived_ntfs: None,
        },
        None,
    );
    let json = serde_json::to_value(value).unwrap();
    assert_eq!(json["diskAdapter"], "ide");
    assert_eq!(
        json["diskAdapterReason"],
        "initramfs contains ata_piix; selected IDE"
    );
    assert!(json.get("derivedNtfs").is_none());
}

#[test]
fn derived_provenance_distinguishes_source_hash_selected_volume_and_guest_view() {
    let derived = transport::dto::case::DerivedNtfsDiskDto {
        layout_version: 1,
        source_media_bytes: 2_000_396_321_280,
        source_volume_offset: 0,
        source_volume_bytes: 275_402_194_944,
        source_volume_serial: "4482D035A65A1162".into(),
        guest_partition_index: 1,
        guest_partition_offset: 1_048_576,
        guest_disk_bytes: 275_403_243_520,
        generated_disk_signature: 0x12345678,
        source_hidden_sectors: 649216,
        guest_hidden_sectors: 2048,
        generated_mbr_sha256: "ab".repeat(32),
        boot_repair_required: true,
    };
    let identity =
        evidence_emulation::ParentIdentity::new(derived.guest_disk_bytes, [0x12; 32]).unwrap();
    let json = serde_json::to_value(EmulationProvenance::new(
        ProvenanceIds {
            session_id: "session",
            case_id: "case",
            data_source_id: "source",
        },
        &identity,
        ProvenanceGuest {
            firmware: evidence_emulation::VmwareFirmware::Bios,
            options: evidence_emulation::VmOptions::default(),
            disk_adapter: evidence_emulation::VmdkAdapter::Ide,
            disk_adapter_reason: "Windows recovery",
            maintenance_media: true,
            derived_ntfs: Some(&derived),
        },
        None,
    ))
    .unwrap();
    assert_eq!(json["schemaVersion"], 2);
    assert_eq!(json["parentHashScope"], "importSourceFingerprint");
    assert_eq!(json["parentSha256"], "12".repeat(32));
    assert_eq!(json["logicalLength"], derived.guest_disk_bytes);
    let selection = &json["derivedNtfs"];
    assert_eq!(selection["sourceMediaBytes"], derived.source_media_bytes);
    assert_eq!(selection["sourceVolumeBytes"], derived.source_volume_bytes);
    assert_eq!(selection["guestPartitionOffset"], 1_048_576);
    assert_eq!(selection["sourceHiddenSectors"], 649216);
    assert_eq!(selection["guestHiddenSectors"], 2048);
    assert_eq!(selection["bootRepairRequired"], true);
    assert_eq!(selection["generatedMbrSha256"], "ab".repeat(32));
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
