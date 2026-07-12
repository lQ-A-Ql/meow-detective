//! Cryptographic signing support for exchange artifacts using Ed25519.
//!
//! Provides a `SigningEngine` that generates Ed25519 keypairs, signs arbitrary
//! data, verifies signatures, and produces cryptographically-signed case export
//! bundles.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Result of signing a case export.
///
/// Contains the Ed25519 signature, the public key needed for verification,
/// a timestamp, a content hash, and identifying metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedExport {
    /// Ed25519 signature over the export payload (64 bytes).
    pub signature: Vec<u8>,
    /// Public key used for verification (32 bytes).
    pub public_key: Vec<u8>,
    /// ISO 8601 timestamp when the signature was produced.
    pub timestamp: String,
    /// SHA-256 hash of the case content that was signed.
    pub case_hash: Vec<u8>,
    /// Identifier of the case that was signed.
    pub case_id: String,
    /// Algorithm identifier — always "Ed25519".
    pub algorithm: String,
}

/// Ed25519-based signing engine for export artifacts.
///
/// All methods are stateless; the engine is a namespace for key generation,
/// signing, and verification operations.
pub struct SigningEngine;

impl SigningEngine {
    /// Generate a new Ed25519 keypair using OS randomness.
    ///
    /// Returns `(private_key, public_key)` each as a 32-byte `Vec<u8>`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let (sk, pk) = SigningEngine::generate_keypair();
    /// assert_eq!(sk.len(), 32);
    /// assert_eq!(pk.len(), 32);
    /// ```
    pub fn generate_keypair() -> (Vec<u8>, Vec<u8>) {
        let secret_bytes: [u8; 32] = rand::random();
        let signing_key = SigningKey::from_bytes(&secret_bytes);
        let verifying_key = signing_key.verifying_key();
        (
            signing_key.to_bytes().to_vec(),
            verifying_key.to_bytes().to_vec(),
        )
    }

    /// Sign arbitrary data with the given 32-byte Ed25519 private key.
    ///
    /// Returns the signature as a 64-byte `Vec<u8>`, or an error string if the
    /// private key is not exactly 32 bytes.
    pub fn sign_data(private_key: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
        let key_bytes: [u8; 32] = private_key
            .try_into()
            .map_err(|_| "Private key must be 32 bytes".to_string())?;
        let signing_key = SigningKey::from_bytes(&key_bytes);
        let signature: Signature = signing_key.sign(data);
        Ok(signature.to_bytes().to_vec())
    }

    /// Verify an Ed25519 signature against a public key and data.
    ///
    /// Returns `true` if the signature is valid, `false` if the keys or
    /// signature are malformed, or if the signature does not match.
    pub fn verify_signature(public_key: &[u8], data: &[u8], signature: &[u8]) -> bool {
        let pk: [u8; 32] = match public_key.try_into() {
            Ok(v) => v,
            Err(_) => return false,
        };
        let sig: [u8; 64] = match signature.try_into() {
            Ok(v) => v,
            Err(_) => return false,
        };
        let verifying_key = match VerifyingKey::from_bytes(&pk) {
            Ok(vk) => vk,
            Err(_) => return false,
        };
        let sig = Signature::from_bytes(&sig);
        verifying_key.verify(data, &sig).is_ok()
    }

    /// Sign a case export payload and return a [`SignedExport`].
    ///
    /// The signing payload is computed as:
    ///
    /// ```text
    /// SHA-256(case_id || timestamp || case_content_hash)
    /// ```
    ///
    /// where `case_content_hash = SHA-256(export_data)`. The resulting
    /// `SignedExport` includes the signature, the derived public key, and all
    /// metadata needed for independent verification.
    pub fn sign_case_export(
        case_id: &str,
        export_data: &[u8],
        private_key: &[u8],
    ) -> Result<SignedExport, String> {
        let timestamp = chrono::Utc::now().to_rfc3339();

        // Derive public key from the private key.
        let key_bytes: [u8; 32] = private_key
            .try_into()
            .map_err(|_| "Private key must be 32 bytes".to_string())?;
        let signing_key = SigningKey::from_bytes(&key_bytes);
        let verifying_key = signing_key.verifying_key();
        let public_key = verifying_key.to_bytes().to_vec();

        // Hash the export content.
        let mut hasher = Sha256::new();
        hasher.update(export_data);
        let case_hash = hasher.finalize().to_vec();

        // Build the signing payload: case_id || timestamp || case_hash
        let mut payload = Vec::new();
        payload.extend_from_slice(case_id.as_bytes());
        payload.extend_from_slice(timestamp.as_bytes());
        payload.extend_from_slice(&case_hash);

        // Sign the payload.
        let signature: Signature = signing_key.sign(&payload);

        Ok(SignedExport {
            signature: signature.to_bytes().to_vec(),
            public_key,
            timestamp,
            case_hash,
            case_id: case_id.to_string(),
            algorithm: "Ed25519".to_string(),
        })
    }
}

#[cfg(test)]
#[path = "../tests/unit/signing.rs"]
mod tests;
