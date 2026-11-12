use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::extraction::artifact_query::query_artifact_rows;
use crate::analysis_service::extraction::attr_mapping::{
    optional_bool_attr, optional_string_attr, optional_u32_attr, optional_u64_attr, string_attr,
};
use rusqlite::Connection;
use transport::dto::{
    AmcacheApplicationDto, AmcacheApplicationFileDto, AppCompatLayerDto, LastVisitedMruEntryDto,
    MuiCacheEntryDto, OpenSaveMruEntryDto, RunMruEntryDto, ShellbagEntryDto,
};

pub(super) struct ActivityRegistryData {
    pub(super) open_save_mru: Vec<OpenSaveMruEntryDto>,
    pub(super) last_visited_mru: Vec<LastVisitedMruEntryDto>,
    pub(super) run_mru: Vec<RunMruEntryDto>,
    pub(super) shellbag_entries: Vec<ShellbagEntryDto>,
    pub(super) muicache_entries: Vec<MuiCacheEntryDto>,
    pub(super) amcache_applications: Vec<AmcacheApplicationDto>,
    pub(super) amcache_application_files: Vec<AmcacheApplicationFileDto>,
    pub(super) appcompat_layers: Vec<AppCompatLayerDto>,
}

impl ActivityRegistryData {
    pub(super) fn load(conn: &Connection) -> Result<Self, AnalysisServiceError> {
        let open_save_mru = load_open_save_mru(conn)?;
        let last_visited_mru = load_last_visited_mru(conn)?;
        let run_mru = load_run_mru(conn)?;
        let shellbag_entries = load_shellbag_entries(conn)?;
        let muicache_entries = load_muicache_entries(conn)?;
        let amcache_applications = load_amcache_applications(conn)?;
        let amcache_application_files = load_amcache_application_files(conn)?;
        let appcompat_layers = load_appcompat_layers(conn)?;
        Ok(Self {
            open_save_mru,
            last_visited_mru,
            run_mru,
            shellbag_entries,
            muicache_entries,
            amcache_applications,
            amcache_application_files,
            appcompat_layers,
        })
    }
}

fn load_open_save_mru(conn: &Connection) -> Result<Vec<OpenSaveMruEntryDto>, AnalysisServiceError> {
    Ok(
        query_artifact_rows(conn, &["RegistryOpenSaveMru"], 0, 10_000)?
            .into_iter()
            .map(|row| OpenSaveMruEntryDto {
                extension: string_attr(&row.attrs, "extension"),
                value_name: string_attr(&row.attrs, "valueName"),
                file_name: string_attr(&row.attrs, "fileName"),
                raw_pidl_hex: string_attr(&row.attrs, "rawPidlHex"),
                source_key_path: string_attr(&row.attrs, "sourceKeyPath"),
                last_write: optional_string_attr(&row.attrs, "lastWrite"),
            })
            .collect(),
    )
}

fn load_last_visited_mru(
    conn: &Connection,
) -> Result<Vec<LastVisitedMruEntryDto>, AnalysisServiceError> {
    Ok(
        query_artifact_rows(conn, &["RegistryLastVisitedMru"], 0, 10_000)?
            .into_iter()
            .map(|row| LastVisitedMruEntryDto {
                value_name: string_attr(&row.attrs, "valueName"),
                path: string_attr(&row.attrs, "path"),
                raw_pidl_hex: string_attr(&row.attrs, "rawPidlHex"),
                source_key_path: string_attr(&row.attrs, "sourceKeyPath"),
                last_write: optional_string_attr(&row.attrs, "lastWrite"),
            })
            .collect(),
    )
}

fn load_run_mru(conn: &Connection) -> Result<Vec<RunMruEntryDto>, AnalysisServiceError> {
    Ok(query_artifact_rows(conn, &["RegistryRunMru"], 0, 10_000)?
        .into_iter()
        .map(|row| RunMruEntryDto {
            value_name: string_attr(&row.attrs, "valueName"),
            command: string_attr(&row.attrs, "command"),
            source_key_path: string_attr(&row.attrs, "sourceKeyPath"),
            last_write: optional_string_attr(&row.attrs, "lastWrite"),
        })
        .collect())
}

