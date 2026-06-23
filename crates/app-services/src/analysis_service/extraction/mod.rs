pub(crate) mod browser;
pub(crate) mod email;
pub(crate) mod evtx;
pub(crate) mod registry;

use self::browser::extract_browser_candidate;
use self::email::extract_email_candidate;
use self::evtx::extract_evtx_candidate;
pub use self::registry::extract_registry_candidate;
use crate::analysis_service::candidates::{
    evidence_candidates_for_categories, normalize_evidence_path, EvidenceCandidate,
};
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::MAX_ANALYSIS_SOURCE_BYTES;
use chrono::Utc;
use domain::{Artifact, FileEntryId, TimelineEvent};
use persistence_sqlite::repositories::{artifact_repo::ArtifactRepo, timeline_repo::TimelineRepo};
use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use transport::dto::{
    AmcacheApplicationDto, AmcacheApplicationFileDto, AnalysisExtractionRunDto,
    AnalysisParseStatusDto, AppCompatLayerDto, BrowserDownloadDto, BrowserHistorySummaryDto,
    BrowserVisitDto, CachedCredentialDto, EmailExtractionSummaryDto, EmailMessageDto,
    InstalledSoftwareDto, LastVisitedMruEntryDto, LsaPackageDto, LsaSecretDto, MountedDeviceDto,
    MuiCacheEntryDto, NetworkProfileDto, OpenSaveMruEntryDto, RegistryExtractionSummaryDto,
    RegistryHiveOverviewDto, RegistryRunKeyDto, RegistryStructuredSummaryDto, RegistryValueDto,
    RunMruEntryDto, SamUserAccountDto, SecurityPolicyDto, ShellbagEntryDto, ShimCacheEntryDto,
    ShutdownTimeDto, SystemServiceDto, UsbDeviceHistoryDto, UserAssistEntryDto, WinlogonConfigDto,
};

type TxlogBytes = (Option<Vec<u8>>, Option<Vec<u8>>);

