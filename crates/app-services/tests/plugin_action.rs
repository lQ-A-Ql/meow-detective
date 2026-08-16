//! Plugin action channel (ABI doc §3 optional export): symbol-missing
//! degradation, `describe`, action invocation, and plugin-side panic
//! self-catch, all across the real DLL boundary.
#![cfg(windows)]

mod plugin_fixture_util;

use app_services::plugin_loader::load_plugins_from_dirs;

fn load_one(
    names: &[&str],
) -> (
    tempfile::TempDir,
    app_services::plugin_loader::PluginExtractor,
) {
    let dir = plugin_fixture_util::stage_plugins(names);
    let plugins = load_plugins_from_dirs(&[dir.path().to_path_buf()]);
    assert_eq!(plugins.len(), 1);
    (dir, plugins.into_iter().next().expect("one plugin"))
}

#[test]
fn plugin_without_action_symbol_degrades_gracefully() {
    let (_dir, plugin) = load_one(&["good"]);

    assert!(!plugin.has_actions());
    assert_eq!(
        plugin.describe_actions().expect("describe degrades"),
        serde_json::Value::Array(Vec::new())
    );
    let error = plugin
        .call_action("anything", &serde_json::json!({}))
        .expect_err("action call on a plugin without the channel must fail");
    assert!(
        error.contains("does not export the action channel"),
        "{error}"
    );
}

#[test]
fn describe_returns_the_declared_action_list() {
    let (_dir, plugin) = load_one(&["action"]);

    assert!(plugin.has_actions());
    let actions = plugin.describe_actions().expect("describe");
    let echo = &actions[0];
    assert_eq!(echo["id"], "echo");
    assert_eq!(echo["label"], "回显");
    assert_eq!(echo["inputKind"], "none");
    assert_eq!(actions[1]["id"], "panic");
}

#[test]
fn action_call_round_trips_params() {
    let (_dir, plugin) = load_one(&["action"]);

    let response = plugin
        .call_action("echo", &serde_json::json!({ "hello": "world", "n": 7 }))
        .expect("echo action");
    assert_eq!(response["echo"]["hello"], "world");
    assert_eq!(response["echo"]["n"], 7);
}

#[test]
fn unknown_action_maps_to_error() {
    let (_dir, plugin) = load_one(&["action"]);

    let error = plugin
        .call_action("nope", &serde_json::json!({}))
        .expect_err("unknown action must fail");
    assert!(error.contains("Unsupported"), "{error}");
}

#[test]
fn plugin_side_panic_is_self_caught_as_internal_error() {
    let (_dir, plugin) = load_one(&["action"]);

    // The fixture's `panic` action panics inside the DLL; guarded_action
    // catches it there and returns InternalError instead of unwinding across
    // the FFI boundary (which would abort the host process on MSVC).
    let error = plugin
        .call_action("panic", &serde_json::json!({}))
        .expect_err("panicking action must surface as an error");
    assert!(error.contains("InternalError"), "{error}");
    assert!(error.contains("plugin panicked during action"), "{error}");
}
