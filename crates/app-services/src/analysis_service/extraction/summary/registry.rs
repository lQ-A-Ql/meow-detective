use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::extraction::artifact_query::{
    count_artifacts_by_family_prefix, query_artifact_rows, status_from_total,
};
use crate::analysis_service::extraction::attr_mapping::{
    bool_attr, optional_bool_attr, optional_string_attr, optional_u32_attr, optional_u64_attr,
    string_attr, string_vec_attr, u64_attr,
};
use chrono::Utc;
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use transport::dto::{
    AmcacheApplicationDto, AmcacheApplicationFileDto, AnalysisParseStatusDto, AppCompatLayerDto,
    CachedCredentialDto, InstalledSoftwareDto, LastVisitedMruEntryDto, LsaPackageDto, LsaSecretDto,
    MountedDeviceDto, MuiCacheEntryDto, NetworkProfileDto, OpenSaveMruEntryDto,
    RegistryExtractionSummaryDto, RegistryHiveOverviewDto, RegistryRunKeyDto,
    RegistryStructuredSummaryDto, RegistryValueDto, RunMruEntryDto, SamUserAccountDto,
    SecurityPolicyDto, ShellbagEntryDto, ShimCacheEntryDto, ShutdownTimeDto, SystemServiceDto,
    UsbDeviceHistoryDto, UserAssistEntryDto, WinlogonConfigDto,
};

pub fn get_registry_extraction_summary(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<RegistryExtractionSummaryDto, AnalysisServiceError> {
    let total = count_artifacts_by_family_prefix(conn, "Registry")?;
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
    let mut hive_artifact_counts: HashMap<String, u64> = HashMap::new();
    for row in &hive_rows {
        let hive_name = string_attr(&row.attrs, "hivePath")
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string();
        if !hive_name.is_empty() {
            *hive_artifact_counts.entry(hive_name).or_insert(0) += 1;
        }
    }
    let mut hive_overviews: Vec<RegistryHiveOverviewDto> = Vec::new();
    let mut seen_hives = HashSet::new();
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
        let key_value_count = hive_artifact_counts.get(&hive_name).copied().unwrap_or(0);
        hive_overviews.push(RegistryHiveOverviewDto {
            hive_name,
            status,
            key_value_count,
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
