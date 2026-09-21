use transport::dto::{TopologyEdgeKindDto, TopologyMemberRoleDto, TopologyScopeKindDto};

#[test]
fn topology_contract_uses_stable_snake_case_values() {
    assert_eq!(
        serde_json::to_string(&TopologyScopeKindDto::Kubernetes).unwrap(),
        "\"kubernetes\""
    );
    assert_eq!(
        serde_json::to_string(&TopologyMemberRoleDto::StorageNode).unwrap(),
        "\"storage_node\""
    );
    assert_eq!(
        serde_json::to_string(&TopologyEdgeKindDto::ProvidesStorage).unwrap(),
        "\"provides_storage\""
    );
}
