use crate::analysis_service::candidates::find_candidate_by_path_suffix;
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::provenance::{
    entry_provenance, registry_field_provenance, EVTX_BOOT_SHUTDOWN_PARSER,
    REGISTRY_SOFTWARE_PARSER, REGISTRY_SYSTEM_PARSER,
};
use crate::analysis_service::MAX_REGISTRY_ANALYSIS_BYTES;
use domain::{FileEntry, FileEntryId};
use rusqlite::Connection;
use transport::dto::analysis::{
    AnalysisBootRecordDto, AnalysisFieldProvenanceDto, AnalysisNetworkAdapterDto,
    AnalysisProvenanceDto,
};
use transport::dto::{AnalysisParseStatusDto, AnalysisSystemInfoDto};

#[derive(Default)]
struct SystemInfoExtraction {
    computer_name: Option<String>,
    os_version: Option<String>,
    build_number: Option<String>,
    install_date: Option<String>,
    registered_owner: Option<String>,
    organization: Option<String>,
    product_id: Option<String>,
    timezone: Option<String>,
    network_adapters: Vec<AnalysisNetworkAdapterDto>,
    boot_history: Vec<AnalysisBootRecordDto>,
    field_provenance: Vec<AnalysisFieldProvenanceDto>,
}

struct AnalysisProvenanceContext<'a> {
    parsed_at: &'a str,
    data_source_id: Option<&'a str>,
}

impl SystemInfoExtraction {
    fn has_registry_field(&self) -> bool {
        self.computer_name.is_some()
            || self.os_version.is_some()
            || self.build_number.is_some()
            || self.install_date.is_some()
            || self.registered_owner.is_some()
            || self.organization.is_some()
            || self.product_id.is_some()
            || self.timezone.is_some()
            || !self.network_adapters.is_empty()
    }
}

/// Extracts bounded system analysis facts from evidence-backed Registry hives.
///
/// EVTX boot/shutdown records are reported as EventLog/User32 candidates, not as
/// direct boot assertions. This service never manufactures host facts from file
/// presence alone.
pub fn extract_system_info_for_case<E: std::fmt::Display>(
    conn: &Connection,
    mut read_header_fn: impl FnMut(&FileEntryId, usize) -> Result<Vec<u8>, E>,
) -> AnalysisSystemInfoDto {
    let parsed_at = chrono::Utc::now().to_rfc3339();
    let mut warnings = Vec::new();
    let mut provenance = Vec::new();
    let mut extraction = SystemInfoExtraction::default();
    let data_source_id = find_unique_data_source_id(conn);
    let provenance_context = AnalysisProvenanceContext {
        parsed_at: &parsed_at,
        data_source_id: data_source_id.as_deref(),
    };

    match find_system_info_candidates(conn) {
        Ok(candidates) => {
            let system_hive = candidates.system_hive.as_ref();
            let software_hive = candidates.software_hive.as_ref();
            let system_evtx = candidates.system_evtx.as_ref();

            inspect_registry_hive(
                system_hive,
                REGISTRY_SYSTEM_PARSER,
                &provenance_context,
                &mut read_header_fn,
                &mut warnings,
                &mut provenance,
                &mut extraction,
            );
            inspect_registry_hive(
                software_hive,
                REGISTRY_SOFTWARE_PARSER,
                &provenance_context,
                &mut read_header_fn,
                &mut warnings,
                &mut provenance,
                &mut extraction,
            );
            inspect_evtx_boot_source(
                system_evtx,
                &provenance_context,
                &mut read_header_fn,
                &mut warnings,
                &mut provenance,
                &mut extraction.boot_history,
            );
        }
        Err(err) => {
            let warning = format!("无法枚举文件目录以发现 Registry/EVTX: {}", err);
            warnings.push(warning);
        }
    }

    let status = if extraction.has_registry_field() {
        AnalysisParseStatusDto::Parsed
    } else {
        AnalysisParseStatusDto::NotParsed
    };
    AnalysisSystemInfoDto {
        computer_name: extraction.computer_name,
        os_version: extraction.os_version,
        build_number: extraction.build_number,
        install_date: extraction.install_date,
        registered_owner: extraction.registered_owner,
        organization: extraction.organization,
        product_id: extraction.product_id,
        network_adapters: extraction.network_adapters,
        boot_history: extraction.boot_history,
        timezone: extraction.timezone,
        language: None,
        status,
        warnings,
        provenance,
        field_provenance: extraction.field_provenance,
    }
}

