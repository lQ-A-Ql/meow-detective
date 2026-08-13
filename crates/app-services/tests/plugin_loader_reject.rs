//! M2 reject path: ABI mismatch, missing exports and duplicate plugin ids are
//! refused with a log entry while the remaining plugins keep loading.
#![cfg(windows)]

mod plugin_fixture_util;

use app_services::plugin_loader::load_plugins_from_dirs;
use artifacts_core::ArtifactExtractor;

#[test]
fn rejects_duplicate_plugin_id() {
    let dir = plugin_fixture_util::stage_plugins(&["good", "good_dup"]);
    let plugins = load_plugins_from_dirs(&[dir.path().to_path_buf()]);

    assert_eq!(
        plugins.len(),
        1,
        "the duplicate id must be refused (first-seen wins)"
    );
    assert_eq!(plugins[0].id(), "meow.fixture.good");
}

#[test]
fn rejects_abi_mismatch_and_missing_export_without_blocking_valid_plugins() {
    let dir = plugin_fixture_util::stage_plugins(&["abi_mismatch", "missing_export", "good"]);
    let plugins = load_plugins_from_dirs(&[dir.path().to_path_buf()]);

    let ids: Vec<&str> = plugins.iter().map(|p| p.id()).collect();
    assert_eq!(
        ids,
        vec!["meow.fixture.good"],
        "only the valid plugin may load; got {ids:?}"
    );
}