pub fn run_analysis_extraction<E: std::fmt::Display>(
    conn: &Connection,
    case_id: &str,
    categories: &[&str],
    mut file_reader: impl FnMut(&FileEntryId) -> Result<Box<dyn Read>, E>,
) -> Result<AnalysisExtractionRunDto, AnalysisServiceError> {
    let generated_at = Utc::now().to_rfc3339();
    let selected = if categories.is_empty() {
        vec!["Registry", "BrowserHistory", "Email"]
    } else {
        categories.to_vec()
    };
    let candidates = evidence_candidates_for_categories(conn, &selected)?;
    let mut artifacts = Vec::new();
    let mut events = Vec::new();
    let mut warnings = Vec::new();
    let mut scanned_count = 0u64;

    // Pre-load registry hives so that SAM/SECURITY extraction can reuse the
    // SYSTEM BootKey without re-reading files mid-loop.  Also pre-load
    // companion .LOG1/.LOG2 transaction logs when they are present.
    let mut registry_bytes: HashMap<(String, String), Vec<u8>> = HashMap::new();
    let mut txlog_bytes: HashMap<(String, String), TxlogBytes> = HashMap::new();
    let mut boot_keys: HashMap<String, Option<[u8; 16]>> = HashMap::new();
    for candidate in &candidates {
        if candidate.category != "Registry" {
            continue;
        }
        if already_has_v1_artifacts(conn, candidate)? {
            continue;
        }
        let mut reader = match file_reader(&candidate.file_id) {
            Ok(reader) => reader,
            Err(err) => {
                warnings.push(format!("{} read failed: {}", candidate.path, err));
                continue;
            }
        };
        let mut bytes = Vec::new();
        if let Err(err) = reader
            .by_ref()
            .take(MAX_ANALYSIS_SOURCE_BYTES as u64)
            .read_to_end(&mut bytes)
        {
            warnings.push(format!("{} read failed: {}", candidate.path, err));
            continue;
        }
        let normalized = normalize_evidence_path(&candidate.path);
        if normalized.ends_with("/windows/system32/config/system") {
            boot_keys.insert(
                candidate.data_source_id.clone(),
                artifacts_windows::extract_boot_key(&bytes),
            );
        }
        registry_bytes.insert(
            (candidate.data_source_id.clone(), normalized.clone()),
            bytes,
        );

        let log1_path = format!("{}.log1", normalized);
        let log2_path = format!("{}.log2", normalized);
        let log1_id = find_file_entry_id_by_path(conn, &candidate.data_source_id, &log1_path);
        let log2_id = find_file_entry_id_by_path(conn, &candidate.data_source_id, &log2_path);
        let log1_bytes = log1_id.and_then(|id| {
            read_file_entry_bytes(
                &mut file_reader,
                &id,
                &candidate.path,
                "LOG1",
                &mut warnings,
            )
        });
        let log2_bytes = log2_id.and_then(|id| {
            read_file_entry_bytes(
                &mut file_reader,
                &id,
                &candidate.path,
                "LOG2",
                &mut warnings,
            )
        });
        txlog_bytes.insert(
            (candidate.data_source_id.clone(), normalized),
            (log1_bytes, log2_bytes),
        );
    }

    for candidate in candidates {
        if !matches!(
            candidate.category.as_str(),
            "Registry" | "BrowserHistory" | "Email" | "EventLogs"
        ) {
            continue;
        }
        if already_has_v1_artifacts(conn, &candidate)? {
            continue;
        }

        let outcome = match candidate.category.as_str() {
            "Registry" => {
                let key = (
                    candidate.data_source_id.clone(),
                    normalize_evidence_path(&candidate.path),
                );
                let Some(bytes) = registry_bytes.get(&key) else {
                    warnings.push(format!("{} registry bytes not preloaded", candidate.path));
                    continue;
                };
                let boot_key = boot_keys.get(&candidate.data_source_id).copied().flatten();
                let (txlog1, txlog2) = txlog_bytes
                    .get(&key)
                    .map(|(a, b)| (a.as_deref(), b.as_deref()))
                    .unwrap_or((None, None));
                scanned_count += 1;
                extract_registry_candidate(&candidate, bytes, boot_key, txlog1, txlog2)
            }
            "BrowserHistory" | "Email" | "EventLogs" => {
                let mut reader = match file_reader(&candidate.file_id) {
                    Ok(reader) => reader,
                    Err(err) => {
                        warnings.push(format!("{} read failed: {}", candidate.path, err));
                        continue;
                    }
                };
                let mut bytes = Vec::new();
                if let Err(err) = reader
                    .by_ref()
                    .take(MAX_ANALYSIS_SOURCE_BYTES as u64)
                    .read_to_end(&mut bytes)
                {
                    warnings.push(format!("{} read failed: {}", candidate.path, err));
                    continue;
                }
                scanned_count += 1;
                match candidate.category.as_str() {
                    "BrowserHistory" => extract_browser_candidate(&candidate, &bytes),
                    "Email" => extract_email_candidate(&candidate, &bytes),
                    "EventLogs" => extract_evtx_candidate(&candidate, &bytes),
                    _ => ExtractionOutcome::default(),
                }
            }
            _ => ExtractionOutcome::default(),
        };
        warnings.extend(outcome.warnings);
        artifacts.extend(outcome.artifacts);
        events.extend(outcome.timeline_events);
    }

    if !artifacts.is_empty() {
        let by_source = artifacts_by_data_source(artifacts);
        let repo = ArtifactRepo::new(conn);
        for (data_source_id, group) in by_source {
            repo.insert_batch(&group, case_id, &data_source_id)?;
        }
    }
    if !events.is_empty() {
        TimelineRepo::new(conn).insert_batch_with_case(&events, case_id)?;
    }

    let artifact_count = count_analysis_artifacts(conn)?;
    Ok(AnalysisExtractionRunDto {
        status: if scanned_count == 0 {
            AnalysisParseStatusDto::NotFound
        } else if warnings.is_empty() {
            AnalysisParseStatusDto::Parsed
        } else {
            AnalysisParseStatusDto::Partial
        },
        scanned_count,
        artifact_count,
        timeline_event_count: events.len() as u64,
        generated_at,
        warnings,
    })
}

pub fn get_registry_extraction_summary(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<RegistryExtractionSummaryDto, AnalysisServiceError> {
    let total = count_artifacts_by_type(conn, "RegistryValue")?;
    let rows = query_artifact_rows(conn, &["RegistryValue"], offset, limit)?;
    let values = rows
        .into_iter()
        .map(|row| RegistryValueDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            hive_path: string_attr(&row.attrs, "hivePath"),
            key_path: string_attr(&row.attrs, "keyPath"),
            value_name: string_attr(&row.attrs, "valueName"),
            value_type: string_attr(&row.attrs, "valueType"),
            data: string_attr(&row.attrs, "data"),
            parser: row
                .extractor_id
                .unwrap_or_else(|| "registry.v1".to_string()),
            created_at: row.created_at,
        })
        .collect::<Vec<_>>();
    Ok(RegistryExtractionSummaryDto {
        status: status_from_total(total),
        total,
        values,
        generated_at: Utc::now().to_rfc3339(),
        warnings: Vec::new(),
    })
}

