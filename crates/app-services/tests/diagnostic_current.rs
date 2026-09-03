use evidence_core::{EvidenceReader, FileSystemReader, RawImageReader};
use std::io::Read;
use std::path::PathBuf;

#[test]
#[ignore]
fn dump_current_boot_window() {
    let path = PathBuf::from(std::env::var_os("FORENSICS_XFS_COW_E01_FIXTURE").unwrap());
    let reader: Box<dyn EvidenceReader> = Box::new(RawImageReader::open(&path).unwrap());
    let pool = fs_lvm::LvmPool::discover(vec![reader], vec![1_074_790_400]).unwrap();
    let index = pool
        .list_volumes()
        .iter()
        .position(|v| v.name == "root")
        .unwrap();
    let root = pool.open_volume(index).unwrap();
    let fs = fs_xfs::XfsReader::open(Box::new(root), 0).unwrap();
    for path in [
        "var/log/messages",
        "var/log/boot.log",
        "var/log/secure",
        "etc/ssh/sshd_config",
        "etc/shadow",
    ] {
        let mut file = fs.open_file(path).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        println!("=== {path} bytes={} ===", bytes.len());
        let text = String::from_utf8_lossy(&bytes);
        for (number, line) in text.lines().enumerate() {
            if path == "etc/shadow"
                || (path == "etc/ssh/sshd_config" && (60..=70).contains(&(number + 1)))
                || line.contains("May 14 10:3")
                || line.contains("May 14 10:4")
            {
                println!("{line}");
            }
        }
    }
}
