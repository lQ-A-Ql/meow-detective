use domain::{DataSourcePlatform, DataSourcePlatformParseError};

#[test]
fn platform_storage_values_are_stable_and_round_trip() {
    let cases = [
        (DataSourcePlatform::Windows, "windows"),
        (DataSourcePlatform::Linux, "linux"),
        (DataSourcePlatform::Unknown, "unknown"),
    ];

    for (platform, storage_value) in cases {
        assert_eq!(platform.as_storage_str(), storage_value);
        assert_eq!(platform.to_string(), storage_value);
        assert_eq!(
            DataSourcePlatform::from_storage_str(Some(storage_value)),
            Ok(platform)
        );
    }
}

#[test]
fn absent_or_blank_storage_values_map_to_unknown() {
    for value in [None, Some(""), Some("   ")] {
        assert_eq!(
            DataSourcePlatform::from_storage_str(value),
            Ok(DataSourcePlatform::Unknown)
        );
    }

    assert_eq!(
        DataSourcePlatform::from_storage_str(Some("unknown")),
        Ok(DataSourcePlatform::Unknown)
    );
    assert_eq!(
        DataSourcePlatform::from_storage_str(Some(" LiNuX ")),
        Ok(DataSourcePlatform::Linux)
    );
    assert_eq!(
        DataSourcePlatform::from_storage_str(Some("UNKNOWN")),
        Ok(DataSourcePlatform::Unknown)
    );
}

#[test]
fn explicit_platform_accepts_only_windows_or_linux() {
    assert_eq!(
        DataSourcePlatform::parse_explicit(" windows "),
        Ok(DataSourcePlatform::Windows)
    );
    assert_eq!(
        DataSourcePlatform::parse_explicit("LINUX"),
        Ok(DataSourcePlatform::Linux)
    );
    assert_eq!(
        DataSourcePlatform::parse_explicit("unknown"),
        Err(DataSourcePlatformParseError::UnknownExplicitValue)
    );
    assert_eq!(
        DataSourcePlatform::parse_explicit(""),
        Err(DataSourcePlatformParseError::MissingExplicitValue)
    );
    assert_eq!(
        DataSourcePlatform::parse_explicit("   "),
        Err(DataSourcePlatformParseError::MissingExplicitValue)
    );
}

#[test]
fn retired_and_invalid_platforms_are_rejected_instead_of_downgraded() {
    for value in ["macos", "android", "not-a-platform"] {
        let expected = DataSourcePlatformParseError::UnsupportedValue {
            value: value.to_owned(),
        };

        assert_eq!(
            DataSourcePlatform::from_storage_str(Some(value)),
            Err(expected.clone())
        );
        assert_eq!(DataSourcePlatform::parse_explicit(value), Err(expected));
    }

    assert_eq!(
        DataSourcePlatform::from_storage_str(Some(" MACOS ")),
        Err(DataSourcePlatformParseError::UnsupportedValue {
            value: "MACOS".to_string(),
        })
    );
}
