use std::collections::BTreeMap;

use artifacts_android::{find_setting_value, parse_build_properties};
use rusqlite::Connection;

use super::model::AndroidFact;
use super::source_files::read_first_available_file;
use crate::analysis_service::AnalysisServiceError;
use crate::file_service::SourceReadContext;

const MAX_BUILD_PROP_BYTES: usize = 1024 * 1024;
const MAX_SETTINGS_XML_BYTES: usize = 8 * 1024 * 1024;
const SETTINGS_SECURE_PATHS: &[&str] = &[
    "data/system/users/0/settings_secure.xml",
    "system/users/0/settings_secure.xml",
];
const SETTINGS_GLOBAL_PATHS: &[&str] = &[
    "data/system/users/0/settings_global.xml",
    "system/users/0/settings_global.xml",
];
const BUILD_PROPERTY_SOURCES: &[(&str, i64)] = &[
    ("system/build.prop", 0),
    ("system_ext/build.prop", 1),
    ("product/build.prop", 2),
    ("vendor/build.prop", 3),
    ("odm/build.prop", 4),
];
const DEVICE_FIELDS: &[(&str, &[&str])] = &[
    ("model", &["ro.product.model"]),
    ("manufacturer", &["ro.product.manufacturer"]),
    ("brand", &["ro.product.brand"]),
    ("device", &["ro.product.device"]),
    ("androidVersion", &["ro.build.version.release"]),
    ("sdkInt", &["ro.build.version.sdk"]),
    ("buildId", &["ro.build.id"]),
    ("buildDisplayId", &["ro.build.display.id"]),
    ("buildFingerprint", &["ro.build.fingerprint"]),
    ("serialNumber", &["ro.serialno", "ro.boot.serialno"]),
];

pub(super) fn collect_device_facts(
    source_conn: &Connection,
    source_reader: &mut SourceReadContext<'_>,
    warnings: &mut Vec<String>,
    scanned_file_count: &mut u64,
) -> Result<Vec<AndroidFact>, AnalysisServiceError> {
    let mut selected = BTreeMap::new();
    for (suffix, source_rank) in BUILD_PROPERTY_SOURCES {
        let Some((entry, bytes)) = read_first_available_file(
            source_conn,
            source_reader,
            &[*suffix],
            MAX_BUILD_PROP_BYTES,
            warnings,
            scanned_file_count,
        )?
        else {
            continue;
        };
        let properties = match parse_build_properties(&bytes) {
            Ok(properties) => properties,
            Err(error) => {
                warnings.push(format!("{}: {error}", entry.path));
                continue;
            }
        };
        for (field, property_keys) in DEVICE_FIELDS {
            if selected.contains_key(*field) {
                continue;
            }
            let value = property_keys.iter().find_map(|key| {
                properties
                    .iter()
                    .find(|property| property.key == *key)
                    .map(|property| property.value.clone())
                    .filter(|value| !value.is_empty())
            });
            if let Some(value) = value {
                selected.insert(
                    *field,
                    AndroidFact {
                        field,
                        value,
                        source_file_id: entry.id.0.clone(),
                        source_path: entry.path.clone(),
                        source_rank: *source_rank,
                        confidence: "direct",
                        warning: None,
                    },
                );
            }
        }
    }
    collect_android_id(
        source_conn,
        source_reader,
        warnings,
        scanned_file_count,
        &mut selected,
    )?;
    collect_device_name(
        source_conn,
        source_reader,
        warnings,
        scanned_file_count,
        &mut selected,
    )?;
    Ok(selected.into_values().collect())
}

fn collect_android_id(
    source_conn: &Connection,
    source_reader: &mut SourceReadContext<'_>,
    warnings: &mut Vec<String>,
    scanned_file_count: &mut u64,
    selected: &mut BTreeMap<&'static str, AndroidFact>,
) -> Result<(), AnalysisServiceError> {
    let Some((entry, bytes)) = read_first_available_file(
        source_conn,
        source_reader,
        SETTINGS_SECURE_PATHS,
        MAX_SETTINGS_XML_BYTES,
        warnings,
        scanned_file_count,
    )?
    else {
        return Ok(());
    };
    match find_setting_value(&bytes, "android_id") {
        Ok(Some(value)) => {
            selected.insert(
                "androidId",
                AndroidFact {
                    field: "androidId",
                    value,
                    source_file_id: entry.id.0,
                    source_path: entry.path,
                    source_rank: 0,
                    confidence: "contextual",
                    warning: Some(
                        "Android 8+ can scope ANDROID_ID by app signing key and user".to_string(),
                    ),
                },
            );
        }
        Ok(None) => {}
        Err(error) => warnings.push(format!("{}: {error}", entry.path)),
    }
    Ok(())
}

fn collect_device_name(
    source_conn: &Connection,
    source_reader: &mut SourceReadContext<'_>,
    warnings: &mut Vec<String>,
    scanned_file_count: &mut u64,
    selected: &mut BTreeMap<&'static str, AndroidFact>,
) -> Result<(), AnalysisServiceError> {
    let Some((entry, bytes)) = read_first_available_file(
        source_conn,
        source_reader,
        SETTINGS_GLOBAL_PATHS,
        MAX_SETTINGS_XML_BYTES,
        warnings,
        scanned_file_count,
    )?
    else {
        return Ok(());
    };
    match find_setting_value(&bytes, "device_name") {
        Ok(Some(value)) => {
            selected.insert(
                "deviceName",
                AndroidFact {
                    field: "deviceName",
                    value,
                    source_file_id: entry.id.0,
                    source_path: entry.path,
                    source_rank: 0,
                    confidence: "direct",
                    warning: None,
                },
            );
        }
        Ok(None) => {}
        Err(error) => warnings.push(format!("{}: {error}", entry.path)),
    }
    Ok(())
}
