use crate::cluster_service::parse_cni_config;

#[test]
fn cni_parser_preserves_network_plugin_and_ipam_evidence() {
    let summary = parse_cni_config(br#"{
      "cniVersion": "1.0.0",
      "name": "pod-network",
      "plugins": [
        {"type": "calico", "ipam": {"type": "host-local", "subnet": "10.244.0.0/16", "routes": [{"dst": "0.0.0.0/0"}]}},
        {"type": "portmap"}
      ]
    }"#).unwrap();
    assert_eq!(summary.name.as_deref(), Some("pod-network"));
    assert_eq!(summary.cni_version.as_deref(), Some("1.0.0"));
    assert_eq!(summary.plugin_types, vec!["calico", "portmap"]);
    assert_eq!(summary.ipam_type.as_deref(), Some("host-local"));
    assert_eq!(summary.subnets, vec!["10.244.0.0/16"]);
    assert_eq!(summary.routes, vec!["0.0.0.0/0"]);
}

#[test]
fn cni_parser_rejects_non_object_json() {
    assert!(parse_cni_config(br#"["calico"]"#).is_err());
}
