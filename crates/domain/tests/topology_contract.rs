use domain::{
    AnalysisObjectKind, EnvironmentObjectKind, InfrastructureRelationKind, StorageObjectKind,
    TopologyObjectKind,
};

#[test]
fn storage_environment_and_analysis_domains_are_distinct() {
    let pve = TopologyObjectKind::Environment(EnvironmentObjectKind::Pve);
    let ceph = TopologyObjectKind::Storage(StorageObjectKind::CephCluster);
    let artifact = TopologyObjectKind::Analysis(AnalysisObjectKind::Artifact);

    assert_ne!(pve, ceph);
    assert_ne!(ceph, artifact);
    assert_ne!(pve, artifact);
}

#[test]
fn infrastructure_relation_matrix_accepts_expected_edges() {
    assert!(InfrastructureRelationKind::Hosts.allows(
        TopologyObjectKind::Environment(EnvironmentObjectKind::Pve),
        TopologyObjectKind::Environment(EnvironmentObjectKind::VirtualMachine),
    ));
    assert!(InfrastructureRelationKind::MaterializesAs.allows(
        TopologyObjectKind::Storage(StorageObjectKind::CephRbd),
        TopologyObjectKind::Storage(StorageObjectKind::VirtualDisk),
    ));
    assert!(InfrastructureRelationKind::Runs.allows(
        TopologyObjectKind::Environment(EnvironmentObjectKind::OsInstance),
        TopologyObjectKind::Environment(EnvironmentObjectKind::Kubernetes),
    ));
}

#[test]
fn infrastructure_relation_matrix_rejects_cross_layer_shortcuts() {
    assert!(!InfrastructureRelationKind::ProvidesStorage.allows(
        TopologyObjectKind::Environment(EnvironmentObjectKind::Kubernetes),
        TopologyObjectKind::Storage(StorageObjectKind::CephCluster),
    ));
    assert!(!InfrastructureRelationKind::Runs.allows(
        TopologyObjectKind::Storage(StorageObjectKind::CephRbd),
        TopologyObjectKind::Environment(EnvironmentObjectKind::Kubernetes),
    ));
    assert!(!InfrastructureRelationKind::MaterializesAs.allows(
        TopologyObjectKind::Environment(EnvironmentObjectKind::Pve),
        TopologyObjectKind::Storage(StorageObjectKind::CephRbd),
    ));
}
