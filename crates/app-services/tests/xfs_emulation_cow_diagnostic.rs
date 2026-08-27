//! Read-only diagnostics for an emulation session's merged raw disk view.
//!
//! Set `FORENSICS_XFS_COW_E01_FIXTURE` to the session `mount/disk.raw` path. The
//! application must still own the session mount while this test runs.

use std::path::PathBuf;

use evidence_core::{EvidenceReader, FileSystemReader, RawImageReader};

const FIXTURE_ENV: &str = "FORENSICS_XFS_COW_E01_FIXTURE";
const PV_OFFSET: u64 = 1_074_790_400;

#[test]
#[ignore = "requires FORENSICS_XFS_COW_E01_FIXTURE emulation session disk"]
fn inspect_guest_boot_state_from_cow_view() {
    let path = PathBuf::from(std::env::var_os(FIXTURE_ENV).expect("fixture env is required"));
    let reader: Box<dyn EvidenceReader> =
        Box::new(RawImageReader::open(&path).expect("open merged COW raw view"));
    let pool = fs_lvm::LvmPool::discover(vec![reader], vec![PV_OFFSET])
        .expect("discover LVM in merged COW view");
    let volumes = pool.list_volumes();
    eprintln!("logical volumes: {volumes:#?}");
    let root_index = volumes
        .iter()
        .position(|volume| volume.name == "root")
        .expect("root LV should exist");
    let root = pool.open_volume(root_index).expect("open root LV");
    let fs = fs_xfs::XfsReader::open(Box::new(root), 0).expect("open root XFS");
    let snapshot = fs
        .read_internal_log_snapshot(fs_xfs::log::XFS_LOG_MAX_SNAPSHOT_BYTES)
        .expect("read root XFS log");
    eprintln!(
        "root XFS log state after guest boot: {:?}",
        fs_xfs::log::assess_log_state(&snapshot)
    );

    assert_clean_guest_mount(&fs);

    for path in [
        "var/log/messages",
        "var/log/boot.log",
        "var/log/dmesg",
        "var/log/secure",
        "etc/fstab",
        "etc/systemd/system/dbus.socket",
        "usr/lib/systemd/system/dbus.socket",
        "usr/lib/systemd/system/systemd-logind.service",
    ] {
        match fs.read_file_range(path, 0, 4 * 1024 * 1024) {
            Ok(bytes) => {
                eprintln!("===== /{path} ({} bytes) =====", bytes.len());
                print_diagnostic_lines(path, &String::from_utf8_lossy(&bytes));
            }
            Err(error) => eprintln!("===== /{path}: {error} ====="),
        }
    }
    print_persistent_journal(&fs);
}

fn assert_clean_guest_mount(fs: &dyn FileSystemReader) {
    let bytes = fs
        .read_file_range("var/log/messages", 0, 4 * 1024 * 1024)
        .expect("read guest messages log");
    let messages = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
    assert!(
        messages.contains("xfs (dm-0): ending clean mount"),
        "guest kernel did not record a clean root-XFS mount"
    );
    for failure in [
        "structure needs cleaning",
        "metadata i/o error",
        "xfs (dm-0): corruption",
    ] {
        assert!(
            !messages.contains(failure),
            "guest kernel recorded XFS failure: {failure}"
        );
    }
}

fn print_persistent_journal(fs: &dyn FileSystemReader) {
    let Ok(machine_dirs) = fs.list_children("var/log/journal") else {
        eprintln!("===== no persistent systemd journal =====");
        return;
    };
    for machine_dir in machine_dirs.into_iter().filter(|node| node.is_dir) {
        let Ok(files) = fs.list_children(&machine_dir.path) else {
            continue;
        };
        for file in files.into_iter().filter(|node| {
            !node.is_dir && (node.name.ends_with(".journal") || node.name.ends_with(".journal~"))
        }) {
            let length = usize::try_from(file.size.min(32 * 1024 * 1024)).unwrap_or(0);
            let Ok(bytes) = fs.read_file_range(&file.path, 0, length) else {
                continue;
            };
            let Ok(parsed) = artifacts_linux::parse_journal_full(&bytes) else {
                continue;
            };
            eprintln!(
                "===== /{} entries={} truncated={} corrupt={} =====",
                file.path,
                parsed.entries.len(),
                parsed.truncated,
                parsed.skipped_corrupt
            );
            for entry in parsed.entries.into_iter().rev().take(2_000).rev() {
                let unit = entry.systemd_unit.as_deref().unwrap_or_default();
                let message = entry.message.as_deref().unwrap_or_default();
                let folded = format!("{unit} {message}").to_ascii_lowercase();
                if [
                    "xfs",
                    "dbus",
                    "logind",
                    "sshd",
                    "read-only",
                    "corrupt",
                    "failed",
                ]
                .iter()
                .any(|term| folded.contains(term))
                {
                    eprintln!(
                        "{} boot={} unit={} {}",
                        entry
                            .timestamp
                            .map(|value| value.to_rfc3339())
                            .unwrap_or_default(),
                        entry.boot_id.as_deref().unwrap_or_default(),
                        unit,
                        message
                    );
                }
            }
        }
    }
}

fn print_diagnostic_lines(path: &str, text: &str) {
    const TERMS: &[&str] = &[
        "xfs",
        "dbus",
        "logind",
        "read-only",
        "structure needs cleaning",
        "metadata i/o",
        "corrupt",
        "failed",
        "error",
        "mounted /",
        "reached target multi-user",
        "root login",
        "server listening",
    ];
    if !path.starts_with("var/log/") || path == "var/log/boot.log" {
        eprintln!("{text}");
        return;
    }
    let lines = text.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(1_000);
    for line in &lines[start..] {
        let folded = line.to_ascii_lowercase();
        if TERMS.iter().any(|term| folded.contains(term)) {
            eprintln!("{line}");
        }
    }
}
