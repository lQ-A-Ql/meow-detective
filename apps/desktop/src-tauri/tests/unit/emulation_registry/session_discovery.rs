use std::path::Path;

use super::find_active_session_with;

#[test]
fn only_running_workspace_for_requested_source_is_selected() {
    let root = tempfile::tempdir().expect("temporary case root");
    let active = write_workspace(
        root.path(),
        "11111111-1111-4111-8111-111111111111",
        "source-a",
    );
    let other = write_workspace(
        root.path(),
        "22222222-2222-4222-8222-222222222222",
        "source-a",
    );
    let unrelated = write_workspace(
        root.path(),
        "33333333-3333-4333-8333-333333333333",
        "source-b",
    );

    let active_vmx = active.join("machine.vmx");
    let result = find_active_session_with(root.path(), "source-a", |path| {
        Ok(path == active_vmx.as_path())
    })
    .expect("discovery should succeed");

    assert_eq!(
        result.as_deref(),
        Some("emulation-11111111-1111-4111-8111-111111111111")
    );
    assert_ne!(other, unrelated);
}

#[test]
fn malformed_or_stale_provenance_is_ignored() {
    let root = tempfile::tempdir().expect("temporary case root");
    let stale = root.path().join("emulation").join("stale");
    std::fs::create_dir_all(&stale).expect("stale workspace");
    std::fs::write(stale.join("provenance.json"), b"not-json").expect("provenance");
    std::fs::write(stale.join("machine.vmx"), b"config.version = \"8\"").expect("vmx");

    let result = find_active_session_with(root.path(), "source-a", |_path| Ok(true))
        .expect("discovery should succeed");
    assert!(result.is_none());
}

fn write_workspace(root: &Path, uuid: &str, source: &str) -> std::path::PathBuf {
    let directory = root.join("emulation").join(uuid);
    std::fs::create_dir_all(&directory).expect("workspace");
    std::fs::write(
        directory.join("provenance.json"),
        format!(r#"{{"sessionId":"emulation-{uuid}","dataSourceId":"{source}"}}"#),
    )
    .expect("provenance");
    std::fs::write(directory.join("machine.vmx"), b"config.version = \"8\"").expect("vmx");
    directory
}
