use std::io::Read;
use std::path::Path;

use evidence_core::{EvidenceReader, FileSystemReader, RawImageReader};
use image_e01::E01Reader;
use sha2::{Digest, Sha256};

fn read_file(fs: &dyn FileSystemReader, path: &str) -> Option<Vec<u8>> {
    let mut file = fs.open_file(path).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
#[ignore]
fn compare_source_and_cow_files() {
    let source = std::env::var("FORENSICS_LINUX_E01_FIXTURE").unwrap();
    let cow = std::env::var("FORENSICS_XFS_COW_E01_FIXTURE").unwrap();
    let make_fs = |reader: Box<dyn EvidenceReader>| {
        let pool = fs_lvm::LvmPool::discover(vec![reader], vec![1_074_790_400]).unwrap();
        let root = pool
            .list_volumes()
            .iter()
            .position(|v| v.name == "root")
            .unwrap();
        fs_xfs::XfsReader::open(Box::new(pool.open_volume(root).unwrap()), 0).unwrap()
    };
    let source_fs = make_fs(Box::new(E01Reader::open(Path::new(&source)).unwrap()));
    let cow_fs = make_fs(Box::new(RawImageReader::open(Path::new(&cow)).unwrap()));
    for path in [
        "etc/shadow",
        "etc/fstab",
        "usr/lib/systemd/system/dbus.socket",
        "usr/lib/systemd/system/systemd-logind.service",
        "usr/lib/systemd/system/sshd.service",
        "usr/lib/systemd/system/nginx.service",
        "usr/lib/systemd/system/mysqld.service",
        "usr/lib/systemd/system/rsyslog.service",
        "usr/lib/systemd/system/NetworkManager.service",
        "var/log/messages",
        "var/log/boot.log",
        "var/log/secure",
        "etc/ssh/sshd_config",
        "sbin/init",
    ] {
        let left = read_file(&source_fs, path);
        let right = read_file(&cow_fs, path);
        println!(
            "{path}: source={} cow={} equal={}",
            left.as_ref().map_or("missing".into(), |v| digest(v)),
            right.as_ref().map_or("missing".into(), |v| digest(v)),
            left == right
        );
    }
}
