use chrono::{TimeZone, Utc};

use super::{ParsedRegistryField, RegistryHiveReader, RegistryValue};

pub(crate) fn lookup_string_field(
    hive: &RegistryHiveReader<'_>,
    hive_path: &str,
    parser: &str,
    key_path: &[&str],
    value_name: &str,
    warnings: &mut Vec<String>,
) -> Option<ParsedRegistryField> {
    match hive.lookup_value(key_path, value_name) {
        Ok(Some(RegistryValue::String(value))) if !value.trim().is_empty() => {
            Some(ParsedRegistryField {
                value,
                hive_path: hive_path.to_string(),
                key_path: key_path.join("\\"),
                value_name: value_name.to_string(),
                parser: parser.to_string(),
            })
        }
        Ok(Some(RegistryValue::String(_))) => None,
        Ok(Some(other)) => {
            warnings.push(format!(
                "{}\\{} has unsupported type: {:?}",
                key_path.join("\\"),
                value_name,
                other
            ));
            None
        }
        Ok(None) => {
            warnings.push(format!("{}\\{} not found", key_path.join("\\"), value_name));
            None
        }
        Err(err) => {
            warnings.push(format!(
                "{}\\{} parse error: {}",
                key_path.join("\\"),
                value_name,
                err
            ));
            None
        }
    }
}

pub(crate) fn lookup_optional_string_field(
    hive: &RegistryHiveReader<'_>,
    hive_path: &str,
    parser: &str,
    key_path: &[&str],
    value_name: &str,
    warnings: &mut Vec<String>,
) -> Option<ParsedRegistryField> {
    match hive.lookup_value(key_path, value_name) {
        Ok(None) => None,
        _ => lookup_string_field(hive, hive_path, parser, key_path, value_name, warnings),
    }
}

pub(crate) fn lookup_install_date_field(
    hive: &RegistryHiveReader<'_>,
    hive_path: &str,
    parser: &str,
    key_path: &[&str],
    warnings: &mut Vec<String>,
) -> Option<ParsedRegistryField> {
    match hive.lookup_value(key_path, "InstallDate") {
        Ok(Some(RegistryValue::Dword(value))) => {
            let Some(dt) = Utc.timestamp_opt(value as i64, 0).single() else {
                warnings.push("InstallDate is outside supported timestamp range".to_string());
                return None;
            };
            if !(946_684_800..=4_102_444_800).contains(&value) {
                warnings.push(format!("InstallDate {value} is outside plausible range"));
                return None;
            }
            Some(ParsedRegistryField {
                value: dt.to_rfc3339(),
                hive_path: hive_path.to_string(),
                key_path: key_path.join("\\"),
                value_name: "InstallDate".to_string(),
                parser: parser.to_string(),
            })
        }
        Ok(Some(other)) => {
            warnings.push(format!("InstallDate has unsupported type: {:?}", other));
            None
        }
        Ok(None) => {
            warnings.push(format!("{}\\InstallDate not found", key_path.join("\\")));
            None
        }
        Err(err) => {
            warnings.push(format!(
                "{}\\InstallDate parse error: {}",
                key_path.join("\\"),
                err
            ));
            None
        }
    }
}
