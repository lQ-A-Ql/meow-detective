//! Shared test fixture path helpers.

use std::path::{Path, PathBuf};

const TINY_LOGICAL_REL: &str = "testdata/fixtures/public-small/logical";
const TINY_RAW_REL: &str = "testdata/fixtures/public-small/raw/tiny.raw";
const TINY_E01_REL: &str = "testdata/fixtures/public-small/e01/tiny.E01";
const TINY_SYSTEM_EVTX_REL: &str = "testdata/fixtures/public-small/evtx/system.evtx";
const TINY_REGISTRY_SYSTEM_REL: &str =
    "testdata/fixtures/public-small/logical/Windows/System32/config/SYSTEM";
const TINY_REGISTRY_SOFTWARE_REL: &str =
    "testdata/fixtures/public-small/logical/Windows/System32/config/SOFTWARE";

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
#[path = "../../tests/unit/fixtures.rs"]
mod tests;
