use super::*;

#[test]
fn interface_values_preserve_multiple_tcpip_addresses() {
    let mut adapter = NetworkAdapterInfo::default();
    apply_interface_value(
        &mut adapter,
        "IPAddress",
        RegistryValue::MultiString(vec!["192.0.2.10".to_string(), "2001:db8::10".to_string()]),
    );
    apply_interface_value(
        &mut adapter,
        "SubnetMask",
        RegistryValue::MultiString(vec!["255.255.255.0".to_string()]),
    );
    apply_interface_value(
        &mut adapter,
        "DefaultGateway",
        RegistryValue::MultiString(vec!["192.0.2.1".to_string()]),
    );

    assert_eq!(adapter.ip_addresses, ["192.0.2.10", "2001:db8::10"]);
    assert_eq!(adapter.subnet_masks, ["255.255.255.0"]);
    assert_eq!(adapter.gateways, ["192.0.2.1"]);
}

#[test]
fn dhcp_name_server_accepts_comma_and_space_separators() {
    let mut adapter = NetworkAdapterInfo::default();
    apply_interface_value(
        &mut adapter,
        "DhcpNameServer",
        RegistryValue::String("192.0.2.53, 192.0.2.54 2001:db8::53".to_string()),
    );

    assert_eq!(
        adapter.dns_servers,
        ["192.0.2.53", "192.0.2.54", "2001:db8::53"]
    );
}

#[test]
fn interface_guid_filter_rejects_network_container_keys() {
    assert!(is_interface_guid("{98420441-28EB-43A5-A59F-C9EACCBA714B}"));
    assert!(!is_interface_guid("Descriptions"));
    assert!(!is_interface_guid("{NOT-A-GUID}"));
}
