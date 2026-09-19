use sha2::{Digest, Sha256};
use thiserror::Error;

const LEAF_DOMAIN: &[u8] = b"meow-ledger-merkle-v1/leaf\0";
const NODE_DOMAIN: &[u8] = b"meow-ledger-merkle-v1/node\0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerkleProofStep {
    pub sibling_hash: String,
    pub sibling_on_left: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerkleProof {
    pub leaf_hash: String,
    pub leaf_index: u64,
    pub leaf_count: u64,
    pub steps: Vec<MerkleProofStep>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MerkleError {
    #[error("merkle tree requires at least one leaf")]
    EmptyTree,
    #[error("merkle leaf index {index} is outside {leaf_count} leaves")]
    IndexOutOfBounds { index: usize, leaf_count: usize },
    #[error("invalid SHA-256 hash: {0}")]
    InvalidHash(String),
}

pub fn merkle_root(leaves: &[String]) -> Result<String, MerkleError> {
    let mut level = leaves
        .iter()
        .map(|leaf| hash_leaf(leaf))
        .collect::<Result<Vec<_>, _>>()?;
    if level.is_empty() {
        return Err(MerkleError::EmptyTree);
    }
    while level.len() > 1 {
        level = next_level(&level);
    }
    Ok(hex::encode(level[0]))
}

pub fn merkle_proof(leaves: &[String], index: usize) -> Result<MerkleProof, MerkleError> {
    if leaves.is_empty() {
        return Err(MerkleError::EmptyTree);
    }
    if index >= leaves.len() {
        return Err(MerkleError::IndexOutOfBounds {
            index,
            leaf_count: leaves.len(),
        });
    }
    let mut level = leaves
        .iter()
        .map(|leaf| hash_leaf(leaf))
        .collect::<Result<Vec<_>, _>>()?;
    let mut position = index;
    let mut steps = Vec::new();
    while level.len() > 1 {
        let sibling = if position.is_multiple_of(2) {
            position + 1
        } else {
            position - 1
        };
        let sibling = sibling.min(level.len() - 1);
        steps.push(MerkleProofStep {
            sibling_hash: hex::encode(level[sibling]),
            sibling_on_left: !position.is_multiple_of(2),
        });
        level = next_level(&level);
        position /= 2;
    }
    Ok(MerkleProof {
        leaf_hash: leaves[index].clone(),
        leaf_index: index as u64,
        leaf_count: leaves.len() as u64,
        steps,
    })
}

pub fn verify_merkle_proof(proof: &MerkleProof, expected_root: &str) -> bool {
    let Ok(expected_root) = decode_hash(expected_root) else {
        return false;
    };
    if proof.leaf_count == 0 || proof.leaf_index >= proof.leaf_count {
        return false;
    }
    let Ok(mut current) = hash_leaf(&proof.leaf_hash) else {
        return false;
    };
    let Ok(mut position) = usize::try_from(proof.leaf_index) else {
        return false;
    };
    let Ok(mut width) = usize::try_from(proof.leaf_count) else {
        return false;
    };
    for step in &proof.steps {
        if width <= 1 || step.sibling_on_left == position.is_multiple_of(2) {
            return false;
        }
        let Ok(sibling) = decode_hash(&step.sibling_hash) else {
            return false;
        };
        if position == width - 1 && !width.is_multiple_of(2) && sibling != current {
            return false;
        }
        current = if step.sibling_on_left {
            hash_node(&sibling, &current)
        } else {
            hash_node(&current, &sibling)
        };
        position /= 2;
        width = width.div_ceil(2);
    }
    position == 0 && width == 1 && current == expected_root
}

fn hash_leaf(value: &str) -> Result<[u8; 32], MerkleError> {
    let bytes = decode_hash(value)?;
    let mut hasher = Sha256::new();
    hasher.update(LEAF_DOMAIN);
    hasher.update(bytes);
    Ok(hasher.finalize().into())
}

fn hash_node(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(NODE_DOMAIN);
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().into()
}

fn next_level(level: &[[u8; 32]]) -> Vec<[u8; 32]> {
    level
        .chunks(2)
        .map(|pair| hash_node(&pair[0], pair.get(1).unwrap_or(&pair[0])))
        .collect()
}

fn decode_hash(value: &str) -> Result<[u8; 32], MerkleError> {
    if value.len() != 64 || value.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err(MerkleError::InvalidHash(value.to_string()));
    }
    let bytes = hex::decode(value).map_err(|_| MerkleError::InvalidHash(value.to_string()))?;
    bytes
        .try_into()
        .map_err(|_| MerkleError::InvalidHash(value.to_string()))
}