pub fn get_registry_structured_summary(
    conn: &Connection,
) -> Result<RegistryStructuredSummaryDto, AnalysisServiceError> {
    let sam_rows = query_artifact_rows(conn, &["RegistrySamUser"], 0, 10_000)?;
    let sam_users = sam_rows
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
        .collect::<Vec<_>>();

    let ua_rows = query_artifact_rows(conn, &["RegistryUserAssist"], 0, 10_000)?;
    let user_assist_entries = ua_rows
        .into_iter()
        .map(|row| UserAssistEntryDto {
            program_path: string_attr(&row.attrs, "programPath"),
            exec_count: u64_attr(&row.attrs, "execCount") as u32,
            last_exec_time: optional_string_attr(&row.attrs, "lastExecTime"),
            is_suspicious: None,
            suspicious_reason: None,
        })
        .collect::<Vec<_>>();

    let hive_rows = query_artifact_rows(conn, &["RegistryHive", "RegistryValue"], 0, 10_000)?;
    let mut hive_overviews: Vec<RegistryHiveOverviewDto> = Vec::new();
    let mut seen_hives = std::collections::HashSet::new();
    for row in hive_rows {
        let hive_name = string_attr(&row.attrs, "hivePath")
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string();
        if hive_name.is_empty() || !seen_hives.insert(hive_name.clone()) {
            continue;
        }
        let status = if row.source_object_id.is_some() {
            AnalysisParseStatusDto::Parsed
        } else {
            AnalysisParseStatusDto::Partial
        };
        hive_overviews.push(RegistryHiveOverviewDto {
            hive_name,
            status,
            key_value_count: 0,
            extracted_at: row.created_at.clone(),
            data_source_id: string_attr(&row.attrs, "dataSourceId"),
            source_path: string_attr(&row.attrs, "sourcePath"),
            txlog_merged: bool_attr(&row.attrs, "txlogMerged"),
            deleted_keys_found: u64_attr(&row.attrs, "deletedKeysFound") as u32,
        });
    }

    let network_rows = query_artifact_rows(conn, &["RegistryNetworkProfile"], 0, 10_000)?;
    let network_profiles = network_rows
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
        .collect::<Vec<_>>();

    let software_rows = query_artifact_rows(conn, &["RegistryInstalledSoftware"], 0, 10_000)?;
    let installed_software = software_rows
        .into_iter()
        .map(|row| InstalledSoftwareDto {
            display_name: string_attr(&row.attrs, "displayName"),
            version: string_attr(&row.attrs, "version"),
            publisher: optional_string_attr(&row.attrs, "publisher"),
            install_date: optional_string_attr(&row.attrs, "installDate"),
            estimated_size: optional_string_attr(&row.attrs, "estimatedSize"),
            is_suspicious: None,
        })
        .collect::<Vec<_>>();

    let service_rows = query_artifact_rows(conn, &["RegistrySystemService"], 0, 10_000)?;
    let system_services = service_rows
        .into_iter()
        .map(|row| SystemServiceDto {
            service_name: string_attr(&row.attrs, "serviceName"),
            display_name: optional_string_attr(&row.attrs, "displayName"),
            image_path: optional_string_attr(&row.attrs, "imagePath"),
            service_dll: optional_string_attr(&row.attrs, "serviceDll"),
            service_type: string_attr(&row.attrs, "serviceType"),
            start_type: string_attr(&row.attrs, "startType"),
            delayed_auto_start: bool_attr(&row.attrs, "delayedAutoStart"),
            error_control: optional_u32_attr(&row.attrs, "errorControl"),
            group: optional_string_attr(&row.attrs, "group"),
            object_name: optional_string_attr(&row.attrs, "objectName"),
            depend_on_service: string_vec_attr(&row.attrs, "dependOnService"),
            depend_on_group: string_vec_attr(&row.attrs, "dependOnGroup"),
            failure_command: optional_string_attr(&row.attrs, "failureCommand"),
            required_privileges: string_vec_attr(&row.attrs, "requiredPrivileges"),
            key_path: string_attr(&row.attrs, "keyPath"),
            key_last_write: optional_string_attr(&row.attrs, "keyLastWrite"),
        })
        .collect::<Vec<_>>();

    let usb_rows = query_artifact_rows(conn, &["RegistryUsbDevice"], 0, 10_000)?;
    let usb_devices = usb_rows
        .into_iter()
        .map(|row| UsbDeviceHistoryDto {
            device_name: string_attr(&row.attrs, "deviceName"),
            serial_number: string_attr(&row.attrs, "serialNumber"),
            first_connect: optional_string_attr(&row.attrs, "firstConnect"),
            last_connect: optional_string_attr(&row.attrs, "lastConnect"),
            volume_label: optional_string_attr(&row.attrs, "volumeLabel"),
            drive_letter: optional_string_attr(&row.attrs, "driveLetter"),
            file_system: optional_string_attr(&row.attrs, "fileSystem"),
            capacity: optional_string_attr(&row.attrs, "capacity"),
            is_suspicious: None,
            suspicious_reason: None,
        })
        .collect::<Vec<_>>();

    let mounted_rows = query_artifact_rows(conn, &["RegistryMountedDevice"], 0, 10_000)?;
    let mounted_devices = mounted_rows
        .into_iter()
        .map(|row| MountedDeviceDto {
            device_name: string_attr(&row.attrs, "deviceName"),
            drive_letter: optional_string_attr(&row.attrs, "driveLetter"),
            volume_guid: optional_string_attr(&row.attrs, "volumeGuid"),
            disk_signature_hex: optional_string_attr(&row.attrs, "diskSignatureHex"),
            target_name: optional_string_attr(&row.attrs, "targetName"),
        })
        .collect::<Vec<_>>();

    let shutdown_rows = query_artifact_rows(conn, &["RegistryShutdownTime"], 0, 10_000)?;
    let shutdown_times = shutdown_rows
        .into_iter()
        .map(|row| ShutdownTimeDto {
            key_path: string_attr(&row.attrs, "keyPath"),
            shutdown_time: string_attr(&row.attrs, "shutdownTime"),
        })
        .collect::<Vec<_>>();

    let shimcache_rows = query_artifact_rows(conn, &["RegistryShimCache"], 0, 10_000)?;
    let shimcache_entries = shimcache_rows
        .into_iter()
        .map(|row| ShimCacheEntryDto {
            path: string_attr(&row.attrs, "path"),
            last_modified: optional_string_attr(&row.attrs, "lastModified"),
            source_key_path: string_attr(&row.attrs, "sourceKeyPath"),
        })
        .collect::<Vec<_>>();

    let run_key_rows = query_artifact_rows(conn, &["RegistryMachineRunKey"], 0, 10_000)?;
    let run_keys = run_key_rows
        .into_iter()
        .map(|row| RegistryRunKeyDto {
            key_path: string_attr(&row.attrs, "keyPath"),
            value_name: string_attr(&row.attrs, "valueName"),
            command: string_attr(&row.attrs, "command"),
            timestamp: optional_string_attr(&row.attrs, "timestamp"),
            scope: string_attr(&row.attrs, "scope"),
        })
        .collect::<Vec<_>>();

    let winlogon_rows = query_artifact_rows(conn, &["RegistryWinlogonConfig"], 0, 10_000)?;
    let winlogon_config = winlogon_rows
        .into_iter()
        .next()
        .map(|row| WinlogonConfigDto {
            shell: optional_string_attr(&row.attrs, "shell"),
            userinit: optional_string_attr(&row.attrs, "userinit"),
            notify: optional_string_attr(&row.attrs, "notify"),
            auto_admin_logon: optional_string_attr(&row.attrs, "autoAdminLogon"),
            default_domain_name: optional_string_attr(&row.attrs, "defaultDomainName"),
            default_user_name: optional_string_attr(&row.attrs, "defaultUserName"),
            key_path: string_attr(&row.attrs, "keyPath"),
        });

    let lsa_rows = query_artifact_rows(conn, &["RegistryLsaPackage"], 0, 10_000)?;
    let lsa_packages = lsa_rows
        .into_iter()
        .map(|row| LsaPackageDto {
            control_set: string_attr(&row.attrs, "controlSet"),
            authentication_packages: string_vec_attr(&row.attrs, "authenticationPackages"),
            notification_packages: string_vec_attr(&row.attrs, "notificationPackages"),
            security_packages: string_vec_attr(&row.attrs, "securityPackages"),
        })
        .collect::<Vec<_>>();

    let open_save_rows = query_artifact_rows(conn, &["RegistryOpenSaveMru"], 0, 10_000)?;
    let open_save_mru = open_save_rows
        .into_iter()
        .map(|row| OpenSaveMruEntryDto {
            extension: string_attr(&row.attrs, "extension"),
            value_name: string_attr(&row.attrs, "valueName"),
            file_name: string_attr(&row.attrs, "fileName"),
            raw_pidl_hex: string_attr(&row.attrs, "rawPidlHex"),
            source_key_path: string_attr(&row.attrs, "sourceKeyPath"),
            last_write: optional_string_attr(&row.attrs, "lastWrite"),
        })
        .collect::<Vec<_>>();

    let last_visited_rows = query_artifact_rows(conn, &["RegistryLastVisitedMru"], 0, 10_000)?;
    let last_visited_mru = last_visited_rows
        .into_iter()
        .map(|row| LastVisitedMruEntryDto {
            value_name: string_attr(&row.attrs, "valueName"),
            path: string_attr(&row.attrs, "path"),
            raw_pidl_hex: string_attr(&row.attrs, "rawPidlHex"),
            source_key_path: string_attr(&row.attrs, "sourceKeyPath"),
            last_write: optional_string_attr(&row.attrs, "lastWrite"),
        })
        .collect::<Vec<_>>();

    let run_mru_rows = query_artifact_rows(conn, &["RegistryRunMru"], 0, 10_000)?;
    let run_mru = run_mru_rows
        .into_iter()
        .map(|row| RunMruEntryDto {
            value_name: string_attr(&row.attrs, "valueName"),
            command: string_attr(&row.attrs, "command"),
            source_key_path: string_attr(&row.attrs, "sourceKeyPath"),
            last_write: optional_string_attr(&row.attrs, "lastWrite"),
        })
        .collect::<Vec<_>>();

    let shellbag_rows = query_artifact_rows(conn, &["RegistryShellbag"], 0, 10_000)?;
    let shellbag_entries = shellbag_rows
        .into_iter()
        .map(|row| ShellbagEntryDto {
            path: string_attr(&row.attrs, "path"),
            raw_pidl_hex: string_attr(&row.attrs, "rawPidlHex"),
            node_slot: optional_u32_attr(&row.attrs, "nodeSlot"),
            source_key_path: string_attr(&row.attrs, "sourceKeyPath"),
            last_write: optional_string_attr(&row.attrs, "lastWrite"),
        })
        .collect::<Vec<_>>();

    let muicache_rows = query_artifact_rows(conn, &["RegistryMuiCache"], 0, 10_000)?;
    let muicache_entries = muicache_rows
        .into_iter()
        .map(|row| MuiCacheEntryDto {
            program_path: string_attr(&row.attrs, "programPath"),
            friendly_name: string_attr(&row.attrs, "friendlyName"),
            source_key_path: string_attr(&row.attrs, "sourceKeyPath"),
            last_write: optional_string_attr(&row.attrs, "lastWrite"),
        })
        .collect::<Vec<_>>();

    let amcache_app_rows = query_artifact_rows(conn, &["RegistryAmcacheApplication"], 0, 10_000)?;
    let amcache_applications = amcache_app_rows
        .into_iter()
        .map(|row| AmcacheApplicationDto {
            program_id: optional_string_attr(&row.attrs, "programId"),
            name: optional_string_attr(&row.attrs, "name"),
            version: optional_string_attr(&row.attrs, "version"),
            publisher: optional_string_attr(&row.attrs, "publisher"),
            install_date: optional_string_attr(&row.attrs, "installDate"),
            source: optional_string_attr(&row.attrs, "source"),
            os_version_at_install_time: optional_string_attr(&row.attrs, "osVersionAtInstallTime"),
            registry_key_path: string_attr(&row.attrs, "registryKeyPath"),
        })
        .collect::<Vec<_>>();

    let amcache_file_rows =
        query_artifact_rows(conn, &["RegistryAmcacheApplicationFile"], 0, 10_000)?;
    let amcache_application_files = amcache_file_rows
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
        .collect::<Vec<_>>();

    let appcompat_layer_rows = query_artifact_rows(conn, &["RegistryAppCompatLayer"], 0, 10_000)?;
    let appcompat_layers = appcompat_layer_rows
        .into_iter()
        .map(|row| AppCompatLayerDto {
            executable_path: string_attr(&row.attrs, "executablePath"),
            layer_string: string_attr(&row.attrs, "layerString"),
            source_hive_path: string_attr(&row.attrs, "sourceHivePath"),
            source_key_path: string_attr(&row.attrs, "sourceKeyPath"),
            last_write: optional_string_attr(&row.attrs, "lastWrite"),
        })
        .collect::<Vec<_>>();

    let security_policy_rows = query_artifact_rows(conn, &["RegistrySecurityPolicy"], 0, 10_000)?;
    let security_policies = security_policy_rows
        .into_iter()
        .map(|row| SecurityPolicyDto {
            domain_name: optional_string_attr(&row.attrs, "domainName"),
            account_domain_name: optional_string_attr(&row.attrs, "accountDomainName"),
            machine_sid: optional_string_attr(&row.attrs, "machineSid"),
            audit_policy_hex: optional_string_attr(&row.attrs, "auditPolicyHex"),
            source_key_path: string_attr(&row.attrs, "sourceKeyPath"),
            last_write: optional_string_attr(&row.attrs, "lastWrite"),
        })
        .collect::<Vec<_>>();

    let lsa_secret_rows = query_artifact_rows(conn, &["RegistryLsaSecret"], 0, 10_000)?;
    let lsa_secrets = lsa_secret_rows
        .into_iter()
        .map(|row| LsaSecretDto {
            secret_name: string_attr(&row.attrs, "secretName"),
            version: string_attr(&row.attrs, "version"),
            encrypted_blob_hex: string_attr(&row.attrs, "encryptedBlobHex"),
            source_key_path: string_attr(&row.attrs, "sourceKeyPath"),
            last_write: optional_string_attr(&row.attrs, "lastWrite"),
        })
        .collect::<Vec<_>>();

    let cached_credential_rows =
        query_artifact_rows(conn, &["RegistryCachedCredential"], 0, 10_000)?;
    let cached_credentials = cached_credential_rows
        .into_iter()
        .map(|row| CachedCredentialDto {
            entry_name: string_attr(&row.attrs, "entryName"),
            encrypted_blob_hex: string_attr(&row.attrs, "encryptedBlobHex"),
            source_key_path: string_attr(&row.attrs, "sourceKeyPath"),
            last_write: optional_string_attr(&row.attrs, "lastWrite"),
        })
        .collect::<Vec<_>>();

    let status = if sam_users.is_empty()
        && user_assist_entries.is_empty()
        && hive_overviews.is_empty()
        && network_profiles.is_empty()
        && usb_devices.is_empty()
        && mounted_devices.is_empty()
        && shutdown_times.is_empty()
        && shimcache_entries.is_empty()
        && run_keys.is_empty()
        && open_save_mru.is_empty()
        && last_visited_mru.is_empty()
        && run_mru.is_empty()
        && shellbag_entries.is_empty()
        && muicache_entries.is_empty()
        && amcache_applications.is_empty()
        && amcache_application_files.is_empty()
        && winlogon_config.is_none()
        && lsa_packages.is_empty()
        && appcompat_layers.is_empty()
        && security_policies.is_empty()
        && lsa_secrets.is_empty()
        && cached_credentials.is_empty()
    {
        AnalysisParseStatusDto::NotFound
    } else {
        AnalysisParseStatusDto::Parsed
    };

    Ok(RegistryStructuredSummaryDto {
        hive_overviews,
        sam_users,
        user_assist_entries,
        network_profiles,
        installed_software,
        usb_devices,
        mounted_devices,
        system_services,
        shutdown_times,
        shimcache_entries,
        run_keys,
        open_save_mru,
        last_visited_mru,
        run_mru,
        shellbag_entries,
        muicache_entries,
        amcache_applications,
        amcache_application_files,
        winlogon_config,
        lsa_packages,
        appcompat_layers,
        security_policies,
        lsa_secrets,
        cached_credentials,
        status,
        generated_at: Utc::now().to_rfc3339(),
        warnings: Vec::new(),
    })
}

