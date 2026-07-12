//! Chain-of-custody with Merkle tree verification.
//!
//! Provides a tamper-evident audit log where each entry is linked to its
//! predecessor via SHA-256, and the entire chain can be summarised as a Merkle
//! tree for efficient proof-of-inclusion.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// Chain Entry
// ---------------------------------------------------------------------------

/// A single custody-log entry forming one link in the chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainEntry {
    /// Unique identifier for this entry (UUID v4).
    pub id: String,
    /// The case this entry belongs to.
    pub case_id: String,
    /// The action performed (e.g. "import", "export", "tag", "delete").
    pub action: String,
    /// The identity/username of the actor who performed the action.
    pub actor: String,
    /// ISO 8601 timestamp of the action.
    pub timestamp: String,
    /// SHA-256 hash of the previous entry in the chain.
    /// Empty for the genesis (first) entry.
    pub prev_entry_hash: String,
    /// SHA-256 hash of the action data payload.
    pub data_hash: String,
}

// ---------------------------------------------------------------------------
// Merkle Tree
// ---------------------------------------------------------------------------

/// A binary Merkle tree built from custody log entries.
///
/// Leaves are `SHA-256(serialized_entry)`. The tree is built bottom-up; when
/// the leaf count is not a power of two the last leaf is duplicated at each
/// level as needed to produce a balanced binary tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleTree {
    /// Root hash of the tree (32 bytes).
    pub root_hash: Vec<u8>,
    /// Number of leaves in the tree.
    pub leaf_count: usize,
}

/// A Merkle inclusion proof for a single leaf.
///
/// Contains the sibling hashes along the path from the leaf to the root.
/// The verifier re-computes the root by hashing upward and compares the result
/// with a trusted root hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerkleProof {
    /// Index of the leaf being proven (0-based).
    pub leaf_index: usize,
    /// Ordered list of sibling hashes from leaf to root.
    /// At each level, the sibling hash is the one that pairs with the current
    /// node to compute the parent.
    pub proof: Vec<Vec<u8>>,
}

// ---------------------------------------------------------------------------
// ChainOfCustody
// ---------------------------------------------------------------------------

/// Manages a sequentially-hashed chain of custody log with Merkle tree support.
pub struct ChainOfCustody;

impl ChainOfCustody {
    /// Create the genesis (first) entry in a custody chain.
    ///
    /// The genesis entry has an empty `prev_entry_hash`. Subsequent entries
    /// are linked by hashing the previous entry.
    pub fn append_entry(case_id: &str, action: &str, actor: &str, data: &[u8]) -> ChainEntry {
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = chrono::Utc::now().to_rfc3339();
        let data_hash = hex::encode(Sha256::digest(data));

        ChainEntry {
            id,
            case_id: case_id.to_string(),
            action: action.to_string(),
            actor: actor.to_string(),
            timestamp,
            prev_entry_hash: String::new(),
            data_hash,
        }
    }

    /// Append an entry that is cryptographically linked to a previous entry.
    ///
    /// The new entry's `prev_entry_hash` is set to `SHA-256(serialized_prev)`.
    pub fn append_entry_after(
        prev: &ChainEntry,
        case_id: &str,
        action: &str,
        actor: &str,
        data: &[u8],
    ) -> ChainEntry {
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = chrono::Utc::now().to_rfc3339();
        let data_hash = hex::encode(Sha256::digest(data));
        let prev_entry_hash = Self::hash_entry(prev);

        ChainEntry {
            id,
            case_id: case_id.to_string(),
            action: action.to_string(),
            actor: actor.to_string(),
            timestamp,
            prev_entry_hash,
            data_hash,
        }
    }

