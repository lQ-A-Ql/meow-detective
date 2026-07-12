//! Hash calculation services.

use infrastructure::hashing;
use std::io::{self, Read};
use std::path::Path;

pub struct HashService;

impl HashService {
    pub fn sha256_reader(reader: &mut dyn Read) -> io::Result<String> {
        hashing::sha256_reader(reader)
    }

    pub fn sha256_file(path: &Path) -> io::Result<String> {
        hashing::sha256_file(path)
    }

    pub fn sha256_bytes(data: &[u8]) -> String {
        hashing::sha256_bytes(data)
    }

    pub fn verify_sha256(data: &[u8], expected_hash: &str) -> bool {
        hashing::verify_sha256(data, expected_hash)
    }
}

#[cfg(test)]
#[path = "../tests/unit/hash_service.rs"]
mod tests;
