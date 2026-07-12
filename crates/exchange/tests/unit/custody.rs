use super::*;

// ------------------------------------------------------------------
// Chain-of-custody tests
// ------------------------------------------------------------------

#[test]
fn test_chain_append_creates_sequential_hash() {
    let e1 = ChainOfCustody::append_entry("case-A", "import", "analyst1", b"evidence-1");
    let e2 = ChainOfCustody::append_entry_after(&e1, "case-A", "tag", "analyst1", b"evidence-2");

    // e2.prev_entry_hash must equal hash(e1).
    assert_eq!(e2.prev_entry_hash, ChainOfCustody::hash_entry(&e1));

    // e1 (genesis) must have an empty prev_entry_hash.
    assert!(e1.prev_entry_hash.is_empty());
}

#[test]
fn test_chain_integrity_verification() {
    let e1 = ChainOfCustody::append_entry("case-B", "create", "admin", b"initial");
    let e2 = ChainOfCustody::append_entry_after(&e1, "case-B", "update", "admin", b"changed");
    let e3 = ChainOfCustody::append_entry_after(&e2, "case-B", "export", "admin", b"exported");

    let chain = vec![e1, e2, e3];
    assert!(ChainOfCustody::verify_chain(&chain));
}

#[test]
fn test_tampered_chain_detected() {
    let e1 = ChainOfCustody::append_entry("case-C", "create", "admin", b"initial");
    let e2 = ChainOfCustody::append_entry_after(&e1, "case-C", "update", "admin", b"changed");
    let e3 = ChainOfCustody::append_entry_after(&e2, "case-C", "export", "admin", b"exported");

    let mut chain = vec![e1, e2.clone(), e3];
    // Tamper with e2's action.
    chain[1].action = "tampered".to_string();

    assert!(!ChainOfCustody::verify_chain(&chain));
}

#[test]
fn test_tampered_prev_hash_detected() {
    let e1 = ChainOfCustody::append_entry("case-D", "create", "admin", b"initial");
    let e2 = ChainOfCustody::append_entry_after(&e1, "case-D", "update", "admin", b"changed");

    let mut chain = vec![e1, e2];
    // Directly forge prev_entry_hash on the second entry.
    chain[1].prev_entry_hash =
        "0000000000000000000000000000000000000000000000000000000000000000".to_string();

    assert!(!ChainOfCustody::verify_chain(&chain));
}

// ------------------------------------------------------------------
// Merkle tree tests
// ------------------------------------------------------------------

#[test]
fn test_merkle_proof_verification() {
    let e1 = ChainOfCustody::append_entry("case-E", "create", "admin", b"data-1");
    let e2 = ChainOfCustody::append_entry_after(&e1, "case-E", "import", "admin", b"data-2");
    let e3 = ChainOfCustody::append_entry_after(&e2, "case-E", "tag", "admin", b"data-3");
    let e4 = ChainOfCustody::append_entry_after(&e3, "case-E", "export", "admin", b"data-4");

    let entries = vec![e1, e2, e3, e4];
    let tree = ChainOfCustody::build_merkle_tree(&entries);

    // Verify proof for each leaf.
    for i in 0..entries.len() {
        let leaf_hash_hex = ChainOfCustody::hash_entry(&entries[i]);
        let leaf_hash = hex::decode(&leaf_hash_hex).expect("valid hex");
        let proof = ChainOfCustody::generate_merkle_proof(&entries, i).expect("proof should exist");
        assert!(
            ChainOfCustody::verify_merkle_proof(&leaf_hash, &proof, &tree.root_hash),
            "Merkle proof verification failed for leaf {i}"
        );
    }
}

#[test]
fn test_merkle_proof_out_of_bounds() {
    let e1 = ChainOfCustody::append_entry("case-F", "create", "admin", b"data");
    let entries = vec![e1];
    assert!(ChainOfCustody::generate_merkle_proof(&entries, 1).is_none());
}

#[test]
fn test_merkle_proof_rejects_wrong_leaf() {
    let e1 = ChainOfCustody::append_entry("case-G", "create", "admin", b"data-1");
    let e2 = ChainOfCustody::append_entry_after(&e1, "case-G", "update", "admin", b"data-2");

    let entries = vec![e1, e2];
    let tree = ChainOfCustody::build_merkle_tree(&entries);

    // Generate proof for leaf 0.
    let proof = ChainOfCustody::generate_merkle_proof(&entries, 0).unwrap();

    // Try to verify with leaf 1's hash — should fail.
    let wrong_leaf_hex = ChainOfCustody::hash_entry(&entries[1]);
    let wrong_leaf = hex::decode(&wrong_leaf_hex).unwrap();
    assert!(!ChainOfCustody::verify_merkle_proof(
        &wrong_leaf,
        &proof,
        &tree.root_hash
    ));
}

#[test]
fn test_merkle_tree_odd_leaf_count() {
    let e1 = ChainOfCustody::append_entry("case-H", "create", "admin", b"data-1");
    let e2 = ChainOfCustody::append_entry_after(&e1, "case-H", "import", "admin", b"data-2");
    let e3 = ChainOfCustody::append_entry_after(&e2, "case-H", "tag", "admin", b"data-3");

    // 3 leaves — should be padded to 4 internally.
    let entries = vec![e1, e2, e3];
    let tree = ChainOfCustody::build_merkle_tree(&entries);
    assert_eq!(tree.leaf_count, 3);

    // All proofs should verify.
    for i in 0..entries.len() {
        let leaf_hash_hex = ChainOfCustody::hash_entry(&entries[i]);
        let leaf_hash = hex::decode(&leaf_hash_hex).unwrap();
        let proof = ChainOfCustody::generate_merkle_proof(&entries, i).unwrap();
        assert!(ChainOfCustody::verify_merkle_proof(
            &leaf_hash,
            &proof,
            &tree.root_hash
        ));
    }
}

#[test]
fn test_empty_merkle_tree() {
    let entries: Vec<ChainEntry> = vec![];
    let tree = ChainOfCustody::build_merkle_tree(&entries);
    assert_eq!(tree.leaf_count, 0);
    assert_eq!(tree.root_hash, Sha256::digest(b"").to_vec());
}

#[test]
fn test_export_custody_log() {
    let e1 = ChainOfCustody::append_entry("case-I", "create", "admin", b"data");
    let json = ChainOfCustody::export_custody_log(&[e1]);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&json).expect("valid JSON");
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0]["case_id"], "case-I");
}

#[test]
fn test_single_entry_chain_is_valid() {
    let e1 = ChainOfCustody::append_entry("case-J", "create", "admin", b"data");
    assert!(ChainOfCustody::verify_chain(&[e1]));
}
