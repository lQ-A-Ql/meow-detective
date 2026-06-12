use crate::analysis_service::candidates::find_candidate_by_path_suffix;
use crate::analysis_service::provenance::{
    entry_provenance, registry_field_provenance, unknown_provenance, EVTX_BOOT_SHUTDOWN_PARSER,
    REGISTRY_SOFTWARE_PARSER, REGISTRY_SYSTEM_PARSER,
};
use crate::analysis_service::MAX_REGISTRY_ANALYSIS_BYTES;
use domain::{FileEntry, FileEntryId};
use rusqlite::Connection;
use transport::dto::analysis::{
    AnalysisBootRecordDto, AnalysisFieldProvenanceDto, AnalysisProvenanceDto,
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
    boot_history: Vec<AnalysisBootRecordDto>,
    field_provenance: Vec<AnalysisFieldProvenanceDto>,
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
    }
}

/// Extracts bounded system analysis facts from evidence-backed Registry hives.
///
/// EVTX boot/shutdown records are reported as EventLog/User32 candidates, not as
/// direct boot assertions. This service never manufactures host facts from file
/// presence alone.
pub fn extract_system_info_for_case(
    conn: &Connection,
    mut read_header_fn: impl FnMut(&FileEntryId, usize) -> Result<Vec<u8>, String>,
) -> AnalysisSystemInfoDto {
    let parsed_at = chrono::Utc::now().to_rfc3339();
    let mut warnings = Vec::new();
    let mut provenance = Vec::new();
    let mut extraction = SystemInfoExtraction::default();

    match find_system_info_candidates(conn) {
        Ok(candidates) => {
            let system_hive = candidates.system_hive.as_ref();
            let software_hive = candidates.software_hive.as_ref();
            let system_evtx = candidates.system_evtx.as_ref();

            inspect_registry_hive(
                system_hive,
                REGISTRY_SYSTEM_PARSER,
                &parsed_at,
                &mut read_header_fn,
                &mut warnings,
                &mut provenance,
                &mut extraction,
            );
            inspect_registry_hive(
                software_hive,
                REGISTRY_SOFTWARE_PARSER,
                &parsed_at,
                &mut read_header_fn,
                &mut warnings,
                &mut provenance,
                &mut extraction,
            );
            inspect_evtx_boot_source(
                system_evtx,
                &parsed_at,
                &mut read_header_fn,
                &mut warnings,
                &mut provenance,
                &mut extraction.boot_history,
            );
        }
        Err(err) => {
            let warning = format!("无法枚举文件目录以发现 Registry/EVTX: {}", err);
            warnings.push(warning.clone());
            provenance.push(unknown_provenance(
                REGISTRY_SYSTEM_PARSER,
                &parsed_at,
                AnalysisParseStatusDto::Unavailable,
                vec![warning],
            ));
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
        network_adapters: Vec::new(),
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

fn find_system_info_candidates(conn: &Connection) -> Result<SystemInfoCandidates, String> {
    Ok(SystemInfoCandidates {
        system_hive: find_candidate_by_path_suffix(conn, "windows/system32/config/system")?,
        software_hive: find_candidate_by_path_suffix(conn, "windows/system32/config/software")?,
        system_evtx: find_candidate_by_path_suffix(
            conn,
            "windows/system32/winevt/logs/system.evtx",
        )?,
    })
}

fn inspect_registry_hive(
    entry: Option<&FileEntry>,
    parser: &str,
    parsed_at: &str,
    read_header_fn: &mut impl FnMut(&FileEntryId, usize) -> Result<Vec<u8>, String>,
    warnings: &mut Vec<String>,
    provenance: &mut Vec<AnalysisProvenanceDto>,
    extraction: &mut SystemInfoExtraction,
) {
    match entry {
        Some(entry) => {
            let read_result = read_header_fn(&entry.id, MAX_REGISTRY_ANALYSIS_BYTES);
            let mut parser_warnings = Vec::new();
            let mut parsed_any = false;
            match read_result {
                Ok(bytes) if bytes.starts_with(b"regf") => match parser {
                    REGISTRY_SYSTEM_PARSER => {
                        match artifacts_windows::extract_system_hive_fields(&bytes, &entry.path) {
                            Ok(info) => {
                                parsed_any |= assign_registry_field(
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
                                parser_warnings.extend(info.warnings);
                            }
                            Err(err) => {
                                parser_warnings.push(format!("{} 解析失败: {}", entry.path, err))
                            }
                        }
                    }
                    REGISTRY_SOFTWARE_PARSER => {
                        match artifacts_windows::extract_software_hive_fields(&bytes, &entry.path) {
                            Ok(info) => {
                                parsed_any |= assign_registry_field(
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
                                if let Some(display_version) = info.display_version {
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
                                    parsed_any = true;
                                }
                                if let Some(current_version) = info.current_version {
                                    extraction.field_provenance.push(registry_field_provenance(
                                        "osCurrentVersion",
                                        current_version,
                                    ));
                                    parsed_any = true;
                                }
                                parser_warnings.extend(info.warnings);
                            }
                            Err(err) => {
                                parser_warnings.push(format!("{} 解析失败: {}", entry.path, err))
                            }
                        }
                    }
                    _ => parser_warnings.push(format!("{} parser unsupported", parser)),
                },
                Ok(bytes) if bytes.len() >= MAX_REGISTRY_ANALYSIS_BYTES => {
                    parser_warnings.push(format!(
                        "{} 达到 Registry parser 读取上限 {} bytes，且未取得有效 regf 头。",
                        entry.path, MAX_REGISTRY_ANALYSIS_BYTES
                    ));
                }
                Ok(_) => {
                    parser_warnings.push(format!(
                        "{} 不含 regf 头，无法作为 Registry hive 解析。",
                        entry.path
                    ));
                }
                Err(err) => {
                    parser_warnings.push(format!("{} 读取失败: {}", entry.path, err));
                }
            }
            warnings.extend(parser_warnings.clone());
            provenance.push(entry_provenance(
                entry,
                parser,
                parsed_at,
                if parsed_any {
                    AnalysisParseStatusDto::Parsed
                } else {
                    AnalysisParseStatusDto::NotParsed
                },
                parser_warnings,
            ));
        }
        None => {
            let artifact_path = match parser {
                REGISTRY_SYSTEM_PARSER => "Windows/System32/config/SYSTEM",
                REGISTRY_SOFTWARE_PARSER => "Windows/System32/config/SOFTWARE",
                _ => "Windows/System32/config",
            };
            let warning = format!("未在证据文件目录中发现 {}。", artifact_path);
            warnings.push(warning.clone());
            provenance.push(AnalysisProvenanceDto {
                data_source_id: String::new(),
                artifact_path: artifact_path.to_string(),
                parser: parser.to_string(),
                parsed_at: parsed_at.to_string(),
                status: AnalysisParseStatusDto::Unavailable,
                warnings: vec![warning],
            });
        }
    }
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

fn inspect_evtx_boot_source(
    entry: Option<&FileEntry>,
    parsed_at: &str,
    read_header_fn: &mut impl FnMut(&FileEntryId, usize) -> Result<Vec<u8>, String>,
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
                    let extraction =
                        artifacts_windows::extract_boot_shutdown_events(&bytes, &entry.path);
                    parser_warnings.extend(extraction.warnings);
                    if !extraction.events.is_empty() {
                        parsed_any = true;
                    }
                    let event_provenance = entry_provenance(
                        entry,
                        EVTX_BOOT_SHUTDOWN_PARSER,
                        parsed_at,
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
                            provenance: event_provenance.clone(),
                        }
                    }));
                }
                Err(err) => parser_warnings.push(format!("{} 读取失败: {}", entry.path, err)),
            }
            provenance.push(entry_provenance(
                entry,
                EVTX_BOOT_SHUTDOWN_PARSER,
                parsed_at,
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
            provenance.push(AnalysisProvenanceDto {
                data_source_id: String::new(),
                artifact_path: artifact_path.to_string(),
                parser: EVTX_BOOT_SHUTDOWN_PARSER.to_string(),
                parsed_at: parsed_at.to_string(),
                status: AnalysisParseStatusDto::Unavailable,
                warnings: vec![warning],
            });
        }
    }
}