pub fn get_browser_history_summary(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<BrowserHistorySummaryDto, AnalysisServiceError> {
    let visit_total = count_artifacts_by_type(conn, "BrowserHistory")?;
    let download_total = count_artifacts_by_type(conn, "BrowserDownload")?;
    let visit_rows = query_artifact_rows(conn, &["BrowserHistory"], offset, limit)?;
    let download_rows = query_artifact_rows(conn, &["BrowserDownload"], offset, limit)?;
    let visits = visit_rows
        .into_iter()
        .map(|row| BrowserVisitDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            browser: string_attr(&row.attrs, "browser"),
            profile: string_attr(&row.attrs, "profile"),
            url: string_attr(&row.attrs, "url"),
            title: string_attr(&row.attrs, "title"),
            visit_time: optional_string_attr(&row.attrs, "visitTime"),
            visit_count: u64_attr(&row.attrs, "visitCount"),
        })
        .collect::<Vec<_>>();
    let downloads = download_rows
        .into_iter()
        .map(|row| BrowserDownloadDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            browser: string_attr(&row.attrs, "browser"),
            profile: string_attr(&row.attrs, "profile"),
            url: string_attr(&row.attrs, "url"),
            target_path: string_attr(&row.attrs, "targetPath"),
            start_time: optional_string_attr(&row.attrs, "startTime"),
            total_bytes: u64_attr(&row.attrs, "totalBytes"),
        })
        .collect::<Vec<_>>();
    Ok(BrowserHistorySummaryDto {
        status: status_from_total(visit_total + download_total),
        visit_total,
        download_total,
        visits,
        downloads,
        generated_at: Utc::now().to_rfc3339(),
        warnings: Vec::new(),
    })
}

