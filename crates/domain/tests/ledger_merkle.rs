use domain::{merkle_proof, merkle_root, verify_merkle_proof, MerkleError};

fn leaves(count: usize) -> Vec<String> {
    (0..count).map(|index| format!("{index:064x}")).collect()
}

#[test]
fn merkle_proof_verifies_for_odd_leaf_count() {
    let leaves = leaves(5);
    let root = merkle_root(&leaves).unwrap();
    let proof = merkle_proof(&leaves, 4).unwrap();
    assert!(verify_merkle_proof(&proof, &root));
}

#[test]
fn tampered_merkle_proof_is_rejected() {
    let leaves = leaves(3);
    let root = merkle_root(&leaves).unwrap();
    let mut proof = merkle_proof(&leaves, 1).unwrap();
    proof.leaf_hash = leaves[2].clone();
    assert!(!verify_merkle_proof(&proof, &root));
}

#[test]
fn malformed_merkle_path_shape_is_rejected() {
    let leaves = leaves(5);
    let root = merkle_root(&leaves).unwrap();
    let mut proof = merkle_proof(&leaves, 4).unwrap();
    proof.steps.pop();
    assert!(!verify_merkle_proof(&proof, &root));

    let mut proof = merkle_proof(&leaves, 1).unwrap();
    proof.steps[0].sibling_on_left = false;
    assert!(!verify_merkle_proof(&proof, &root));
}

#[test]
fn merkle_inputs_fail_closed() {
    assert_eq!(merkle_root(&[]), Err(MerkleError::EmptyTree));
    let leaves = leaves(1);
    assert!(matches!(
        merkle_proof(&leaves, 1),
        Err(MerkleError::IndexOutOfBounds { .. })
    ));
    assert!(merkle_root(&["ABC".to_string()]).is_err());
}
