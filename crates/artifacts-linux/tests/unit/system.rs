use super::*;
use crate::parse_systemd_unit;

#[test]
fn parse_passwd_accounts() {
    let input = "\
root:x:0:0:root:/root:/bin/bash
daemon:x:1:1:daemon:/usr/sbin:/usr/sbin/nologin
alice:x:1000:1000:Alice:/home/alice:/bin/bash
";
    let accounts = parse_passwd(input).expect("passwd should parse");
    assert_eq!(accounts.len(), 3);
    assert_eq!(accounts[0].username, "root");
    assert_eq!(accounts[0].uid, 0);
    assert_eq!(accounts[2].username, "alice");
    assert_eq!(accounts[2].home, "/home/alice");
    assert_eq!(accounts[2].shell, "/bin/bash");
}

#[test]
fn parse_os_release_fields() {
    let input = "\
NAME=\"CentOS Stream\"
VERSION_ID=\"9\"
ID=centos
PRETTY_NAME=\"CentOS Stream 9\"
";
    let info = parse_os_release(input).expect("os-release should parse");
    assert_eq!(info.pretty_name.as_deref(), Some("CentOS Stream 9"));
    assert_eq!(info.id.as_deref(), Some("centos"));
    assert_eq!(info.version_id.as_deref(), Some("9"));
    assert_eq!(info.fields["NAME"], "CentOS Stream");
}

#[test]
fn parse_systemd_unit_infers_enablement_and_fields() {
    let input = b"[Unit]\nDescription=Demo service\n[Service]\nExecStart=/usr/bin/demo --serve\nUser=demo\nRestart=on-failure\n[Install]\nWantedBy=multi-user.target\n";
    let unit = parse_systemd_unit(
        "etc/systemd/system/multi-user.target.wants/demo.service",
        input,
    )
    .expect("systemd unit should parse");
    assert_eq!(unit.name, "demo.service");
    assert_eq!(unit.description.as_deref(), Some("Demo service"));
    assert_eq!(unit.state, "enabled");
    assert!(unit.enabled);
    assert_eq!(unit.exec_start, vec!["/usr/bin/demo --serve"]);
    assert_eq!(
        unit.enablement_symlink.as_deref(),
        Some("etc/systemd/system/multi-user.target.wants/demo.service")
    );
}

#[test]
fn parse_systemd_unit_detects_mask_and_limits() {
    let unit = parse_systemd_unit("etc/systemd/system/demo.service", b"/dev/null")
        .expect("masked unit should parse");
    assert!(unit.masked);
    assert_eq!(unit.state, "masked");
    let oversized = vec![b'x'; 1024 * 1024 + 1];
    assert!(parse_systemd_unit("demo.service", &oversized).is_err());
}

#[test]
fn parse_systemd_unit_distinguishes_disabled_from_static() {
    let disabled = parse_systemd_unit(
        "usr/lib/systemd/system/demo.service",
        b"[Unit]\nDescription=Demo\n[Install]\nWantedBy=multi-user.target\n",
    )
    .expect("disabled unit should parse");
    assert_eq!(disabled.state, "disabled");
    assert!(!disabled.enabled);
    assert!(!disabled.static_unit);

    let static_unit = parse_systemd_unit(
        "usr/lib/systemd/system/static.service",
        b"[Unit]\nDescription=Static\n[Service]\nExecStart=/bin/true\n",
    )
    .expect("static unit should parse");
    assert_eq!(static_unit.state, "static");
    assert!(static_unit.static_unit);
}
