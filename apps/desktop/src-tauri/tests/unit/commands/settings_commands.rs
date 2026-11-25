use super::*;

#[test]
fn load_app_settings_returns_default_when_file_is_missing() {
    let temp = tempfile::tempdir().unwrap();
    let settings = load_app_settings(&temp.path().join("missing.json")).unwrap();

    assert!(!settings.case_root.trim().is_empty());
}

#[test]
fn persisted_app_settings_roundtrip() {
    let temp = tempfile::tempdir().unwrap();
    let case_root = temp.path().join("cases");
    let search_root = temp.path().join("images");
    std::fs::create_dir_all(&case_root).unwrap();
    std::fs::create_dir_all(&search_root).unwrap();
    let path = temp.path().join("settings.json");
    let settings = AppSettingsDto {
        case_root: case_root.display().to_string(),
        image_search_paths: vec![search_root.display().to_string()],
        dev_event_trace: true,
        max_import_workers: Some(4),
        max_analysis_workers: Some(2),
        import_analysis_mode: "budgetedContent".to_string(),
        hex_chunk_bytes: 32 * 1024,
        max_viewer_range_length: 2 * 1024 * 1024,
        max_inline_image_preview_bytes: 1024 * 1024,
        max_inline_media_preview_bytes: 10 * 1024 * 1024,
    };

    std::fs::write(&path, serde_json::to_string(&settings).unwrap()).unwrap();
    let loaded = load_app_settings(&path).unwrap();

    assert_eq!(loaded.case_root, settings.case_root);
    assert_eq!(loaded.image_search_paths, settings.image_search_paths);
    assert!(loaded.dev_event_trace);
    assert_eq!(loaded.max_import_workers, Some(4));
    assert_eq!(loaded.max_analysis_workers, Some(2));
    assert_eq!(loaded.import_analysis_mode, "budgetedContent");
}
