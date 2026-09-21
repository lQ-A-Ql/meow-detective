use transport::dto::{
    AnalysisObjectKindDto, EnvironmentObjectKindDto, InfrastructureRelationKindDto,
    StorageObjectKindDto, TopologyEdgeKindDto, TopologyMemberRoleDto, TopologyScopeKindDto,
};

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
    assert_eq!(
        serde_json::to_string(&EnvironmentObjectKindDto::VirtualMachine).unwrap(),
        "\"virtual_machine\""
    );
    assert_eq!(
        serde_json::to_string(&StorageObjectKindDto::CephRbd).unwrap(),
        "\"ceph_rbd\""
    );
    assert_eq!(
        serde_json::to_string(&AnalysisObjectKindDto::Artifact).unwrap(),
        "\"artifact\""
    );
    assert_eq!(
        serde_json::to_string(&InfrastructureRelationKindDto::MaterializesAs).unwrap(),
        "\"materializes_as\""
    );
}
