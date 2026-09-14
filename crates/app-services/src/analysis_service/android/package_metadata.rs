use std::collections::BTreeMap;

use artifacts_android::{
    parse_packages_list, parse_packages_xml, AndroidPackageListRecord, AndroidPackageMetadata,
};
use chrono::{DateTime, Utc};
use domain::FileEntry;
use rusqlite::Connection;

use super::model::AndroidPackage;
use super::source_files::read_first_available_file;
use crate::analysis_service::AnalysisServiceError;
use crate::file_service::SourceReadContext;

const MAX_PACKAGES_XML_BYTES: usize = 32 * 1024 * 1024;
const MAX_PACKAGES_LIST_BYTES: usize = 8 * 1024 * 1024;
const PACKAGE_XML_PATHS: &[&str] = &["data/system/packages.xml", "system/packages.xml"];
const PACKAGE_LIST_PATHS: &[&str] = &["data/system/packages.list", "system/packages.list"];

pub(super) fn collect_packages(
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
