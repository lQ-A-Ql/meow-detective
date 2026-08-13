//! M2 panic path: a panicking plugin surfaces as a per-extractor error
//! (logged as a warning upstream) and never interrupts the remaining batch.
//!
//! The cross-boundary behavior is pinned down by a subprocess test: on MSVC a
//! panic that unwinds out of the DLL is a foreign exception to the host's
//! Rust runtime, so the process aborts even with the host-side
//! `catch_unwind`. Contract §3 therefore makes plugins catch their own
//! panics; the host `catch_unwind` is defense in depth.
#![cfg(windows)]

mod plugin_fixture_util;

use app_services::plugin_loader::load_plugins_from_dirs;
use artifacts_core::{ArtifactContext, ArtifactExtractor, VecSink};
use domain::FileEntryId;

const CHILD_ENV: &str = "MEOW_PLUGIN_PANIC_UNWIND_CHILD";

fn ctx(file_id: &str, path: &str) -> ArtifactContext {
    ArtifactContext {
        file_id: FileEntryId(file_id.to_string()),
        file_path: path.to_string(),
        reader: Box::new(std::io::Cursor::new(vec![0u8; 8])),
    }
}

#[test]
fn plugin_panic_becomes_error_and_does_not_block_other_plugins() {
    let dir = plugin_fixture_util::stage_plugins(&["panic_extract", "good"]);
    let plugins = load_plugins_from_dirs(&[dir.path().to_path_buf()]);
    assert_eq!(plugins.len(), 2);
    let by_id = |id: &str| {
        plugins
            .iter()
            .find(|p| p.id() == id)
            .unwrap_or_else(|| panic!("plugin {id} must be loaded"))
    };

    let mut sink = VecSink::new();
    let error = by_id("meow.fixture.panic")
        .run(ctx("ds:1:1", "C:/Evidence/X.pnc"), &mut sink)
        .err()
        .expect("a plugin panic must surface as an extractor error");
    assert!(
        error.contains("InternalError") && error.contains("fixture panic"),
        "error should carry the plugin's panic report: {error}"
    );

    // The batch continues: the sibling plugin still extracts normally.
    let mut sink = VecSink::new();
    let report = by_id("meow.fixture.good")
        .run(ctx("ds:1:2", "C:/Evidence/ok.mfx"), &mut sink)
        .expect("sibling plugin must keep working after a panic");
    assert_eq!(report.artifacts_found, 1);
    assert_eq!(sink.artifacts.len(), 1);
}

/// A panic that violates the contract and unwinds across the DLL boundary
/// aborts the process on MSVC (foreign exception to the host runtime). This
/// subprocess test pins that documented limitation so it cannot silently
/// change; see docs/plugin-abi-contract-design.md §8.
#[test]
fn cross_boundary_panic_aborts_the_process() {
    if std::env::var_os(CHILD_ENV).is_some() {
        let dir = plugin_fixture_util::stage_plugins(&["panic_unwind"]);
        let plugins = load_plugins_from_dirs(&[dir.path().to_path_buf()]);
        assert_eq!(plugins.len(), 1);
        let mut sink = VecSink::new();
        // Expected to abort the process before returning.
        let _ = plugins[0].run(ctx("ds:1:9", "C:/Evidence/x.pnu"), &mut sink);
        eprintln!("cross-boundary panic returned normally");
        return;
    }

    let exe = std::env::current_exe().expect("current test exe");
    let status = std::process::Command::new(exe)
        .args(["cross_boundary_panic_aborts_the_process", "--exact"])
        .env(CHILD_ENV, "1")
        .status()
        .expect("spawn panic child process");
    assert!(
        !status.success(),
        "a cross-boundary plugin panic must abort the child process"
    );
}
