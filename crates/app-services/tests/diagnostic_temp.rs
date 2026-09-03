use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use std::io::Read;
use std::path::Path;

#[test]
#[ignore]
fn dump_linux_boot_configuration() {
    let path = std::env::var("FORENSICS_LINUX_E01_FIXTURE").unwrap();
    let reader: Box<dyn EvidenceReader> = Box::new(E01Reader::open(Path::new(&path)).unwrap());
    let pool = fs_lvm::LvmPool::discover(vec![reader], vec![1_074_790_400]).unwrap();
    let index = pool
        .list_volumes()
        .iter()
        .position(|volume| volume.name == "root")
        .unwrap();
    let fs = fs_xfs::XfsReader::open(Box::new(pool.open_volume(index).unwrap()), 0).unwrap();
    let snapshot = fs
        .read_internal_log_snapshot(fs_xfs::log::XFS_LOG_MAX_SNAPSHOT_BYTES)
        .unwrap();
    println!(
        "=== XFS log: bytes={} complete={} state={:?} ===",
        snapshot.bytes.len(),
        snapshot.complete,
        fs_xfs::log::assess_log_state(&snapshot)
    );
    for path in [
        "etc/fstab",
        "etc/hosts",
        "etc/hostname",
        "etc/resolv.conf",
        "etc/sysconfig/network-scripts/ifcfg-ens33",
        "etc/systemd/system/net-monitor.service",
        "etc/systemd/system/multi-user.target.wants/NetworkManager.service",
        "etc/systemd/system/multi-user.target.wants/firewalld.service",
        "etc/systemd/system/multi-user.target.wants/sshd.service",
        "etc/systemd/system/multi-user.target.wants/nginx.service",
        "etc/systemd/system/multi-user.target.wants/php-fpm.service",
        "etc/systemd/system/multi-user.target.wants/mysqld.service",
        "etc/systemd/system/multi-user.target.wants/kdump.service",
        "etc/systemd/system/multi-user.target.wants/rsyslog.service",
        "etc/systemd/system/multi-user.target.wants/vmtoolsd.service",
        "etc/nginx/nginx.conf",
        "etc/php-fpm.d/www.conf",
        "etc/my.cnf",
        "etc/sysconfig/readonly-root",
        "usr/lib/systemd/system/rhel-readonly.service",
        "usr/lib/systemd/system/sshd.service",
        "usr/lib/systemd/system/nginx.service",
        "usr/lib/systemd/system/php-fpm.service",
        "usr/lib/systemd/system/mysqld.service",
        "usr/lib/systemd/system/rsyslog.service",
        "usr/lib/systemd/system/dbus.socket",
        "usr/lib/systemd/system/systemd-logind.service",
        "etc/default/grub",
        "boot/grub2/grub.cfg",
        "etc/selinux/config",
        "etc/ssh/sshd_config",
        "etc/ssh/ssh_host_rsa_key",
        "etc/ssh/ssh_host_ed25519_key",
        "etc/shadow",
        "etc/passwd",
        "etc/nsswitch.conf",
        "etc/pam.d/sshd",
        "etc/pam.d/login",
        "etc/pam.d/system-auth",
        "etc/pam.d/password-auth",
        "usr/lib/systemd/system/dbus.service",
        "usr/lib/systemd/system/firewalld.service",
        "usr/lib/systemd/system/NetworkManager.service",
        "usr/lib/systemd/system/kdump.service",
        "usr/lib/systemd/system/vmtoolsd.service",
        "etc/my.cnf.d/server.cnf",
        "var/log/boot.log",
        "var/log/messages",
        "var/log/secure",
    ] {
        match fs.open_file(path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(256 * 1024).read_to_end(&mut bytes).unwrap();
                println!(
                    "=== {path} ({}) ===\n{}",
                    bytes.len(),
                    String::from_utf8_lossy(&bytes)
                );
            }
            Err(error) => println!("=== {path}: ERROR {error} ==="),
        }
    }
}
