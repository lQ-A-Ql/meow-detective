//! M2 load path: a real fixture DLL passes the ABI handshake, lands in the
//! loaded set, and matches paths per its declared patterns.
#![cfg(windows)]

mod plugin_fixture_util;

use app_services::plugin_loader::load_plugins_from_dirs;
use artifacts_core::ArtifactExtractor;

#[test]
fn loads_fixture_plugin_and_matches_declared_patterns() {
    let dir = plugin_fixture_util::stage_plugins(&["good"]);
    let plugins = load_plugins_from_dirs(&[dir.path().to_path_buf()]);

    assert_eq!(plugins.len(), 1, "exactly one plugin must load");
    let extractor = &plugins[0];
    assert_eq!(extractor.id(), "meow.fixture.good");
    assert_eq!(extractor.display_name(), "Fixture Good");
    assert_eq!(extractor.family().name, "Fixture");

    // Suffix pattern "*.mfx": case-insensitive, matches any directory depth.
    assert!(extractor.supports_path("[P0]/Windows/Prefetch/FOO.MFX"));
    assert!(extractor.supports_path("C:/Evidence/bar.mfx"));
    // Exact-name pattern "fixture-exact.bin": only the file name component.
    assert!(extractor.supports_path("D:/data/fixture-exact.bin"));
    assert!(extractor.supports_path("FIXTURE-EXACT.BIN"));
    // Negative cases.
    assert!(!extractor.supports_path("C:/Evidence/FOO.PF"));
    assert!(!extractor.supports_path("D:/data/other.bin"));
    assert!(!extractor.supports_path("D:/data/fixture-exact.bin.bak"));
}

#[test]
fn create_registry_runs_plugin_discovery_without_failures() {
    // The test host has no exe-adjacent plugins directory, so discovery must
    // degrade to the built-ins without failing.
    let registry = app_services::artifact_service::create_registry();
    assert!(
        registry.families().len() >= 6,
        "built-in extractors must remain registered"
    );
}
