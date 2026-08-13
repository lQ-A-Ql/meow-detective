use super::*;

#[test]
fn plugin_capability_is_selectable_on_both_platforms() {
    let windows = select_capabilities(
        DataSourcePlatform::Windows,
        WINDOWS_CAPABILITIES,
        &[PLUGIN_CAPABILITY_KEY],
    )
    .expect("Windows plugin capability selection");
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].platform, DataSourcePlatform::Windows);

    let linux = select_capabilities(
        DataSourcePlatform::Linux,
        LINUX_CAPABILITIES,
        &[PLUGIN_CAPABILITY_KEY],
    )
    .expect("Linux plugin capability selection");
    assert_eq!(linux.len(), 1);
    assert_eq!(linux[0].platform, DataSourcePlatform::Linux);
}

#[test]
fn default_selection_includes_plugin_capability_key() {
    let windows = select_capabilities(DataSourcePlatform::Windows, WINDOWS_CAPABILITIES, &[])
        .expect("default Windows capabilities");
    assert!(windows
        .iter()
        .any(|capability| capability.key == PLUGIN_CAPABILITY_KEY));
    let linux = select_capabilities(DataSourcePlatform::Linux, LINUX_CAPABILITIES, &[])
        .expect("default Linux capabilities");
    assert!(linux
        .iter()
        .any(|capability| capability.key == PLUGIN_CAPABILITY_KEY));
}

#[test]
fn plugin_capability_drops_out_without_loaded_plugins() {
    let mut selected = select_capabilities(
        DataSourcePlatform::Windows,
        WINDOWS_CAPABILITIES,
        &[PLUGIN_CAPABILITY_KEY],
    )
    .expect("plugin capability selection");
    retain_active_plugin_capability(&mut selected, false);
    assert!(selected.is_empty());

    let mut selected = select_capabilities(
        DataSourcePlatform::Windows,
        WINDOWS_CAPABILITIES,
        &["Registry", PLUGIN_CAPABILITY_KEY],
    )
    .expect("mixed selection");
    retain_active_plugin_capability(&mut selected, false);
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].key, "Registry");

    retain_active_plugin_capability(&mut selected, true);
    assert_eq!(selected.len(), 1, "non-plugin capabilities stay untouched");

    let mut selected = select_capabilities(
        DataSourcePlatform::Windows,
        WINDOWS_CAPABILITIES,
        &[PLUGIN_CAPABILITY_KEY],
    )
    .expect("plugin capability selection");
    retain_active_plugin_capability(&mut selected, true);
    assert_eq!(selected.len(), 1);
}

#[test]
fn unknown_capability_is_still_rejected() {
    let result = select_capabilities(
        DataSourcePlatform::Windows,
        WINDOWS_CAPABILITIES,
        &["NoSuchCapability"],
    );
    assert!(matches!(result, Err(AnalysisServiceError::InvalidInput(_))));
}
