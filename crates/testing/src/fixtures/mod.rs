//! Shared test fixture path helpers.

use std::path::{Path, PathBuf};

const TINY_LOGICAL_REL: &str = "testdata/fixtures/tiny/logical";
const TINY_RAW_REL: &str = "testdata/fixtures/tiny/raw/tiny.raw";
const TINY_E01_REL: &str = "testdata/fixtures/tiny/e01/tiny.E01";

/// Returns the repository root as seen by the `testing` crate.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("testing crate should live under <repo>/crates/testing")
}

/// Tiny logical directory fixture with a few files and nested paths.
pub fn tiny_logical_dir() -> PathBuf {
    repo_root().join(TINY_LOGICAL_REL)
}

/// Tiny RAW fixture with a valid MBR signature and deterministic payload.
pub fn tiny_raw_image() -> PathBuf {
    repo_root().join(TINY_RAW_REL)
}

/// Tiny synthetic E01 fixture for default reader tests.
///
/// This is a deterministic single-segment EWF-like image with one uncompressed
/// chunk. It is not intended to represent a real filesystem image.
pub fn tiny_e01_image() -> PathBuf {
    repo_root().join(TINY_E01_REL)
}

/// Optional local E01 fixture for manual slow tests.
///
/// This intentionally returns `None` unless `FORENSICS_E01_FIXTURE` points to
/// an existing file. Default test runs must not depend on private local images.
pub fn local_e01_fixture() -> Option<PathBuf> {
    std::env::var_os("FORENSICS_E01_FIXTURE")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiny_logical_fixture_exists() {
        let root = tiny_logical_dir();
        assert!(
            root.is_dir(),
            "missing fixture directory: {}",
            root.display()
        );
        assert!(root.join("README.txt").is_file());
        assert!(root.join("Users/alice/notes.txt").is_file());
    }

    #[test]
    fn tiny_raw_fixture_exists_and_has_mbr_signature() {
        let path = tiny_raw_image();
        assert!(path.is_file(), "missing RAW fixture: {}", path.display());

        let bytes = std::fs::read(&path).expect("read tiny RAW fixture");
        assert_eq!(bytes.len(), 1024);
        assert_eq!(&bytes[510..512], &[0x55, 0xAA]);
        assert_eq!(&bytes[512..520], b"FWB-TINY");
    }

    #[test]
    fn tiny_e01_fixture_exists_and_has_evf_header() {
        let path = tiny_e01_image();
        assert!(path.is_file(), "missing E01 fixture: {}", path.display());

        let bytes = std::fs::read(&path).expect("read tiny E01 fixture");
        assert!(bytes.len() < 1024 * 1024);
        assert_eq!(&bytes[0..3], b"EVF");
        assert!(bytes
            .windows(b"FWB-TINY-E01".len())
            .any(|w| w == b"FWB-TINY-E01"));
    }

    #[test]
    fn local_e01_fixture_is_opt_in() {
        let fixture = local_e01_fixture();
        if let Some(path) = fixture {
            assert!(path.is_file());
        }
    }
}
