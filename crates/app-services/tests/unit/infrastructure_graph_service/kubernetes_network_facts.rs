use super::is_resource_candidate;

#[test]
fn resource_candidates_are_limited_to_kubernetes_yaml_roots() {
    assert!(is_resource_candidate(
        "/etc/kubernetes/network/service.yaml"
    ));
    assert!(is_resource_candidate("/var/lib/kubelet/config/network.yml"));
    assert!(!is_resource_candidate("/etc/hosts"));
    assert!(!is_resource_candidate("/etc/kubernetes/pki/ca.crt"));
}