pub fn get_email_extraction_summary(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<EmailExtractionSummaryDto, AnalysisServiceError> {
    let total = count_artifacts_by_type(conn, "EmailMessage")?;
    let rows = query_artifact_rows(conn, &["EmailMessage"], offset, limit)?;
    let messages = rows
        .into_iter()
        .map(|row| EmailMessageDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            sent_at: optional_string_attr(&row.attrs, "sentAt"),
            from: string_attr(&row.attrs, "from"),
            to: string_vec_attr(&row.attrs, "to"),
            cc: string_vec_attr(&row.attrs, "cc"),
            bcc: string_vec_attr(&row.attrs, "bcc"),
            subject: string_attr(&row.attrs, "subject"),
            message_id: string_attr(&row.attrs, "messageId"),
            attachments: string_vec_attr(&row.attrs, "attachments"),
            body_preview: string_attr(&row.attrs, "bodyPreview"),
        })
        .collect::<Vec<_>>();
    Ok(EmailExtractionSummaryDto {
        status: status_from_total(total),
        total,
        messages,
        generated_at: Utc::now().to_rfc3339(),
        warnings: Vec::new(),
    })
}

#[derive(Default)]
pub struct ExtractionOutcome {
    pub artifacts: Vec<Artifact>,
    pub timeline_events: Vec<TimelineEvent>,
    pub warnings: Vec<String>,
}

