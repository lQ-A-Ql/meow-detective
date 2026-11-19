//! Unit tests for the Linux rescue payload staging. These run without a Tauri
//! runtime: the pure ISO builder is parsed back directly, and staging only
//! touches a temporary session workspace.

use std::path::Path;

use evidence_emulation::{VmOptions, VmdkAdapter, VmwareFirmware};
use transport::dto::EmulationLinuxBypassBlockerDto;

use super::{
    build_rescue_payload_iso, rescue_iso_for_session, stage_rescue_payload, RescuePayloadInput,
    VmxBlueprint,
};
use crate::emulation_registry::workspace::SessionWorkspace;

const EDITED_SHADOW: &str = "root:$6$rounds=5000$salthash:19000:0:99999:7:::\n";

fn rescue_input<'a>() -> RescuePayloadInput<'a> {
    RescuePayloadInput {
        partition_index: 3,
        filesystem: "xfs",
        username: "root",
        blocker: EmulationLinuxBypassBlockerDto::SharedExtents,
        edited: EDITED_SHADOW,
    }
}

/// Minimal ISO9660 root-directory reader for the single-sector root table the
/// deterministic writer emits (PVD sector 16, root sector 20).
fn parse_root_files(image: &[u8]) -> Vec<(String, u32, u32)> {
    const ROOT_SECTOR: usize = 20;
    let dir = &image[ROOT_SECTOR * 2048..(ROOT_SECTOR + 1) * 2048];
    let mut files = Vec::new();
    let mut offset = 0usize;
    while offset < dir.len() && dir[offset] != 0 {
        let record_length = dir[offset] as usize;
        let flags = dir[offset + 25];
        if flags == 0 {
            let name_length = dir[offset + 32] as usize;
            let name =
                String::from_utf8_lossy(&dir[offset + 33..offset + 33 + name_length]).into_owned();
            let extent = u32::from_le_bytes(dir[offset + 2..offset + 6].try_into().unwrap());
            let size = u32::from_le_bytes(dir[offset + 10..offset + 14].try_into().unwrap());
            files.push((name, extent, size));
        }
        offset += record_length.max(1);
    }
    files
}

fn read_root_file(image: &[u8], files: &[(String, u32, u32)], name: &str) -> Vec<u8> {
    let (_, extent, size) = files
        .iter()
        .find(|(candidate, _, _)| candidate == name)
        .expect(name);
    let start = *extent as usize * 2048;
    image[start..start + *size as usize].to_vec()
}

#[test]
fn payload_iso_contains_three_files_and_consistent_manifest() {
    let image = build_rescue_payload_iso(&rescue_input()).expect("payload ISO builds");
    let files = parse_root_files(&image);
    let names: Vec<&str> = files.iter().map(|(name, _, _)| name.as_str()).collect();
    assert!(
        names.contains(&"SHADOW.NEW;1"),
        "shadow payload missing: {names:?}"
    );
    assert!(
        names.contains(&"BYPASS.JSON;1"),
        "manifest missing: {names:?}"
    );
    assert!(names.contains(&"BYPASS.SH;1"), "script missing: {names:?}");

    let shadow = read_root_file(&image, &files, "SHADOW.NEW;1");
    assert_eq!(shadow, EDITED_SHADOW.as_bytes());

    let manifest_bytes = read_root_file(&image, &files, "BYPASS.JSON;1");
    let manifest: serde_json::Value = serde_json::from_slice(&manifest_bytes).unwrap();
    assert_eq!(manifest["partitionIndex"], 3);
    assert_eq!(manifest["filesystem"], "xfs");
    assert_eq!(manifest["username"], "root");
    assert_eq!(manifest["blocker"], "sharedExtents");
    assert_eq!(manifest["sizeBytes"], EDITED_SHADOW.len());
    let expected_sha = infrastructure::hashing::sha256_bytes(EDITED_SHADOW.as_bytes());
    assert_eq!(manifest["shadowSha256"], expected_sha);
    assert_eq!(
        manifest["shadowSha256"],
        infrastructure::hashing::sha256_bytes(&shadow)
    );
}

#[test]
fn rescue_iso_for_session_absent_or_unreadable_returns_none() {
    assert!(rescue_iso_for_session(None).is_none());
    assert!(rescue_iso_for_session(Some(Path::new("Z:/missing/rescue.iso"))).is_none());
    let directory = tempfile::tempdir().unwrap();
    assert!(rescue_iso_for_session(Some(directory.path())).is_none());
}

#[test]
fn staging_payload_does_not_replace_maintenance_iso() {
    let case_root = tempfile::tempdir().unwrap();
    let workspace = SessionWorkspace::create(
        case_root.path(),
        "emulation-00000000-0000-4000-8000-000000000000",
    )
    .unwrap();
    workspace
        .write_maintenance_iso(b"windows-maintenance-bytes")
        .unwrap();
    workspace.write_vmx("machine stub").unwrap();

    let blueprint = VmxBlueprint::new(
        VmwareFirmware::Bios,
        "ubuntu-64".to_string(),
        VmdkAdapter::Ide,
        0,
        VmOptions::default(),
        None,
    );
    let payload_path =
        stage_rescue_payload(&workspace, &blueprint, &rescue_input()).expect("payload staged");
    assert!(payload_path.exists());
    assert!(workspace.maintenance_iso_present());

    let kept = std::fs::read(workspace.root().join("maintenance.iso")).unwrap();
    assert_eq!(kept, b"windows-maintenance-bytes");

    let vmx = std::fs::read_to_string(workspace.vmx_path()).unwrap();
    assert!(
        vmx.contains("linux-bypass-payload.iso"),
        "payload CD must be attached as the second optical drive: {vmx}"
    );
}
