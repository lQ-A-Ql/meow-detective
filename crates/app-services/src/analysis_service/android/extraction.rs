use std::collections::BTreeMap;

use artifacts_android::{
    find_setting_value, parse_build_properties, parse_packages_list, parse_packages_xml,
    AndroidPackageListRecord, AndroidPackageMetadata,
};
use chrono::{DateTime, Utc};
use domain::{DataSourceId, FileEntry};
use rusqlite::Connection;
use transport::dto::{AnalysisParseStatusDto, AndroidAnalysisRunDto};

use super::model::{AndroidFact, AndroidPackage};
use super::persistence::persist_android_analysis;
use crate::analysis_service::candidates::find_candidate_by_path_suffix;
use crate::analysis_service::AnalysisServiceError;
use crate::file_service::SourceReadContext;

const MAX_BUILD_PROP_BYTES: usize = 1024 * 1024;
const MAX_PACKAGES_XML_BYTES: usize = 32 * 1024 * 1024;
const MAX_SETTINGS_XML_BYTES: usize = 8 * 1024 * 1024;

const BUILD_PROPERTY_SOURCES: &[(&str, i64)] = &[
    ("system/build.prop", 0),
    ("system_ext/build.prop", 1),
    ("product/build.prop", 2),
    ("vendor/build.prop", 3),
    ("odm/build.prop", 4),
];
const PACKAGE_XML_PATHS: &[&str] = &["data/system/packages.xml", "system/packages.xml"];
const PACKAGE_LIST_PATHS: &[&str] = &["data/system/packages.list", "system/packages.list"];
const SETTINGS_SECURE_PATHS: &[&str] = &[
    "data/system/users/0/settings_secure.xml",
    "system/users/0/settings_secure.xml",
];
const SETTINGS_GLOBAL_PATHS: &[&str] = &[
    "data/system/users/0/settings_global.xml",
    "system/users/0/settings_global.xml",
];
const MAX_PACKAGES_LIST_BYTES: usize = 8 * 1024 * 1024;

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

pub(crate) fn run_android_source_analysis(
    source_conn: &Connection,
    source_reader: &mut SourceReadContext<'_>,
    data_source_id: &DataSourceId,
) -> Result<AndroidAnalysisRunDto, AnalysisServiceError> {
    let mut warnings = Vec::new();
    let mut scanned_file_count = 0u64;
    let mut facts = collect_device_facts(
        source_conn,
        source_reader,
        &mut warnings,
        &mut scanned_file_count,
    )?;
    let packages = collect_packages(
        source_conn,
        source_reader,
        &mut warnings,
        &mut scanned_file_count,
    )?;
    facts.sort_by(|left, right| left.field.cmp(right.field));

    persist_android_analysis(source_conn, data_source_id, &facts, &packages)?;
    let status = if facts.is_empty() && packages.is_empty() {
        AnalysisParseStatusDto::NotFound
    } else if warnings.is_empty() {
        AnalysisParseStatusDto::Parsed
    } else {
        AnalysisParseStatusDto::Partial
    };
    Ok(AndroidAnalysisRunDto {
        status,
        scanned_file_count,
        device_fact_count: facts.len() as u64,
        package_count: packages.len() as u64,
        warnings,
    })
}