struct AnalysisArtifactRow {
    id: String,
    source_object_id: Option<String>,
    extractor_id: Option<String>,
    created_at: String,
    attrs: BTreeMap<String, Value>,
}

fn already_has_v1_artifacts(
    conn: &Connection,
    candidate: &EvidenceCandidate,
) -> Result<bool, AnalysisServiceError> {
    let families = match candidate.category.as_str() {
        "Registry" => &[
            "RegistryValue",
            "RegistrySamUser",
            "RegistryUserAssist",
            "RegistryHive",
            "RegistryNetworkAdapter",
            "RegistryNetworkProfile",
            "RegistryInstalledSoftware",
            "RegistrySystemService",
            "RegistryUsbDevice",
            "RegistryMountedDevice",
            "RegistryShutdownTime",
            "RegistryShimCache",
            "RegistryMachineRunKey",
            "RegistryWinlogonConfig",
            "RegistryLsaPackage",
            "RegistryOpenSaveMru",
            "RegistryLastVisitedMru",
            "RegistryRunMru",
            "RegistryShellbag",
            "RegistryMuiCache",
            "RegistryAmcacheApplication",
            "RegistryAmcacheApplicationFile",
            "RegistryAppCompatLayer",
            "RegistrySecurityPolicy",
            "RegistryLsaSecret",
            "RegistryCachedCredential",
        ][..],
        "BrowserHistory" => &["BrowserHistory", "BrowserDownload"][..],
        "Email" => &["EmailMessage"][..],
        _ => &[][..],
    };
    if families.is_empty() {
        return Ok(false);
    }
    let placeholders = (1..=families.len())
        .map(|index| format!("?{}", index + 1))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT COUNT(*) FROM artifacts WHERE source_object_id = ?1 AND artifact_type IN ({})",
        placeholders
    );
    let mut params_values: Vec<Box<dyn rusqlite::types::ToSql>> =
        vec![Box::new(candidate.file_id.0.clone())];
    for family in families {
        params_values.push(Box::new((*family).to_string()));
    }
    let params_refs = params_values
        .iter()
        .map(|param| param.as_ref())
        .collect::<Vec<&dyn rusqlite::types::ToSql>>();
    let count: i64 = conn.query_row(&sql, params_refs.as_slice(), |row| row.get(0))?;
    Ok(count > 0)
}

