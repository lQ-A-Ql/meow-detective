//! 哈希计算服务
//!
//! 提供文件和数据的 SHA-256 哈希计算功能。

use infrastructure::hashing;
use std::io::{self, Read};
use std::path::Path;

/// 哈希计算服务
pub struct HashService;

impl HashService {
    /// 计算 Reader 的 SHA-256 哈希
    ///
    /// 流式读取数据并计算哈希，适用于大文件。
    pub fn sha256_reader(reader: &mut dyn Read) -> io::Result<String> {
        hashing::sha256_reader(reader)
    }

    /// 计算文件的 SHA-256 哈希
    pub fn sha256_file(path: &Path) -> io::Result<String> {
        hashing::sha256_file(path)
    }

    /// 计算字节切片的 SHA-256 哈希
    pub fn sha256_bytes(data: &[u8]) -> String {
        hashing::sha256_bytes(data)
    }

    /// 验证数据是否匹配预期的 SHA-256 哈希
    pub fn verify_sha256(data: &[u8], expected_hash: &str) -> bool {
        hashing::verify_sha256(data, expected_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn sha256_reader_basic() {
        let data = b"test data for hashing";
        let mut cursor = Cursor::new(data);
        let hash = HashService::sha256_reader(&mut cursor).unwrap();
        assert_eq!(hash, HashService::sha256_bytes(data));
    }

    #[test]
    fn sha256_bytes_hello_world() {
        let hash = HashService::sha256_bytes(b"hello world");
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn verify_sha256_correct() {
        let data = b"evidence data";
        let hash = HashService::sha256_bytes(data);
        assert!(HashService::verify_sha256(data, &hash));
    }

    #[test]
    fn verify_sha256_incorrect() {
        assert!(!HashService::verify_sha256(b"hello", &HashService::sha256_bytes(b"world")));
    }

    #[test]
    fn sha256_file_nonexistent() {
        let result = HashService::sha256_file(Path::new("/nonexistent/file"));
        assert!(result.is_err());
    }
}
