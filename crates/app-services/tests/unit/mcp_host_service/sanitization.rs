use super::*;
use transport::dto::PluginFamilyCountDto;

#[test]
fn plugin_metadata_preserves_version_and_family_counts_without_raw_diagnostics() {
    let summary = plugin_summary(PluginModuleDto {
        plugin_id: "meow.plugin.test".into(),
        display_name: "Test".into(),
        plugin_version: "2.0.0".into(),
        evidence_platform: "windows".into(),
        families: vec![PluginFamilyCountDto {
            family: "Messages".into(),
            count: 19,
        }],
        total_count: 19,
        warnings: vec!["C:\\private\\sensitive-file credential=hidden".into()],
    });
    assert_eq!(summary["pluginVersion"], "2.0.0");
    assert_eq!(summary["families"][0]["count"], 19);
    assert_eq!(summary["warningCount"], 1);
    assert!(summary.get("warnings").is_none());
    assert!(!summary.to_string().contains("private"));
    assert!(!summary.to_string().contains("credential"));
}
