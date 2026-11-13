use crate::analysis_service::provenance::{
    entry_provenance, registry_field_provenance, REGISTRY_SOFTWARE_PARSER, REGISTRY_SYSTEM_PARSER,
};
use crate::analysis_service::system_info_boot::inspect_evtx_boot_source;
use crate::analysis_service::system_info_candidates::find_system_info_candidates;
use crate::analysis_service::MAX_REGISTRY_ANALYSIS_BYTES;
use domain::{FileEntry, FileEntryId};
use rusqlite::Connection;
use std::time::Instant;
use transport::dto::analysis::{
    AnalysisBootRecordDto, AnalysisFieldProvenanceDto, AnalysisNetworkAdapterDto,
    AnalysisProvenanceDto,
};
use transport::dto::{AnalysisParseStatusDto, AnalysisSystemInfoDto};

mod recovery;

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
    shutdown_time: Option<String>,
    network_adapters: Vec<AnalysisNetworkAdapterDto>,
    boot_history: Vec<AnalysisBootRecordDto>,
    field_provenance: Vec<AnalysisFieldProvenanceDto>,
}

pub(crate) struct AnalysisProvenanceContext<'a> {
    pub(crate) parsed_at: &'a str,
    pub(crate) data_source_id: Option<&'a str>,
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

    let candidate_started = Instant::now();
    tracing::info!("Windows system information candidate discovery started");
    match find_system_info_candidates(conn) {
        Ok(candidates) => {
            tracing::info!(
                elapsed_ms = candidate_started.elapsed().as_millis(),
                system = candidates.system_hive.is_some(),
                software = candidates.software_hive.is_some(),
                system_evtx = candidates.system_evtx.is_some(),
                "Windows system information candidate discovery completed"
            );
            let system_hive = candidates.system_hive.as_ref();
            let software_hive = candidates.software_hive.as_ref();
            let system_evtx = candidates.system_evtx.as_ref();

            inspect_registry_hive(
                system_hive,
                candidates.system_input.as_ref(),
                REGISTRY_SYSTEM_PARSER,
                &provenance_context,
                &mut read_header_fn,
                &mut warnings,
                &mut provenance,
                &mut extraction,
            );
            inspect_registry_hive(
                software_hive,
                candidates.software_input.as_ref(),
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
        shutdown_time: extraction.shutdown_time,
        language: None,
        status,
        warnings,
        provenance,
        field_provenance: extraction.field_provenance,
    }
}

fn find_unique_data_source_id(conn: &Connection) -> Option<String> {
    unique_data_source_id_from_files(conn).or_else(|| unique_registered_data_source_id(conn))
}

fn unique_data_source_id_from_files(conn: &Connection) -> Option<String> {
    persistence_source::detail::source_id(conn).ok().flatten()
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
    input: Option<&crate::analysis_service::candidates::EvidenceCandidate>,
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

    let started = Instant::now();
    tracing::info!(parser, path = %entry.path, "Windows system information registry parse started");
    let result = input
        .ok_or_else(|| "registry input metadata is unavailable".to_string())
        .and_then(|input| {
            recovery::snapshot(input, read_header_fn).map_err(|error| error.to_string())
        });
    let (parsed_any, parser_warnings) = match result {
        Ok(snapshot) => {
            let (parsed, mut warnings) =
                inspect_registry_snapshot(&snapshot, entry, parser, extraction);
            if let Some(report) = snapshot.report() {
                warnings.extend(report.diagnostics.iter().map(ToString::to_string));
            }
            (parsed, warnings)
        }
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
    tracing::info!(
        parser,
        path = %entry.path,
        elapsed_ms = started.elapsed().as_millis(),
        parsed = parsed_any,
        "Windows system information registry parse completed"
    );
}

fn inspect_registry_snapshot(
    snapshot: &artifacts_windows::registry::RegistrySnapshot<'_>,
    entry: &FileEntry,
    parser: &str,
    extraction: &mut SystemInfoExtraction,
) -> (bool, Vec<String>) {
    let bytes = snapshot.bytes();
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
        REGISTRY_SYSTEM_PARSER => inspect_system_registry_fields(snapshot, entry, extraction),
        REGISTRY_SOFTWARE_PARSER => inspect_software_registry_fields(snapshot, entry, extraction),
        _ => (false, vec![format!("{} parser unsupported", parser)]),
    }
}

fn inspect_system_registry_fields(
    bytes: &(impl artifacts_windows::registry::RegistryHiveSource + ?Sized),
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
    let mut warnings = info.warnings;
    match artifacts_windows::extract_shutdown_time_from_system_hive(bytes, &entry.path) {
        Ok(entries) => {
            if let Some(entry) = entries.into_iter().next() {
                parsed_any = true;
                extraction.shutdown_time = Some(entry.shutdown_time);
            }
        }
        Err(error) => warnings.push(format!(
            "{} shutdown time parsing failed: {error}",
            entry.path
        )),
    }
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
            (parsed_any, warnings)
        }
        Err(error) => {
            warnings.push(format!(
                "{} network adapter parsing failed: {error}",
                entry.path
            ));
            (parsed_any, warnings)
        }
    }
}

fn inspect_software_registry_fields(
    bytes: &(impl artifacts_windows::registry::RegistryHiveSource + ?Sized),
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
    parsed_any |= assign_build_number(info.current_build, info.update_build_revision, extraction);
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

fn assign_build_number(
    current_build: Option<artifacts_windows::ParsedRegistryField>,
    update_build_revision: Option<artifacts_windows::ParsedRegistryField>,
    extraction: &mut SystemInfoExtraction,
) -> bool {
    let Some(current_build) = current_build else {
        return false;
    };
    let revision = update_build_revision
        .as_ref()
        .map(|field| field.value.as_str());
    extraction.build_number = Some(format_full_build_number(&current_build.value, revision));
    extraction
        .field_provenance
        .push(registry_field_provenance("buildNumber", current_build));
    if let Some(update_build_revision) = update_build_revision {
        extraction.field_provenance.push(registry_field_provenance(
            "buildNumber",
            update_build_revision,
        ));
    }
    true
}

pub(in crate::analysis_service) fn format_full_build_number(
    current_build: &str,
    update_build_revision: Option<&str>,
) -> String {
    let current_build = current_build.trim();
    match update_build_revision
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(revision) if !current_build.contains('.') => {
            format!("{current_build}.{revision}")
        }
        _ => current_build.to_string(),
    }
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

pub(crate) fn push_unavailable_provenance(
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