fn artifacts_by_data_source(artifacts: Vec<Artifact>) -> HashMap<String, Vec<Artifact>> {
    let mut grouped: HashMap<String, Vec<Artifact>> = HashMap::new();
    for artifact in artifacts {
        let data_source_id = artifact
            .attrs
            .get("dataSourceId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        grouped.entry(data_source_id).or_default().push(artifact);
    }
    grouped
}

fn count_analysis_artifacts(conn: &Connection) -> Result<u64, AnalysisServiceError> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM artifacts WHERE artifact_type IN ('RegistryValue', 'RegistrySamUser', 'RegistryUserAssist', 'RegistryHive', 'RegistryNetworkAdapter', 'RegistryNetworkProfile', 'RegistryInstalledSoftware', 'RegistrySystemService', 'RegistryUsbDevice', 'RegistryMountedDevice', 'RegistryShutdownTime', 'RegistryShimCache', 'RegistryMachineRunKey', 'RegistryWinlogonConfig', 'RegistryLsaPackage', 'RegistryOpenSaveMru', 'RegistryLastVisitedMru', 'RegistryRunMru', 'RegistryShellbag', 'RegistryMuiCache', 'RegistryAmcacheApplication', 'RegistryAmcacheApplicationFile', 'RegistryAppCompatLayer', 'RegistrySecurityPolicy', 'RegistryLsaSecret', 'RegistryCachedCredential', 'BrowserHistory', 'BrowserDownload', 'EmailMessage')",
            [],
            |row| row.get(0),
        )
        ?;
    Ok(count as u64)
}

