//! Explicit private case/WinPE validation, never a default host-side action.
use super::super::EmulationRegistry;
use evidence_core::FileSystemReader;
use std::{path::PathBuf, sync::Arc, time::Duration};

struct ReleaseSession {
    registry: EmulationRegistry,
    session: String,
}
impl Drop for ReleaseSession {
    fn drop(&mut self) {
        if let Err(error) = self.registry.release(&self.session) {
            eprintln!("private session cleanup requires attention: {error}");
        }
    }
}

#[test]
#[ignore = "requires explicit private case, recovery ISO, output path and FORENSICS_VALIDATE_VM"]
fn direct_volume_disk_and_winpe_session() {
    let root = PathBuf::from(std::env::var_os("FORENSICS_CASE_ROOT").expect("explicit case root"));
    let case = domain::CaseId(std::env::var("FORENSICS_CASE_ID").expect("explicit case ID"));
    let source = domain::DataSourceId(
        std::env::var("FORENSICS_DATA_SOURCE_ID").expect("explicit source ID"),
    );
    let iso =
        PathBuf::from(std::env::var_os("FORENSICS_RECOVERY_ISO").expect("explicit recovery ISO"));
    let output =
        PathBuf::from(std::env::var_os("FORENSICS_VM_OUTPUT").expect("explicit output directory"));
    assert_eq!(
        std::env::var("FORENSICS_VALIDATE_VM").as_deref(),
        Ok("1"),
        "explicit host VM action required"
    );
    std::fs::create_dir_all(&output).unwrap();
    std::fs::write(output.join("stage.txt"), "verifying-source-fingerprint").unwrap();
    let conn = rusqlite::Connection::open_with_flags(
        root.join("app.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    conn.pragma_update(None, "query_only", true).unwrap();
    let registry = EmulationRegistry::default();
    eprintln!("private VM preparation: validating recovery media and complete source fingerprint");
    let status = registry
        .prepare_session(
            &conn,
            &root,
            &case,
            &source,
            Some(&iso),
            evidence_emulation::VmOptions::default(),
        )
        .unwrap();
    let cleanup = ReleaseSession {
        registry: registry.clone(),
        session: status.session_id.clone(),
    };
    std::fs::write(output.join("stage.txt"), "validating-derived-disk").unwrap();
    let (disk, vmx, workspace) = {
        let entries = registry.entries.lock().unwrap();
        let entry = entries.get(&status.session_id).unwrap();
        (
            Arc::clone(&entry.disk),
            entry.workspace.vmx_path().to_path_buf(),
            entry.workspace.root().to_path_buf(),
        )
    };
    let mapping = disk
        .source_volume_mapping()
        .expect("derived volume mapping");
    let mut mbr = [0; 512];
    disk.read_exact_at(0, &mut mbr).unwrap();
    assert_eq!(&mbr[510..], &[0x55, 0xaa]);
    assert_eq!(
        u32::from_le_bytes(mbr[454..458].try_into().unwrap()) as u64 * 512,
        mapping.guest_volume_offset
    );
    {
        let reader = app_services::emulation_cow_reader::CowDiskReader::new(Arc::clone(&disk));
        let ntfs =
            fs_ntfs::NtfsReader::open(Box::new(reader), mapping.guest_volume_offset).unwrap();
        let nodes = ntfs.list_children("").unwrap();
        assert!(nodes
            .iter()
            .any(|node| node.is_dir && node.name.eq_ignore_ascii_case("Windows")));
        eprintln!(
            "derived disk recognized: guestBytes={} volumeOffset={} rootEntries={}",
            disk.len(),
            mapping.guest_volume_offset,
            nodes.len()
        );
    }
    drop(disk);
    for name in ["machine.vmx", "disk.vmdk", "provenance.json"] {
        std::fs::copy(workspace.join(name), output.join(name)).unwrap();
    }
    eprintln!("launching isolated recovery VM: {}", status.session_id);
    std::fs::write(
        output.join("session-path.txt"),
        vmx.to_string_lossy().as_bytes(),
    )
    .unwrap();
    registry.launch(&status.session_id).unwrap();
    std::fs::write(output.join("stage.txt"), "observing-recovery-vm").unwrap();
    std::thread::sleep(Duration::from_secs(90));
    let vmrun = std::env::var_os("FORENSICS_VMWARE_VMRUN").expect("explicit vmrun tool");
    let capture = std::process::Command::new(vmrun)
        .arg("captureScreen")
        .arg(&vmx)
        .arg(output.join("winpe.png"))
        .output()
        .unwrap();
    eprintln!(
        "guest screenshot exit={:?} detail={}",
        capture.status.code(),
        String::from_utf8_lossy(&capture.stderr)
    );
    let log = workspace.join("vmware.log");
    if log.is_file() {
        std::fs::copy(log, output.join("vmware.log")).unwrap();
    }
    let released = registry.release(&status.session_id).unwrap();
    assert_eq!(
        released.state,
        crate::emulation_registry::EmulationState::Released
    );
    drop(cleanup);
    assert!(!workspace.exists(), "derived session cleanup must finish");
    std::fs::write(output.join("stage.txt"), "released").unwrap();
}