#[derive(Default)]
struct SystemInfoCandidates {
    system_hive: Option<FileEntry>,
    software_hive: Option<FileEntry>,
    system_evtx: Option<FileEntry>,
}

fn find_system_info_candidates(
    conn: &Connection,
) -> Result<SystemInfoCandidates, AnalysisServiceError> {
    Ok(SystemInfoCandidates {
        system_hive: find_candidate_by_path_suffix(conn, "windows/system32/config/system")?,
        software_hive: find_candidate_by_path_suffix(conn, "windows/system32/config/software")?,
        system_evtx: find_candidate_by_path_suffix(
            conn,
            "windows/system32/winevt/logs/system.evtx",
        )?,
    })
}

fn find_unique_data_source_id(conn: &Connection) -> Option<String> {
    unique_data_source_id_from_files(conn).or_else(|| unique_registered_data_source_id(conn))
}

fn unique_data_source_id_from_files(conn: &Connection) -> Option<String> {
    conn.query_row(
        "SELECT CASE WHEN COUNT(DISTINCT data_source_id) = 1 THEN MIN(data_source_id) END
         FROM file_entries
         WHERE data_source_id <> ''",
        [],
        |row| row.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
}

fn unique_registered_data_source_id(conn: &Connection) -> Option<String> {
    conn.query_row(
        "SELECT CASE WHEN COUNT(DISTINCT id) = 1 THEN MIN(id) END FROM data_sources",
        [],
        |row| row.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
}

fn inspect_registry_hive<E: std::fmt::Display>(
    entry: Option<&FileEntry>,
    parser: &str,
    context: &AnalysisProvenanceContext<'_>,
    read_header_fn: &mut impl FnMut(&FileEntryId, usize) -> Result<Vec<u8>, E>,
    warnings: &mut Vec<String>,
    provenance: &mut Vec<AnalysisProvenanceDto>,
    extraction: &mut SystemInfoExtraction,
) {
    let Some(entry) = entry else {
        record_missing_registry_hive(parser, context, warnings, provenance);
        return;
    };

    let (parsed_any, parser_warnings) = match read_header_fn(&entry.id, MAX_REGISTRY_ANALYSIS_BYTES)
    {
        Ok(bytes) => inspect_registry_bytes(&bytes, entry, parser, extraction),
        Err(err) => (false, vec![format!("{} 读取失败: {}", entry.path, err)]),
    };
    warnings.extend(parser_warnings.iter().cloned());
    provenance.push(entry_provenance(
        entry,
        parser,
        context.parsed_at,
        if parsed_any {
            AnalysisParseStatusDto::Parsed
        } else {
            AnalysisParseStatusDto::NotParsed
        },
        parser_warnings,
    ));
}

fn inspect_registry_bytes(
    bytes: &[u8],
    entry: &FileEntry,
    parser: &str,
    extraction: &mut SystemInfoExtraction,
) -> (bool, Vec<String>) {
    if !bytes.starts_with(b"regf") {
        let warning = if bytes.len() >= MAX_REGISTRY_ANALYSIS_BYTES {
            format!(
                "{} 达到 Registry parser 读取上限 {} bytes，且未取得有效 regf 头。",
                entry.path, MAX_REGISTRY_ANALYSIS_BYTES
            )
        } else {
            format!("{} 不含 regf 头，无法作为 Registry hive 解析。", entry.path)
        };
        return (false, vec![warning]);
    }

    match parser {
        REGISTRY_SYSTEM_PARSER => inspect_system_registry_fields(bytes, entry, extraction),
        REGISTRY_SOFTWARE_PARSER => inspect_software_registry_fields(bytes, entry, extraction),
        _ => (false, vec![format!("{} parser unsupported", parser)]),
    }
}

fn inspect_system_registry_fields(
    bytes: &[u8],
    entry: &FileEntry,
    extraction: &mut SystemInfoExtraction,
) -> (bool, Vec<String>) {
    let info = match artifacts_windows::extract_system_hive_fields(bytes, &entry.path) {
        Ok(info) => info,
        Err(err) => return (false, vec![format!("{} 解析失败: {}", entry.path, err)]),
    };
    let mut parsed_any = assign_registry_field(
        "computerName",
        info.computer_name,
        &mut extraction.computer_name,
        &mut extraction.field_provenance,
    );
    parsed_any |= assign_registry_field(
        "timezone",
        info.timezone,
        &mut extraction.timezone,
        &mut extraction.field_provenance,
    );
    match artifacts_windows::extract_network_adapters_from_system_hive(bytes, &entry.path) {
        Ok(adapters) => {
            parsed_any |= !adapters.is_empty();
            extraction.network_adapters = adapters
                .into_iter()
                .map(|adapter| AnalysisNetworkAdapterDto {
                    name: adapter.name.unwrap_or(adapter.guid),
                    mac_address: adapter.mac_address,
                    ip_addresses: adapter.ip_addresses,
                    dhcp_enabled: adapter.dhcp_enabled,
                    dhcp_server: adapter.dhcp_server,
                })
                .collect();
            (parsed_any, info.warnings)
        }
        Err(error) => {
            let mut warnings = info.warnings;
            warnings.push(format!(
                "{} network adapter parsing failed: {error}",
                entry.path
            ));
            (parsed_any, warnings)
        }
    }
}

fn inspect_software_registry_fields(
    bytes: &[u8],
    entry: &FileEntry,
    extraction: &mut SystemInfoExtraction,
) -> (bool, Vec<String>) {
    let info = match artifacts_windows::extract_software_hive_fields(bytes, &entry.path) {
        Ok(info) => info,
        Err(err) => return (false, vec![format!("{} 解析失败: {}", entry.path, err)]),
    };
    let mut parsed_any = assign_registry_field(
        "osVersion",
        info.product_name,
        &mut extraction.os_version,
        &mut extraction.field_provenance,
    );
    parsed_any |= assign_registry_field(
        "buildNumber",
        info.current_build,
        &mut extraction.build_number,
        &mut extraction.field_provenance,
    );
    parsed_any |= assign_registry_field(
        "installDate",
        info.install_date,
        &mut extraction.install_date,
        &mut extraction.field_provenance,
    );
    parsed_any |= assign_registry_field(
        "registeredOwner",
        info.registered_owner,
        &mut extraction.registered_owner,
        &mut extraction.field_provenance,
    );
    parsed_any |= assign_registry_field(
        "organization",
        info.registered_organization,
        &mut extraction.organization,
        &mut extraction.field_provenance,
    );
    parsed_any |= assign_registry_field(
        "productId",
        info.product_id,
        &mut extraction.product_id,
        &mut extraction.field_provenance,
    );
    parsed_any |= assign_display_version(info.display_version, extraction);
    if let Some(current_version) = info.current_version {
        extraction.field_provenance.push(registry_field_provenance(
            "osCurrentVersion",
            current_version,
        ));
        parsed_any = true;
    }
    (parsed_any, info.warnings)
}

fn assign_display_version(
    display_version: Option<artifacts_windows::ParsedRegistryField>,
    extraction: &mut SystemInfoExtraction,
) -> bool {
    let Some(display_version) = display_version else {
        return false;
    };
    let value = display_version.value.clone();
    extraction.field_provenance.push(registry_field_provenance(
        "osDisplayVersion",
        display_version,
    ));
    match &mut extraction.os_version {
        Some(os) if !os.contains(&value) => {
            os.push(' ');
            os.push_str(&value);
        }
        None => extraction.os_version = Some(value),
        _ => {}
    }
    true
}

fn record_missing_registry_hive(
    parser: &str,
    context: &AnalysisProvenanceContext<'_>,
    warnings: &mut Vec<String>,
    provenance: &mut Vec<AnalysisProvenanceDto>,
) {
    let artifact_path = match parser {
        REGISTRY_SYSTEM_PARSER => "Windows/System32/config/SYSTEM",
        REGISTRY_SOFTWARE_PARSER => "Windows/System32/config/SOFTWARE",
        _ => "Windows/System32/config",
    };
    let warning = format!("未在证据文件目录中发现 {}。", artifact_path);
    warnings.push(warning.clone());
    push_unavailable_provenance(
        provenance,
        context.data_source_id,
        artifact_path,
        parser,
        context.parsed_at,
        warning,
    );
}

fn push_unavailable_provenance(
    provenance: &mut Vec<AnalysisProvenanceDto>,
    data_source_id: Option<&str>,
    artifact_path: &str,
    parser: &str,
    parsed_at: &str,
    warning: String,
) {
    let Some(data_source_id) = data_source_id else {
        return;
    };
    provenance.push(AnalysisProvenanceDto {
        data_source_id: data_source_id.to_string(),
        artifact_path: artifact_path.to_string(),
        parser: parser.to_string(),
        parsed_at: parsed_at.to_string(),
        status: AnalysisParseStatusDto::Unavailable,
        warnings: vec![warning],
    });
}

fn assign_registry_field(
    field: &str,
    parsed: Option<artifacts_windows::ParsedRegistryField>,
    target: &mut Option<String>,
    field_provenance: &mut Vec<AnalysisFieldProvenanceDto>,
) -> bool {
    let Some(parsed) = parsed else {
        return false;
    };
    *target = Some(parsed.value.clone());
    field_provenance.push(registry_field_provenance(field, parsed));
    true
}

fn inspect_evtx_boot_source<E: std::fmt::Display>(
    entry: Option<&FileEntry>,
    context: &AnalysisProvenanceContext<'_>,
    read_header_fn: &mut impl FnMut(&FileEntryId, usize) -> Result<Vec<u8>, E>,
    warnings: &mut Vec<String>,
    provenance: &mut Vec<AnalysisProvenanceDto>,
    boot_history: &mut Vec<AnalysisBootRecordDto>,
) {
    match entry {
        Some(entry) => {
            let mut parser_warnings = Vec::new();
            let mut parsed_any = false;
            match read_header_fn(&entry.id, artifacts_windows::MAX_EVTX_ANALYSIS_BYTES) {
                Ok(bytes) => {
                    match artifacts_windows::extract_boot_shutdown_events(&bytes, &entry.path) {
                        Ok(extraction) => {
                            parser_warnings.extend(extraction.warnings);
                            if !extraction.events.is_empty() {
                                parsed_any = true;
                            }
                            let event_provenance = entry_provenance(
                                entry,
                                EVTX_BOOT_SHUTDOWN_PARSER,
                                context.parsed_at,
                                AnalysisParseStatusDto::Parsed,
                                Vec::new(),
                            );
                            boot_history.extend(extraction.events.into_iter().map(|event| {
                                AnalysisBootRecordDto {
                                    timestamp: event.timestamp,
                                    boot_type: event.kind.as_str().to_string(),
                                    source: event.source_path,
                                    event_id: Some(event.event_id),
                                    record_id: event.record_id,
                                    note: Some(event.note),
                                    details: event.details,
                                    provenance: event_provenance.clone(),
                                }
                            }));
                        }
                        Err(err) => parser_warnings.push(err.to_string()),
                    }
                }
                Err(err) => parser_warnings.push(format!("{} 读取失败: {}", entry.path, err)),
            }
            provenance.push(entry_provenance(
                entry,
                EVTX_BOOT_SHUTDOWN_PARSER,
                context.parsed_at,
                if parsed_any {
                    AnalysisParseStatusDto::Parsed
                } else {
                    AnalysisParseStatusDto::NotParsed
                },
                parser_warnings,
            ));
        }
        None => {
            let artifact_path = "Windows/System32/winevt/Logs/System.evtx";
            let warning = format!(
                "未在证据文件目录中发现 {}；EVTX boot/shutdown 解析不可用。",
                artifact_path
            );
            warnings.push(warning.clone());
            push_unavailable_provenance(
                provenance,
                context.data_source_id,
                artifact_path,
                EVTX_BOOT_SHUTDOWN_PARSER,
                context.parsed_at,
                warning,
            );
        }
    }
}
