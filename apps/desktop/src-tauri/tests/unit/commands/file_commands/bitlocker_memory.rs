use super::validate_memory_image_path;

#[test]
fn missing_memory_image_is_rejected_without_echoing_the_path() {
    let missing = std::env::temp_dir().join(format!(
        "Meow_Detective-missing-memory-{}.mem",
        uuid::Uuid::new_v4()
    ));
    let missing_text = missing.to_string_lossy().into_owned();
    let error = validate_memory_image_path(missing_text.clone())
        .expect_err("missing memory image must be rejected");

    assert_eq!(error.code, "INVALID_INPUT");
    assert_eq!(error.category, "validation");
    assert!(!error.message.contains(&missing_text));
}

#[test]
fn readable_memory_image_file_is_accepted() {
    let file = tempfile::NamedTempFile::new().expect("temporary memory image");

    assert_eq!(
        validate_memory_image_path(file.path().to_string_lossy().into_owned())
            .expect("readable file"),
        file.path()
    );
}