fn load_shellbag_entries(conn: &Connection) -> Result<Vec<ShellbagEntryDto>, AnalysisServiceError> {
    Ok(query_artifact_rows(conn, &["RegistryShellbag"], 0, 10_000)?
        .into_iter()
        .map(|row| ShellbagEntryDto {
            path: string_attr(&row.attrs, "path"),
            raw_pidl_hex: string_attr(&row.attrs, "rawPidlHex"),
            node_slot: optional_u32_attr(&row.attrs, "nodeSlot"),
            source_key_path: string_attr(&row.attrs, "sourceKeyPath"),
            last_write: optional_string_attr(&row.attrs, "lastWrite"),
        })
        .collect())
}

fn load_muicache_entries(conn: &Connection) -> Result<Vec<MuiCacheEntryDto>, AnalysisServiceError> {
    Ok(query_artifact_rows(conn, &["RegistryMuiCache"], 0, 10_000)?
        .into_iter()
        .map(|row| MuiCacheEntryDto {
            program_path: string_attr(&row.attrs, "programPath"),
            friendly_name: string_attr(&row.attrs, "friendlyName"),
            source_key_path: string_attr(&row.attrs, "sourceKeyPath"),
            last_write: optional_string_attr(&row.attrs, "lastWrite"),
        })
        .collect())
}

fn load_amcache_applications(
    conn: &Connection,
) -> Result<Vec<AmcacheApplicationDto>, AnalysisServiceError> {
    Ok(
        query_artifact_rows(conn, &["RegistryAmcacheApplication"], 0, 10_000)?
            .into_iter()
            .map(|row| AmcacheApplicationDto {
                program_id: optional_string_attr(&row.attrs, "programId"),
                name: optional_string_attr(&row.attrs, "name"),
                version: optional_string_attr(&row.attrs, "version"),
                publisher: optional_string_attr(&row.attrs, "publisher"),
                install_date: optional_string_attr(&row.attrs, "installDate"),
                source: optional_string_attr(&row.attrs, "source"),
                os_version_at_install_time: optional_string_attr(
                    &row.attrs,
                    "osVersionAtInstallTime",
                ),
                registry_key_path: string_attr(&row.attrs, "registryKeyPath"),
            })
            .collect(),
    )
}

fn load_amcache_application_files(
    conn: &Connection,
) -> Result<Vec<AmcacheApplicationFileDto>, AnalysisServiceError> {
    Ok(
        query_artifact_rows(conn, &["RegistryAmcacheApplicationFile"], 0, 10_000)?
            .into_iter()
            .map(|row| AmcacheApplicationFileDto {
                program_id: optional_string_attr(&row.attrs, "programId"),
                lower_case_long_path: optional_string_attr(&row.attrs, "lowerCaseLongPath"),
                long_path_hash: optional_string_attr(&row.attrs, "longPathHash"),
                file_size: optional_u64_attr(&row.attrs, "fileSize"),
                product_name: optional_string_attr(&row.attrs, "productName"),
                company_name: optional_string_attr(&row.attrs, "companyName"),
                file_version: optional_string_attr(&row.attrs, "fileVersion"),
                is_pe_file: optional_bool_attr(&row.attrs, "isPeFile"),
                link_date: optional_string_attr(&row.attrs, "linkDate"),
                registry_key_path: string_attr(&row.attrs, "registryKeyPath"),
            })
            .collect(),
    )
}

fn load_appcompat_layers(
    conn: &Connection,
) -> Result<Vec<AppCompatLayerDto>, AnalysisServiceError> {
    Ok(
        query_artifact_rows(conn, &["RegistryAppCompatLayer"], 0, 10_000)?
            .into_iter()
            .map(|row| AppCompatLayerDto {
                executable_path: string_attr(&row.attrs, "executablePath"),
                layer_string: string_attr(&row.attrs, "layerString"),
                source_hive_path: string_attr(&row.attrs, "sourceHivePath"),
                source_key_path: string_attr(&row.attrs, "sourceKeyPath"),
                last_write: optional_string_attr(&row.attrs, "lastWrite"),
            })
            .collect(),
    )
}
