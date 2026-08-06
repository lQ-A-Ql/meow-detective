//! Cryptographic hashing (SHA-256, MD5).
//!
//! Provides SHA-256 hashing for evidence integrity verification.
//! Used to compute and verify file hashes during forensic analysis.

use sha2::{Digest, Sha256};
use std::io::{self, Read};

mod pipeline;

pub use pipeline::{sha256_file_pipelined_with_cancel, sha256_pipeline_worker_threads};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sha256Acceleration {
    ShaNi,
    Portable,
}

impl Sha256Acceleration {
    pub fn label(self) -> &'static str {
        match self {
            Self::ShaNi => "sha-ni",
            Self::Portable => "portable",
        }
    }
}

/// Reports the backend selected by `sha2` on this process' CPU.
///
/// The x86 SHA-NI backend is selected by `sha2` through runtime feature
/// detection. No compile-time CPU target is required, so the same binary
/// remains safe to run on machines without SHA extensions.
pub fn sha256_acceleration() -> Sha256Acceleration {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        if std::is_x86_feature_detected!("sha")
            && std::is_x86_feature_detected!("sse2")
            && std::is_x86_feature_detected!("ssse3")
            && std::is_x86_feature_detected!("sse4.1")
        {
            return Sha256Acceleration::ShaNi;
        }
    }
    Sha256Acceleration::Portable
}

/// Compute SHA-256 hash of data from a Reader.
///
/// Reads the entire Reader content and returns a hex-encoded SHA-256 digest.
/// Suitable for file integrity verification and evidence chain validation.
///
/// # Example
/// ```no_run
/// use std::io::Cursor;
/// use infrastructure::hashing::sha256_reader;
///
/// let data = b"evidence data";
/// let mut cursor = Cursor::new(data);
/// let hash = sha256_reader(&mut cursor).unwrap();
/// assert_eq!(hash.len(), 64); // SHA-256 hex string is 64 chars
/// ```
pub fn sha256_reader(reader: &mut dyn Read) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Hashes a stream while allowing a caller to cancel between reads.
pub fn sha256_reader_with_cancel(
    reader: &mut dyn Read,
    cancelled: impl Fn() -> bool,
    mut on_progress: impl FnMut(u64),
) -> io::Result<Option<String>> {
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    let mut processed = 0u64;
    loop {
        if cancelled() {
            return Ok(None);
        }
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
        processed = processed.saturating_add(bytes_read as u64);
        on_progress(processed);
    }
    Ok(Some(hex::encode(hasher.finalize())))
}

/// Compute SHA-256 hash of a byte slice.
///
/// # Example
/// ```
/// use infrastructure::hashing::sha256_bytes;
///
/// let hash = sha256_bytes(b"hello world");
/// assert_eq!(hash, "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9");
/// ```
pub fn sha256_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Verify that data matches an expected SHA-256 hash.
///
/// # Example
/// ```
/// use infrastructure::hashing::{sha256_bytes, verify_sha256};
///
/// let data = b"evidence";
/// let hash = sha256_bytes(data);
/// assert!(verify_sha256(data, &hash));
/// assert!(!verify_sha256(b"tampered", &hash));
/// ```
pub fn verify_sha256(data: &[u8], expected_hash: &str) -> bool {
    sha256_bytes(data) == expected_hash
}

/// Compute SHA-256 hash of a file at the given path.
///
/// # Example
/// ```no_run
/// use infrastructure::hashing::sha256_file;
///
/// let hash = sha256_file(std::path::Path::new("evidence.img")).unwrap();
/// println!("SHA-256: {}", hash);
/// ```
pub fn sha256_file(path: &std::path::Path) -> io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    sha256_reader(&mut file)
}

#[cfg(test)]
#[path = "../../tests/unit/hashing.rs"]
mod tests;
