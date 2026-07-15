use super::*;

#[test]
fn derived_source_id_is_deterministic_and_path_safe() {
    let id = derived_data_source_id("cluster-123", "16ecc87af5c9").unwrap();

    assert_eq!(id.0, "rbd-cluster-123-16ecc87af5c9");
}

#[test]
fn derived_source_id_rejects_path_and_scope_separators() {
    for invalid in ["../image", "image/name", "image:name", "image\0name"] {
        let error = derived_data_source_id("cluster-123", invalid).unwrap_err();
        assert!(matches!(
            error,
            DerivedSourceError::InvalidIdentity { field: "image ID" }
        ));
    }
}

#[test]
fn derived_source_id_rejects_unbounded_components() {
    let error = derived_data_source_id("cluster-123", &"a".repeat(129)).unwrap_err();

    assert!(matches!(
        error,
        DerivedSourceError::InvalidIdentity { field: "image ID" }
    ));
}