fn count_artifacts_by_type(
    conn: &Connection,
    artifact_type: &str,
) -> Result<u64, AnalysisServiceError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM artifacts WHERE artifact_type = ?1",
        [artifact_type],
        |row| row.get(0),
    )?;
    Ok(count as u64)
}

fn query_artifact_rows(
    conn: &Connection,
    families: &[&str],
    offset: u64,
    limit: u32,
) -> Result<Vec<AnalysisArtifactRow>, AnalysisServiceError> {
    if families.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = (1..=families.len())
        .map(|index| format!("?{}", index))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT id, source_object_id, extractor_id, created_at, attrs
         FROM artifacts
         WHERE artifact_type IN ({})
         ORDER BY created_at DESC, id ASC
         LIMIT ?{} OFFSET ?{}",
        placeholders,
        families.len() + 1,
        families.len() + 2
    );
    let mut params_values: Vec<Box<dyn rusqlite::types::ToSql>> = families
        .iter()
        .map(|family| Box::new((*family).to_string()) as Box<dyn rusqlite::types::ToSql>)
        .collect();
    params_values.push(Box::new(limit as i64));
    params_values.push(Box::new(offset as i64));
    let params_refs = params_values
        .iter()
        .map(|param| param.as_ref())
        .collect::<Vec<&dyn rusqlite::types::ToSql>>();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_refs.as_slice(), |row| {
        let attrs_text: String = row.get(4)?;
        Ok(AnalysisArtifactRow {
            id: row.get(0)?,
            source_object_id: row.get(1)?,
            extractor_id: row.get(2)?,
            created_at: row.get(3)?,
            attrs: serde_json::from_str(&attrs_text).unwrap_or_default(),
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

fn status_from_total(total: u64) -> AnalysisParseStatusDto {
    if total > 0 {
        AnalysisParseStatusDto::Parsed
    } else {
        AnalysisParseStatusDto::NotFound
    }
}

/// Locate a file entry by its normalized (lower-case, forward-slash) path.
fn find_file_entry_id_by_path(
    conn: &Connection,
    data_source_id: &str,
    normalized_path: &str,
) -> Option<domain::FileEntryId> {
    conn.query_row(
        "SELECT id FROM file_entries \
         WHERE data_source_id = ?1 \
           AND REPLACE(LOWER(path), '\\', '/') = ?2 \
           AND entry_type = 'file' COLLATE NOCASE",
        [data_source_id, normalized_path],
        |row| Ok(domain::FileEntryId(row.get(0)?)),
    )
    .optional()
    .ok()
    .flatten()
}

/// Read the contents of a companion file (e.g. a transaction log) using the
/// same size-bounded reader used for primary evidence sources.
fn read_file_entry_bytes<E: std::fmt::Display>(
    file_reader: &mut impl FnMut(&domain::FileEntryId) -> Result<Box<dyn std::io::Read>, E>,
    file_id: &domain::FileEntryId,
    hive_path: &str,
    label: &str,
    warnings: &mut Vec<String>,
) -> Option<Vec<u8>> {
    let reader = match file_reader(file_id) {
        Ok(reader) => reader,
        Err(err) => {
            warnings.push(format!("{} {} read failed: {}", hive_path, label, err));
            return None;
        }
    };
    let mut bytes = Vec::new();
    if let Err(err) = reader
        .take(MAX_ANALYSIS_SOURCE_BYTES as u64)
        .read_to_end(&mut bytes)
    {
        warnings.push(format!("{} {} read failed: {}", hive_path, label, err));
        return None;
    }
    Some(bytes)
}

fn string_attr(attrs: &BTreeMap<String, Value>, key: &str) -> String {
    attrs
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_default()
}

fn optional_string_attr(attrs: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    attrs
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn u64_attr(attrs: &BTreeMap<String, Value>, key: &str) -> u64 {
    attrs.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn bool_attr(attrs: &BTreeMap<String, Value>, key: &str) -> bool {
    attrs.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn optional_u32_attr(attrs: &BTreeMap<String, Value>, key: &str) -> Option<u32> {
    attrs.get(key).and_then(Value::as_u64).map(|v| v as u32)
}

fn optional_u64_attr(attrs: &BTreeMap<String, Value>, key: &str) -> Option<u64> {
    attrs.get(key).and_then(Value::as_u64)
}

fn optional_bool_attr(attrs: &BTreeMap<String, Value>, key: &str) -> Option<bool> {
    attrs.get(key).and_then(Value::as_bool)
}

fn string_vec_attr(attrs: &BTreeMap<String, Value>, key: &str) -> Vec<String> {
    attrs
        .get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}
