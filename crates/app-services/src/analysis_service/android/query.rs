use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};

use artifacts_android::{inspect_apk, parse_package_restrictions, AndroidPackageRestriction};
use base64::{engine::general_purpose::STANDARD, Engine};
use domain::{DataSourceId, FileEntry};
use rusqlite::{params, Connection, OptionalExtension};
use transport::dto::{
    AnalysisParseStatusDto, AndroidDeviceFactDto, AndroidDeviceInfoDto, AndroidPackageDto,
    AndroidPackageSummaryDto,
};

use crate::analysis_service::candidates::row_to_file_entry_for_analysis;
use crate::analysis_service::AnalysisServiceError;
use crate::file_service::{RangeContentReader, SourceReadContext};

const MAX_APK_BYTES: u64 = 512 * 1024 * 1024;
const MAX_STREAMED_APK_BYTES: u64 = 64 * 1024 * 1024;
const MAX_PACKAGE_RESTRICTIONS_BYTES: usize = 8 * 1024 * 1024;

pub(crate) fn get_android_device_info(
    connection: &Connection,
    data_source_id: &DataSourceId,
) -> Result<AndroidDeviceInfoDto, AnalysisServiceError> {
    let mut statement = connection.prepare(
        "SELECT field_key, field_value, confidence, source_file_id, source_path, parser, warning
         FROM android_system_facts
         WHERE data_source_id = ?1
         ORDER BY source_rank ASC, field_key ASC",
    )?;
    let facts = statement
        .query_map([data_source_id.0.as_str()], |row| {
            Ok(AndroidDeviceFactDto {
                field: row.get(0)?,
                value: row.get(1)?,
                confidence: row.get(2)?,
                source_file_id: row.get(3)?,
                source_path: row.get(4)?,
                parser: row.get(5)?,
                warning: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let values = facts
        .iter()
        .map(|fact| (fact.field.as_str(), fact.value.clone()))
        .collect::<BTreeMap<_, _>>();
    let status = if facts.is_empty() {
        AnalysisParseStatusDto::NotParsed
    } else {
        AnalysisParseStatusDto::Parsed
    };
    let warnings = facts
        .iter()
        .filter_map(|fact| fact.warning.clone())
        .collect();
    Ok(AndroidDeviceInfoDto {
        status,
        model: values.get("model").cloned(),
        manufacturer: values.get("manufacturer").cloned(),
        android_version: values.get("androidVersion").cloned(),
        sdk_int: values.get("sdkInt").cloned(),
        build_id: values.get("buildId").cloned(),
        build_fingerprint: values.get("buildFingerprint").cloned(),
        serial_number: values.get("serialNumber").cloned(),
        android_id: values.get("androidId").cloned(),
        imei: None,
        facts,
        warnings,
    })
}

pub(crate) fn get_android_package_summary(
    connection: &Connection,
    data_source_id: &DataSourceId,
    offset: u64,
    limit: u32,
) -> Result<AndroidPackageSummaryDto, AnalysisServiceError> {
    let total_count = connection.query_row(
        "SELECT COUNT(*) FROM android_packages WHERE data_source_id = ?1",
        [data_source_id.0.as_str()],
        |row| row.get(0),
    )?;
    let mut statement = connection.prepare(
        "SELECT package_name, app_name, version_code, install_time, update_time, installer, uid,
                code_path, source_file_id, source_path, parser, warning
         FROM android_packages
         WHERE data_source_id = ?1
         ORDER BY package_name COLLATE NOCASE ASC
         LIMIT ?2 OFFSET ?3",
    )?;
    let packages = statement
        .query_map(params![data_source_id.0.as_str(), limit, offset], |row| {
            Ok(AndroidPackageDto {
                package_name: row.get(0)?,
                app_name: row.get(1)?,
                version_code: row.get(2)?,
                install_time: row.get(3)?,
                update_time: row.get(4)?,
                installer: row.get(5)?,
                uid: row.get(6)?,
                user_id: None,
                user_state: None,
                code_path: row.get(7)?,
                source_file_id: row.get(8)?,
                source_path: row.get(9)?,
                parser: row.get(10)?,
                icon_data_url: None,
                warning: row.get(11)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let warnings = packages
        .iter()
        .filter_map(|package| package.warning.clone())
        .collect();
    Ok(AndroidPackageSummaryDto {
        status: if total_count == 0 {
            AnalysisParseStatusDto::NotParsed
        } else {
            AnalysisParseStatusDto::Parsed
        },
        total_count,
        page_total: packages.len() as u64,
        packages,
        warnings,
    })
}

pub(crate) fn hydrate_package_presentations(
    connection: &Connection,
    source_reader: &mut SourceReadContext<'_>,
    summary: &mut AndroidPackageSummaryDto,
) -> Result<(), AnalysisServiceError> {
    hydrate_package_restrictions(connection, source_reader, summary)?;
    for package in &mut summary.packages {
        let Some(code_path) = package.code_path.as_deref() else {
            continue;
        };
        let Some(apk) = find_apk_file(connection, code_path)? else {
            append_package_warning(
                package,
                "APK codePath was not present in the imported file catalog".to_string(),
            );
            continue;
        };
        if apk.encrypted || apk.size.is_none_or(|size| size > MAX_APK_BYTES) {
            continue;
        }
        match source_reader.open_file_range_by_id(&apk.id) {
            Ok(RangeContentReader::Seekable(reader)) => {
                apply_apk_presentation(package, inspect_apk(reader));
            }
            Ok(RangeContentReader::Streaming(mut reader)) => {
                if apk.size.is_none_or(|size| size > MAX_STREAMED_APK_BYTES) {
                    continue;
                }
                let mut bytes = Vec::new();
                reader
                    .read_to_end(&mut bytes)
                    .map_err(|error| AnalysisServiceError::Io(std::io::Error::other(error)))?;
                apply_apk_presentation(package, inspect_apk(Cursor::new(bytes)));
            }
            Err(error) => append_package_warning(package, error.to_string()),
        }
    }
    summary.warnings = summary
        .packages
        .iter()
        .filter_map(|package| package.warning.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(())
}

fn hydrate_package_restrictions(
    connection: &Connection,
    source_reader: &mut SourceReadContext<'_>,
    summary: &mut AndroidPackageSummaryDto,
) -> Result<(), AnalysisServiceError> {
    let mut restrictions = BTreeMap::new();
    for entry in find_restriction_files(connection)? {
        if entry.encrypted
            || entry
                .size
                .is_none_or(|size| size > MAX_PACKAGE_RESTRICTIONS_BYTES as u64)
        {
            continue;
        }
        let Ok(bytes) =
            source_reader.read_file_header_by_id(&entry.id, MAX_PACKAGE_RESTRICTIONS_BYTES)
        else {
            continue;
        };
        let Ok(parsed) = parse_package_restrictions(&bytes) else {
            continue;
        };
        let user_id = user_id_from_path(&entry.path).unwrap_or(0);
        for restriction in parsed {
            insert_restriction(&mut restrictions, user_id, restriction);
        }
    }
    for package in &mut summary.packages {
        let Some((user_id, restriction)) = restrictions.get(&package.package_name) else {
            continue;
        };
        package.user_id = Some(*user_id);
        package.user_state = restriction_state(restriction).map(str::to_string);
    }
    Ok(())
}

fn find_restriction_files(connection: &Connection) -> Result<Vec<FileEntry>, AnalysisServiceError> {
    let mut statement = connection.prepare(
        "SELECT id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted,
                hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256,
                encrypted, read_only, archive, unix_mode
         FROM file_entries
         WHERE entry_type = 'file' COLLATE NOCASE
           AND REPLACE(LOWER(path), '\\', '/') LIKE '%package-restrictions.xml'
         ORDER BY path COLLATE NOCASE ASC",
    )?;
    let files = statement
        .query_map([], row_to_file_entry_for_analysis)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(AnalysisServiceError::from)?;
    Ok(files)
}

fn insert_restriction(
    restrictions: &mut BTreeMap<String, (u32, AndroidPackageRestriction)>,
    user_id: u32,
    restriction: AndroidPackageRestriction,
) {
    match restrictions.get(&restriction.package_name) {
        Some((existing_user, _)) if *existing_user <= user_id => {}
        _ => {
            restrictions.insert(restriction.package_name.clone(), (user_id, restriction));
        }
    }
}

fn user_id_from_path(path: &str) -> Option<u32> {
    let components = path.replace('\\', "/");
    let mut components = components.split('/');
    while let Some(component) = components.next() {
        if component.eq_ignore_ascii_case("users") {
            return components.next().and_then(|value| value.parse().ok());
        }
    }
    None
}

fn restriction_state(restriction: &AndroidPackageRestriction) -> Option<&'static str> {
    if restriction.installed == Some(false) {
        return Some("notInstalled");
    }
    if restriction.suspended == Some(true) {
        return Some("suspended");
    }
    if restriction.hidden == Some(true) {
        return Some("hidden");
    }
    if restriction.enabled_state.is_some_and(|value| value >= 2) {
        return Some("disabled");
    }
    if restriction.stopped == Some(true) {
        return Some("stopped");
    }
    (restriction.installed == Some(true)).then_some("installed")
}

fn apply_apk_presentation(
    package: &mut AndroidPackageDto,
    presentation: Result<
        artifacts_android::AndroidApkPresentation,
        artifacts_android::ApkInspectError,
    >,
) {
    match presentation {
        Ok(presentation) => {
            package.app_name = presentation.app_name;
            package.icon_data_url = presentation.icon.map(|icon| {
                format!(
                    "data:{};base64,{}",
                    icon.mime_type,
                    STANDARD.encode(icon.bytes)
                )
            });
        }
        Err(error) => append_package_warning(package, error.to_string()),
    }
}

fn append_package_warning(package: &mut AndroidPackageDto, warning: String) {
    package.warning = match package.warning.take() {
        Some(existing) => Some(format!("{existing}; {warning}")),
        None => Some(warning),
    };
}

fn find_apk_file(
    connection: &Connection,
    code_path: &str,
) -> Result<Option<FileEntry>, AnalysisServiceError> {
    let path = code_path.trim_end_matches('/');
    for variant in code_path_variants(path) {
        if variant.ends_with(".apk") {
            if let Some(apk) = find_exact_apk_file(connection, &variant)? {
                return Ok(Some(apk));
            }
            continue;
        }
        if let Some(base_apk) = find_exact_apk_file(connection, &format!("{variant}/base.apk"))? {
            return Ok(Some(base_apk));
        }
        if let Some(apk) = find_directory_apk_file(connection, &variant)? {
            return Ok(Some(apk));
        }
    }
    if let Some(leaf) = path.rsplit('/').next().filter(|leaf| !leaf.is_empty()) {
        if let Some(apk) = find_exact_apk_file(connection, &format!("{leaf}/base.apk"))? {
            return Ok(Some(apk));
        }
        if let Some(apk) = find_directory_apk_file(connection, leaf)? {
            return Ok(Some(apk));
        }
    }
    Ok(None)
}

fn code_path_variants(path: &str) -> Vec<String> {
    let normalized = path.trim_start_matches('/').to_ascii_lowercase();
    let mut variants = vec![normalized.clone()];
    if let Some(data_relative) = normalized.strip_prefix("data/") {
        variants.push(data_relative.to_string());
    }
    variants
}

fn find_exact_apk_file(
    connection: &Connection,
    path: &str,
) -> Result<Option<FileEntry>, AnalysisServiceError> {
    let suffix = escape_like_suffix(path.trim_start_matches('/'));
    connection
        .query_row(
            "SELECT id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted,
                    hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256, encrypted, read_only, archive, unix_mode
             FROM file_entries
             WHERE entry_type = 'file' COLLATE NOCASE
               AND REPLACE(LOWER(path), '\\', '/') LIKE ?1 ESCAPE '\\'
             ORDER BY LENGTH(path) ASC
             LIMIT 1",
            [format!("%{suffix}")],
            row_to_file_entry_for_analysis,
        )
        .optional()
        .map_err(Into::into)
}

fn find_directory_apk_file(
    connection: &Connection,
    directory: &str,
) -> Result<Option<FileEntry>, AnalysisServiceError> {
    let prefix = escape_like_suffix(directory.trim_start_matches('/'));
    connection
        .query_row(
            "SELECT id, parent_id, data_source_id, path, name, entry_type, size, ext, deleted,
                    hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256, encrypted, read_only, archive, unix_mode
             FROM file_entries
             WHERE entry_type = 'file' COLLATE NOCASE
               AND LOWER(name) LIKE '%.apk'
               AND REPLACE(LOWER(path), '\\', '/') LIKE ?1 ESCAPE '\\'
             ORDER BY LENGTH(path) ASC, path COLLATE NOCASE ASC
             LIMIT 1",
            [format!("%{prefix}/%")],
            row_to_file_entry_for_analysis,
        )
        .optional()
        .map_err(Into::into)
}

fn escape_like_suffix(path: &str) -> String {
    path.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
        .to_ascii_lowercase()
}