fn collect_device_facts(
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

fn collect_packages(
    source_conn: &Connection,
    source_reader: &mut SourceReadContext<'_>,
    warnings: &mut Vec<String>,
    scanned_file_count: &mut u64,
) -> Result<Vec<AndroidPackage>, AnalysisServiceError> {
    let mut packages = BTreeMap::new();
    if let Some((entry, bytes)) = read_first_available_file(
        source_conn,
        source_reader,
        PACKAGE_XML_PATHS,
        MAX_PACKAGES_XML_BYTES,
        warnings,
        scanned_file_count,
    )? {
        match parse_packages_xml(&bytes) {
            Ok(metadata) => {
                for package in metadata {
                    insert_package_metadata(&mut packages, package, &entry);
                }
            }
            Err(error) => warnings.push(format!("{}: {error}", entry.path)),
        }
    }
    if let Some((entry, bytes)) = read_first_available_file(
        source_conn,
        source_reader,
        PACKAGE_LIST_PATHS,
        MAX_PACKAGES_LIST_BYTES,
        warnings,
        scanned_file_count,
    )? {
        match parse_packages_list(&bytes) {
            Ok(records) => merge_packages_list(&mut packages, records, &entry),
            Err(error) => warnings.push(format!("{}: {error}", entry.path)),
        }
    }
    Ok(packages.into_values().collect())
}

fn insert_package_metadata(
    packages: &mut BTreeMap<String, AndroidPackage>,
    package: AndroidPackageMetadata,
    entry: &FileEntry,
) {
    let (install_time, install_warning) = format_package_time(package.install_time_millis);
    let (update_time, update_warning) = format_package_time(package.last_update_time_millis);
    let warning = match (install_warning, update_warning) {
        (Some(left), Some(right)) => Some(format!("{left}; {right}")),
        (Some(warning), None) | (None, Some(warning)) => Some(warning),
        (None, None) => None,
    };
    packages.insert(
        package.package_name.clone(),
        AndroidPackage {
            package_name: package.package_name,
            version_code: package.version_code,
            install_time,
            update_time,
            installer: package.installer,
            uid: package.uid,
            code_path: package.code_path,
            source_file_id: entry.id.0.clone(),
            source_path: entry.path.clone(),
            warning,
        },
    );
}

fn merge_packages_list(
    packages: &mut BTreeMap<String, AndroidPackage>,
    records: Vec<AndroidPackageListRecord>,
    entry: &FileEntry,
) {
    for record in records {
        if let Some(package) = packages.get_mut(&record.package_name) {
            if package.uid.is_none() {
                package.uid = record.uid;
            }
            continue;
        }
        packages.insert(
            record.package_name.clone(),
            AndroidPackage {
                package_name: record.package_name,
                version_code: None,
                install_time: None,
                update_time: None,
                installer: None,
                uid: record.uid,
                code_path: None,
                source_file_id: entry.id.0.clone(),
                source_path: entry.path.clone(),
                warning: Some("present in packages.list without packages.xml metadata".to_string()),
            },
        );
    }
}

fn read_first_available_file(
    source_conn: &Connection,
    source_reader: &mut SourceReadContext<'_>,
    suffixes: &[&str],
    limit: usize,
    warnings: &mut Vec<String>,
    scanned_file_count: &mut u64,
) -> Result<Option<(FileEntry, Vec<u8>)>, AnalysisServiceError> {
    let Some(entry) = find_first_candidate(source_conn, suffixes)? else {
        return Ok(None);
    };
    if entry.encrypted {
        warnings.push(format!("{} is encrypted and was not read", entry.path));
        return Ok(None);
    }
    let Some(entry_size) = entry.size else {
        warnings.push(format!(
            "{} has no catalog size and was not read",
            entry.path
        ));
        return Ok(None);
    };
    if entry_size > limit as u64 {
        warnings.push(format!(
            "{} exceeds the {} byte analysis limit",
            entry.path, limit
        ));
        return Ok(None);
    }
    *scanned_file_count = scanned_file_count.saturating_add(1);
    match source_reader.read_file_header_by_id(&entry.id, limit) {
        Ok(bytes) if bytes.len() as u64 == entry_size => Ok(Some((entry, bytes))),
        Ok(bytes) => {
            warnings.push(format!(
                "{} read length mismatch: catalog={} bytes, read={} bytes",
                entry.path,
                entry_size,
                bytes.len()
            ));
            Ok(None)
        }
        Err(error) => {
            warnings.push(format!("{} could not be read: {error}", entry.path));
            Ok(None)
        }
    }
}

fn find_first_candidate(
    source_conn: &Connection,
    suffixes: &[&str],
) -> Result<Option<FileEntry>, AnalysisServiceError> {
    for suffix in suffixes {
        if let Some(entry) = find_candidate_by_path_suffix(source_conn, suffix)? {
            return Ok(Some(entry));
        }
    }
    Ok(None)
}

fn format_package_time(value: Option<i64>) -> (Option<String>, Option<String>) {
    match value {
        None => (None, None),
        Some(value) => match DateTime::<Utc>::from_timestamp_millis(value) {
            Some(timestamp) => (Some(timestamp.to_rfc3339()), None),
            None => (
                None,
                Some("package timestamp is outside the supported range".to_string()),
            ),
        },
    }
}
