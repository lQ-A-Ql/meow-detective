use super::*;

#[test]
fn accepts_any_predictable_linux_network_pci_slot() {
    assert_eq!(predictable_network_pci_slot("ens10".to_string()), Some(10));
    assert_eq!(
        predictable_network_pci_slot("ens160".to_string()),
        Some(160)
    );
    assert_eq!(predictable_network_pci_slot("ens0".to_string()), None);
    assert_eq!(predictable_network_pci_slot("eth0".to_string()), None);
}

#[test]
fn extracts_network_names_from_config_content() {
    let names = interface_names_from_config(b"match: name: ens42\nset-name: ens42\nName=ens42\n");
    assert_eq!(names, vec!["ens42", "ens42", "ens42"]);
}
