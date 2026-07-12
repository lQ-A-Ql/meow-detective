use super::*;

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
