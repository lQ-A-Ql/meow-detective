//! Shared test fixture path helpers.

use std::path::{Path, PathBuf};

const TINY_LOGICAL_REL: &str = "testdata/fixtures/tiny/logical";
const TINY_RAW_REL: &str = "testdata/fixtures/tiny/raw/tiny.raw";

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
    fn local_e01_fixture_is_opt_in() {
        let fixture = local_e01_fixture();
        if let Some(path) = fixture {
            assert!(path.is_file());
        }
    }
}
