//! CHAP (Challenge-Handshake Authentication Protocol) implementation
//!
//! RFC 3720 Section 8.2 - CHAP Algorithm

use crate::error::{IscsiError, ScsiResult};
use rand::Rng;

/// CHAP algorithm identifier (RFC 1994)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChapAlgorithm {
    /// MD5 algorithm (algorithm identifier 5)
    Md5 = 5,
}

impl ChapAlgorithm {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "5" => Some(ChapAlgorithm::Md5),
            _ => None,
        }
    }
}

/// CHAP credentials for authentication
#[derive(Debug, Clone)]
pub struct ChapCredentials {
    /// Username for CHAP authentication
    pub username: String,
    /// Secret/password for CHAP authentication
    pub secret: String,
}

impl ChapCredentials {
    pub fn new(username: impl Into<String>, secret: impl Into<String>) -> Self {
        Self {
            username: username.into(),
            secret: secret.into(),
        }
    }
}

/// Authentication configuration
#[derive(Debug, Clone)]
pub enum AuthConfig {
    /// No authentication required
    None,
    /// CHAP authentication (one-way: initiator authenticates to target)
    Chap {
        /// Target credentials (for validating initiator)
        credentials: ChapCredentials,
    },
    /// Mutual CHAP (two-way: both initiator and target authenticate)
    MutualChap {
        /// Target credentials (for validating initiator)
        target_credentials: ChapCredentials,
        /// Initiator credentials (for target to prove identity)
        initiator_credentials: ChapCredentials,
    },
}

impl Default for AuthConfig {
    fn default() -> Self {
        AuthConfig::None
    }
}

impl AuthConfig {
    /// Check if authentication is required
    pub fn requires_auth(&self) -> bool {
        !matches!(self, AuthConfig::None)
    }

    /// Get the authentication method string
    pub fn auth_method(&self) -> &str {
        match self {
            AuthConfig::None => "None",
            AuthConfig::Chap { .. } | AuthConfig::MutualChap { .. } => "CHAP",
        }
    }

    /// Check if mutual CHAP is required
    pub fn is_mutual(&self) -> bool {
        matches!(self, AuthConfig::MutualChap { .. })
    }
}

/// CHAP authentication state
#[derive(Debug, Clone)]
pub struct ChapAuthState {
    /// CHAP identifier (random byte)
    pub identifier: u8,
    /// CHAP challenge (random bytes)
    pub challenge: Vec<u8>,
    /// Whether this is for mutual CHAP (target authenticating to initiator)
    pub is_target_auth: bool,
}

impl ChapAuthState {
    /// Generate a new CHAP challenge
    pub fn new(is_target_auth: bool) -> Self {
        let mut rng = rand::thread_rng();

        // Generate random identifier
        let identifier = rng.gen::<u8>();

        // Generate random challenge (16-256 bytes, we'll use 16 for simplicity)
        let mut challenge = vec![0u8; 16];
        rng.fill(&mut challenge[..]);

        Self {
            identifier,
            challenge,
            is_target_auth,
        }
    }

    /// Calculate the expected CHAP response
    /// Response = MD5(identifier + secret + challenge)
    pub fn calculate_response(&self, secret: &str) -> Vec<u8> {
        let mut data = Vec::new();
        data.push(self.identifier);
        data.extend_from_slice(secret.as_bytes());
        data.extend_from_slice(&self.challenge);

        md5::compute(&data).0.to_vec()
    }

    /// Validate a CHAP response
    pub fn validate_response(&self, response: &[u8], secret: &str) -> bool {
        let expected = self.calculate_response(secret);

        // Constant-time comparison to prevent timing attacks
        if response.len() != expected.len() {
            return false;
        }

        let mut diff = 0u8;
        for (a, b) in response.iter().zip(expected.iter()) {
            diff |= a ^ b;
        }

        diff == 0
    }

    /// Convert challenge to hex string for text parameter
    /// RFC 3720 requires the "0x" prefix for CHAP_C
    pub fn challenge_hex(&self) -> String {
        format!("0x{}", hex::encode(&self.challenge))
    }

    /// Convert identifier to string
    pub fn identifier_str(&self) -> String {
        self.identifier.to_string()
    }
}

/// Parse CHAP response from hex string
pub fn parse_chap_response(hex_str: &str) -> ScsiResult<Vec<u8>> {
    // Strip "0x" prefix if present
    let cleaned = hex_str.strip_prefix("0x").unwrap_or(hex_str);
    hex::decode(cleaned).map_err(|e| {
        IscsiError::Auth(format!("Invalid CHAP response hex: {}", e))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chap_response_validation() {
        let state = ChapAuthState::new(false);
        let secret = "mysecret";

        // Calculate response
        let response = state.calculate_response(secret);

        // Validate correct response
        assert!(state.validate_response(&response, secret));

        // Validate incorrect response
        let mut bad_response = response.clone();
        bad_response[0] ^= 1;
        assert!(!state.validate_response(&bad_response, secret));

        // Validate incorrect secret
        assert!(!state.validate_response(&response, "wrongsecret"));
    }

    #[test]
    fn test_chap_challenge_generation() {
        let state1 = ChapAuthState::new(false);
        let state2 = ChapAuthState::new(false);

        // Challenges should be different
        assert_ne!(state1.identifier, state2.identifier);
        assert_ne!(state1.challenge, state2.challenge);
    }

    #[test]
    fn test_auth_config() {
        let none = AuthConfig::None;
        assert!(!none.requires_auth());
        assert_eq!(none.auth_method(), "None");

        let chap = AuthConfig::Chap {
            credentials: ChapCredentials::new("user", "secret"),
        };
        assert!(chap.requires_auth());
        assert_eq!(chap.auth_method(), "CHAP");
        assert!(!chap.is_mutual());

        let mutual = AuthConfig::MutualChap {
            target_credentials: ChapCredentials::new("target", "secret1"),
            initiator_credentials: ChapCredentials::new("initiator", "secret2"),
        };
        assert!(mutual.requires_auth());
        assert_eq!(mutual.auth_method(), "CHAP");
        assert!(mutual.is_mutual());
    }
}
