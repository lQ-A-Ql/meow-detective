use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::extraction::artifact_query::query_artifact_rows;
use crate::analysis_service::extraction::attr_mapping::{
    bool_attr, optional_string_attr, optional_u32_attr, string_attr, string_vec_attr, u64_attr,
};
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use transport::dto::{
    AnalysisParseStatusDto, InstalledSoftwareDto, NetworkProfileDto, RegistryHiveOverviewDto,
    SamUserAccountDto, UserAssistEntryDto,
};

pub(super) struct CoreRegistryData {
    pub(super) sam_users: Vec<SamUserAccountDto>,
    pub(super) user_assist_entries: Vec<UserAssistEntryDto>,
    pub(super) hive_overviews: Vec<RegistryHiveOverviewDto>,
    pub(super) network_profiles: Vec<NetworkProfileDto>,
    pub(super) installed_software: Vec<InstalledSoftwareDto>,
}

impl CoreRegistryData {
    pub(super) fn load(conn: &Connection) -> Result<Self, AnalysisServiceError> {
        let sam_users = load_sam_users(conn)?;
        let user_assist_entries = load_user_assist_entries(conn)?;
        let hive_overviews = load_hive_overviews(conn)?;
        let network_profiles = load_network_profiles(conn)?;
        let installed_software = load_installed_software(conn)?;
        Ok(Self {
            sam_users,
            user_assist_entries,
            hive_overviews,
            network_profiles,
            installed_software,
        })
    }
}

fn load_sam_users(conn: &Connection) -> Result<Vec<SamUserAccountDto>, AnalysisServiceError> {
    Ok(query_artifact_rows(conn, &["RegistrySamUser"], 0, 10_000)?
        .into_iter()
        .map(|row| SamUserAccountDto {
            username: string_attr(&row.attrs, "username"),
            rid: u64_attr(&row.attrs, "rid") as u32,
            rid_hex: string_attr(&row.attrs, "ridHex"),
            sid: string_attr(&row.attrs, "sid"),
            groups: string_vec_attr(&row.attrs, "groups"),
            login_count: u64_attr(&row.attrs, "loginCount") as u32,
            last_login: optional_string_attr(&row.attrs, "lastLogin"),
            account_created: optional_string_attr(&row.attrs, "accountCreated"),
            account_status: string_attr(&row.attrs, "accountStatus"),
            profile_path: optional_string_attr(&row.attrs, "profilePath"),
            password_hash: optional_string_attr(&row.attrs, "passwordHash"),
            password_hash_type: optional_string_attr(&row.attrs, "passwordHashType"),
            password_hint: None,
            data_source_id: string_attr(&row.attrs, "dataSourceId"),
            hive_path: string_attr(&row.attrs, "hivePath"),
            key_path: string_attr(&row.attrs, "keyPath"),
            parser: string_attr(&row.attrs, "parser"),
        })
        .collect())
}

fn load_user_assist_entries(
    conn: &Connection,
) -> Result<Vec<UserAssistEntryDto>, AnalysisServiceError> {
    Ok(
        query_artifact_rows(conn, &["RegistryUserAssist"], 0, 10_000)?
            .into_iter()
            .map(|row| UserAssistEntryDto {
                program_path: string_attr(&row.attrs, "programPath"),
                exec_count: u64_attr(&row.attrs, "execCount") as u32,
                last_exec_time: optional_string_attr(&row.attrs, "lastExecTime"),
                is_suspicious: None,
                suspicious_reason: None,
            })
            .collect(),
    )
}

fn load_hive_overviews(
    conn: &Connection,
) -> Result<Vec<RegistryHiveOverviewDto>, AnalysisServiceError> {
    let rows = query_artifact_rows(conn, &["RegistryHive", "RegistryValue"], 0, 10_000)?;
    let mut artifact_counts: HashMap<String, u64> = HashMap::new();
    for row in &rows {
        let hive_name = hive_name(&row.attrs);
        if !hive_name.is_empty() {
            *artifact_counts.entry(hive_name).or_insert(0) += 1;
        }
    }
    let mut overviews = Vec::new();
    let mut seen_hives = HashSet::new();
    for row in rows {
        let hive_name = hive_name(&row.attrs);
        if hive_name.is_empty() || !seen_hives.insert(hive_name.clone()) {
            continue;
        }
        let status = if row.source_object_id.is_some() {
            AnalysisParseStatusDto::Parsed
        } else {
            AnalysisParseStatusDto::Partial
        };
        overviews.push(RegistryHiveOverviewDto {
            key_value_count: artifact_counts.get(&hive_name).copied().unwrap_or(0),
            hive_name,
            status,
            extracted_at: row.created_at.clone(),
            data_source_id: string_attr(&row.attrs, "dataSourceId"),
            source_path: string_attr(&row.attrs, "sourcePath"),
            txlog_merged: bool_attr(&row.attrs, "txlogMerged"),
            deleted_keys_found: u64_attr(&row.attrs, "deletedKeysFound") as u32,
        });
    }
    Ok(overviews)
}

fn hive_name(attrs: &std::collections::BTreeMap<String, serde_json::Value>) -> String {
    string_attr(attrs, "hivePath")
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string()
}

fn load_network_profiles(
    conn: &Connection,
) -> Result<Vec<NetworkProfileDto>, AnalysisServiceError> {
    Ok(
        query_artifact_rows(conn, &["RegistryNetworkProfile"], 0, 10_000)?
            .into_iter()
            .map(|row| NetworkProfileDto {
                profile_guid: string_attr(&row.attrs, "profileGuid"),
                profile_name: string_attr(&row.attrs, "profileName"),
                description: optional_string_attr(&row.attrs, "description"),
                date_created: optional_string_attr(&row.attrs, "dateCreated"),
                date_last_connected: optional_string_attr(&row.attrs, "dateLastConnected"),
                name_type: optional_u32_attr(&row.attrs, "nameType"),
                managed: bool_attr(&row.attrs, "managed"),
                first_network: optional_string_attr(&row.attrs, "firstNetwork"),
                default_gateway_mac_hex: optional_string_attr(&row.attrs, "defaultGatewayMacHex"),
                dns_suffix: optional_string_attr(&row.attrs, "dnsSuffix"),
                source_key_path: string_attr(&row.attrs, "sourceKeyPath"),
            })
            .collect(),
    )
}

fn load_installed_software(
    conn: &Connection,
) -> Result<Vec<InstalledSoftwareDto>, AnalysisServiceError> {
    Ok(
        query_artifact_rows(conn, &["RegistryInstalledSoftware"], 0, 10_000)?
            .into_iter()
            .map(|row| InstalledSoftwareDto {
                display_name: string_attr(&row.attrs, "displayName"),
                version: string_attr(&row.attrs, "version"),
                publisher: optional_string_attr(&row.attrs, "publisher"),
                install_date: optional_string_attr(&row.attrs, "installDate"),
                estimated_size: optional_string_attr(&row.attrs, "estimatedSize"),
                is_suspicious: None,
            })
            .collect(),
    )
}