    /// Compute a deterministic SHA-256 hash for a single [`ChainEntry`].
    ///
    /// Hash = `SHA-256(id || case_id || action || actor || timestamp || prev_entry_hash || data_hash)`
    pub fn hash_entry(entry: &ChainEntry) -> String {
        let mut hasher = Sha256::new();
        hasher.update(entry.id.as_bytes());
        hasher.update(entry.case_id.as_bytes());
        hasher.update(entry.action.as_bytes());
        hasher.update(entry.actor.as_bytes());
        hasher.update(entry.timestamp.as_bytes());
        hasher.update(entry.prev_entry_hash.as_bytes());
        hasher.update(entry.data_hash.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Build a binary Merkle tree from a slice of custody entries.
    ///
    /// Leaf hashes are `SHA-256(serialized_entry)` (the same as
    /// [`hash_entry`]). Nodes at each level are `SHA-256(left || right)`.
    pub fn build_merkle_tree(entries: &[ChainEntry]) -> MerkleTree {
        if entries.is_empty() {
            return MerkleTree {
                root_hash: Sha256::digest(b"").to_vec(),
                leaf_count: 0,
            };
        }

        let leaves: Vec<Vec<u8>> = entries
            .iter()
            .map(|e| {
                let h = Self::hash_entry(e);
                hex::decode(&h).expect("hex decode of SHA-256 hash")
            })
            .collect();

        // Pad to power of two by duplicating the last leaf.
        let leaf_count = leaves.len();
        let mut padded = leaves.clone();
        let target = leaf_count.next_power_of_two();
        if target > leaf_count {
            let last = leaves.last().cloned().unwrap();
            padded.resize(target, last);
        }

        // Build tree bottom-up.
        let mut current_level = padded;
        while current_level.len() > 1 {
            let mut next_level = Vec::with_capacity(current_level.len() / 2);
            for chunk in current_level.chunks(2) {
                let mut hasher = Sha256::new();
                hasher.update(&chunk[0]);
                hasher.update(&chunk[1]);
                next_level.push(hasher.finalize().to_vec());
            }
            current_level = next_level;
        }

        MerkleTree {
            root_hash: current_level[0].clone(),
            leaf_count,
        }
    }

    /// Generate a Merkle inclusion proof for the leaf at `leaf_index`.
    ///
    /// Returns `None` if the index is out of bounds.
    pub fn generate_merkle_proof(entries: &[ChainEntry], leaf_index: usize) -> Option<MerkleProof> {
        if leaf_index >= entries.len() {
            return None;
        }

        let leaves: Vec<Vec<u8>> = entries
            .iter()
            .map(|e| {
                let h = Self::hash_entry(e);
                hex::decode(&h).expect("hex decode of SHA-256 hash")
            })
            .collect();

        let leaf_count = leaves.len();
        let target = leaf_count.next_power_of_two();
        let mut padded = leaves.clone();
        if target > leaf_count {
            let last = leaves.last().cloned().unwrap();
            padded.resize(target, last);
        }

        let mut proof = Vec::new();
        let mut index = leaf_index;
        let mut level = padded;

        while level.len() > 1 {
            let sibling_idx = if index.is_multiple_of(2) {
                index + 1
            } else {
                index - 1
            };
            proof.push(level[sibling_idx].clone());

            // Move to parent level.
            let mut next_level = Vec::with_capacity(level.len() / 2);
            for chunk in level.chunks(2) {
                let mut hasher = Sha256::new();
                hasher.update(&chunk[0]);
                hasher.update(&chunk[1]);
                next_level.push(hasher.finalize().to_vec());
            }
            level = next_level;
            index /= 2;
        }

        Some(MerkleProof { leaf_index, proof })
    }

    /// Verify a Merkle inclusion proof.
    ///
    /// Given a leaf hash, its index, the proof (sibling hashes from leaf to
    /// root), and the trusted root hash, re-compute the expected root and
    /// compare.
    ///
    /// Returns `true` if the proof is valid.
    pub fn verify_merkle_proof(leaf_hash: &[u8], proof: &MerkleProof, root_hash: &[u8]) -> bool {
        let mut current_hash = leaf_hash.to_vec();
        let mut index = proof.leaf_index;

        for sibling in &proof.proof {
            let mut hasher = Sha256::new();
            if index.is_multiple_of(2) {
                // current is left child, sibling is right
                hasher.update(&current_hash);
                hasher.update(sibling);
            } else {
                // current is right child, sibling is left
                hasher.update(sibling);
                hasher.update(&current_hash);
            }
            current_hash = hasher.finalize().to_vec();
            index /= 2;
        }

        current_hash == root_hash
    }

    /// Verify the sequential hash chain.
    ///
    /// For each entry (starting from the second), recomputes the expected
    /// `prev_entry_hash` and compared with the stored value. Returns `true`
    /// if every link is intact.
    pub fn verify_chain(entries: &[ChainEntry]) -> bool {
        if entries.len() <= 1 {
            return true; // Empty or single-entry chains are trivially valid.
        }

        for i in 1..entries.len() {
            let expected_prev = Self::hash_entry(&entries[i - 1]);
            if entries[i].prev_entry_hash != expected_prev {
                return false;
            }
        }
        true
    }

    /// Export the custody log for a given case as a JSON string.
    pub fn export_custody_log(entries: &[ChainEntry]) -> String {
        serde_json::to_string_pretty(entries).unwrap_or_else(|_| "[]".to_string())
    }
}

// ---------------------------------------------------------------------------
// Helpers — hex encoding (no external crate needed)
// ---------------------------------------------------------------------------

mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes.as_ref().iter().map(|b| format!("{b:02x}")).collect()
    }

    pub fn decode(s: &str) -> Result<Vec<u8>, String> {
        if !s.len().is_multiple_of(2) {
            return Err("Hex string must have even length".to_string());
        }
        (0..s.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&s[i..i + 2], 16)
                    .map_err(|e| format!("Invalid hex at position {i}: {e}"))
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "../tests/unit/custody.rs"]
mod tests;
