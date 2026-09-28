use super::*;

#[test]
fn rewrites_static_ifcfg_to_dhcp_without_changing_file_size() {
    let source = "DEVICE=ens160\nBOOTPROTO=static\nIPADDR=192.168.77.130\nNETMASK=255.255.255.0\nGATEWAY=192.168.77.2\nDNS1=192.168.77.2\n";
    let rewritten = rewrite_ifcfg_for_dhcp(source)
        .expect("rewrite succeeds")
        .expect("static profile changes");
    assert_eq!(rewritten.len(), source.len());
    assert!(rewritten.contains("DEVICE=ens160"));
    assert!(rewritten.contains("BOOTPROTO=dhcp"));
    assert!(!rewritten.contains("192.168.77.130"));
    assert!(!rewritten.contains("192.168.77.2"));
}

#[test]
fn leaves_dhcp_profile_unchanged() {
    let source = "DEVICE=ens160\nBOOTPROTO=dhcp\n";
    assert!(rewrite_ifcfg_for_dhcp(source)
        .expect("rewrite succeeds")
        .is_none());
}
