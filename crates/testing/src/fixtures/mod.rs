//! Shared test fixture path helpers.

use std::path::{Path, PathBuf};

const TINY_LOGICAL_REL: &str = "testdata/fixtures/tiny/logical";
const TINY_RAW_REL: &str = "testdata/fixtures/tiny/raw/tiny.raw";
const TINY_E01_REL: &str = "testdata/fixtures/tiny/e01/tiny.E01";
const TINY_SYSTEM_EVTX_REL: &str = "testdata/fixtures/tiny/evtx/system.evtx";
const TINY_REGISTRY_SYSTEM_REL: &str =
    "testdata/fixtures/tiny/logical/Windows/System32/config/SYSTEM";
const TINY_REGISTRY_SOFTWARE_REL: &str =
    "testdata/fixtures/tiny/logical/Windows/System32/config/SOFTWARE";

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

/// Tiny real System.evtx fixture for parser-path tests.
///
/// This fixture is copied from the MIT/Apache-2.0 licensed upstream `evtx`
/// repository sample set and is small enough for default CI.
pub fn tiny_system_evtx() -> PathBuf {
    repo_root().join(TINY_SYSTEM_EVTX_REL)
}

/// Tiny synthetic SYSTEM registry hive in the logical fixture tree.
///
/// The hive only contains Analysis-targeted keys and is not a full Windows
/// registry corpus.
pub fn tiny_registry_system_hive() -> PathBuf {
    repo_root().join(TINY_REGISTRY_SYSTEM_REL)
}

/// Tiny synthetic SOFTWARE registry hive in the logical fixture tree.
///
/// The hive only contains Analysis-targeted keys and is not a full Windows
/// registry corpus.
pub fn tiny_registry_software_hive() -> PathBuf {
    repo_root().join(TINY_REGISTRY_SOFTWARE_REL)
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

/// Optional local Liu Yang E01 fixture for manual functional regression tests.
///
/// This private real-world sample is not committed to the repository. Set
/// `FORENSICS_LIUYANG_E01_FIXTURE` to a local E01 path when running the ignored
/// Liu Yang regression tests manually.
pub fn local_liuyang_e01_fixture() -> Option<PathBuf> {
    std::env::var_os("FORENSICS_LIUYANG_E01_FIXTURE")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builders::registry;

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
    fn tiny_system_evtx_fixture_exists_and_has_evtx_header() {
        let path = tiny_system_evtx();
        assert!(
            path.is_file(),
            "missing System.evtx fixture: {}",
            path.display()
        );

        let bytes = std::fs::read(&path).expect("read tiny System.evtx fixture");
        assert!(bytes.len() < 2 * 1024 * 1024);
        assert_eq!(&bytes[0..8], b"ElfFile\0");
    }

    #[test]
    fn tiny_registry_system_fixture_matches_builder() {
        let path = tiny_registry_system_hive();
        assert!(
            path.is_file(),
            "missing SYSTEM registry fixture: {}",
            path.display()
        );

        let bytes = std::fs::read(&path).expect("read tiny SYSTEM hive fixture");
        assert_eq!(&bytes[0..4], b"regf");
        assert_eq!(bytes, registry::synthetic_system_hive());
    }

    #[test]
    fn tiny_registry_software_fixture_matches_builder() {
        let path = tiny_registry_software_hive();
        assert!(
            path.is_file(),
            "missing SOFTWARE registry fixture: {}",
            path.display()
        );

        let bytes = std::fs::read(&path).expect("read tiny SOFTWARE hive fixture");
        assert_eq!(&bytes[0..4], b"regf");
        assert_eq!(bytes, registry::synthetic_software_hive());
    }

    #[test]
    fn local_e01_fixture_is_opt_in() {
        let fixture = local_e01_fixture();
        if let Some(path) = fixture {
            assert!(path.is_file());
        }
    }

    #[test]
    fn local_liuyang_e01_fixture_is_opt_in() {
        let fixture = local_liuyang_e01_fixture();
        if let Some(path) = fixture {
            assert!(path.is_file());
        }
    }
}
