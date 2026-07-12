use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::extraction::artifact_query::query_artifact_rows;
use crate::analysis_service::extraction::attr_mapping::{
    bool_attr, optional_string_attr, optional_u32_attr, string_attr, string_vec_attr,
};
use rusqlite::Connection;
use transport::dto::{
    LsaPackageDto, MountedDeviceDto, RegistryRunKeyDto, ShimCacheEntryDto, ShutdownTimeDto,
    SystemServiceDto, UsbDeviceHistoryDto, WinlogonConfigDto,
};

pub(super) struct SystemRegistryData {
    pub(super) system_services: Vec<SystemServiceDto>,
    pub(super) usb_devices: Vec<UsbDeviceHistoryDto>,
    pub(super) mounted_devices: Vec<MountedDeviceDto>,
    pub(super) shutdown_times: Vec<ShutdownTimeDto>,
    pub(super) shimcache_entries: Vec<ShimCacheEntryDto>,
    pub(super) run_keys: Vec<RegistryRunKeyDto>,
    pub(super) winlogon_config: Option<WinlogonConfigDto>,
    pub(super) lsa_packages: Vec<LsaPackageDto>,
}

impl SystemRegistryData {
    pub(super) fn load(conn: &Connection) -> Result<Self, AnalysisServiceError> {
        let system_services = load_system_services(conn)?;
        let usb_devices = load_usb_devices(conn)?;
        let mounted_devices = load_mounted_devices(conn)?;
        let shutdown_times = load_shutdown_times(conn)?;
        let shimcache_entries = load_shimcache_entries(conn)?;
        let run_keys = load_run_keys(conn)?;
        let winlogon_config = load_winlogon_config(conn)?;
        let lsa_packages = load_lsa_packages(conn)?;
        Ok(Self {
            system_services,
            usb_devices,
            mounted_devices,
            shutdown_times,
            shimcache_entries,
            run_keys,
            winlogon_config,
            lsa_packages,
        })
    }
}

fn load_system_services(conn: &Connection) -> Result<Vec<SystemServiceDto>, AnalysisServiceError> {
    Ok(
        query_artifact_rows(conn, &["RegistrySystemService"], 0, 10_000)?
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
            .collect(),
    )
}

fn load_usb_devices(conn: &Connection) -> Result<Vec<UsbDeviceHistoryDto>, AnalysisServiceError> {
    Ok(
        query_artifact_rows(conn, &["RegistryUsbDevice"], 0, 10_000)?
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
            .collect(),
    )
}

fn load_mounted_devices(conn: &Connection) -> Result<Vec<MountedDeviceDto>, AnalysisServiceError> {
    Ok(
        query_artifact_rows(conn, &["RegistryMountedDevice"], 0, 10_000)?
            .into_iter()
            .map(|row| MountedDeviceDto {
                device_name: string_attr(&row.attrs, "deviceName"),
                drive_letter: optional_string_attr(&row.attrs, "driveLetter"),
                volume_guid: optional_string_attr(&row.attrs, "volumeGuid"),
                disk_signature_hex: optional_string_attr(&row.attrs, "diskSignatureHex"),
                target_name: optional_string_attr(&row.attrs, "targetName"),
            })
            .collect(),
    )
}

fn load_shutdown_times(conn: &Connection) -> Result<Vec<ShutdownTimeDto>, AnalysisServiceError> {
    Ok(
        query_artifact_rows(conn, &["RegistryShutdownTime"], 0, 10_000)?
            .into_iter()
            .map(|row| ShutdownTimeDto {
                key_path: string_attr(&row.attrs, "keyPath"),
                shutdown_time: string_attr(&row.attrs, "shutdownTime"),
            })
            .collect(),
    )
}

fn load_shimcache_entries(
    conn: &Connection,
) -> Result<Vec<ShimCacheEntryDto>, AnalysisServiceError> {
    Ok(
        query_artifact_rows(conn, &["RegistryShimCache"], 0, 10_000)?
            .into_iter()
            .map(|row| ShimCacheEntryDto {
                path: string_attr(&row.attrs, "path"),
                last_modified: optional_string_attr(&row.attrs, "lastModified"),
                source_key_path: string_attr(&row.attrs, "sourceKeyPath"),
            })
            .collect(),
    )
}

fn load_run_keys(conn: &Connection) -> Result<Vec<RegistryRunKeyDto>, AnalysisServiceError> {
    Ok(
        query_artifact_rows(conn, &["RegistryMachineRunKey"], 0, 10_000)?
            .into_iter()
            .map(|row| RegistryRunKeyDto {
                key_path: string_attr(&row.attrs, "keyPath"),
                value_name: string_attr(&row.attrs, "valueName"),
                command: string_attr(&row.attrs, "command"),
                timestamp: optional_string_attr(&row.attrs, "timestamp"),
                scope: string_attr(&row.attrs, "scope"),
            })
            .collect(),
    )
}

fn load_winlogon_config(
    conn: &Connection,
) -> Result<Option<WinlogonConfigDto>, AnalysisServiceError> {
    Ok(
        query_artifact_rows(conn, &["RegistryWinlogonConfig"], 0, 10_000)?
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
            }),
    )
}

fn load_lsa_packages(conn: &Connection) -> Result<Vec<LsaPackageDto>, AnalysisServiceError> {
    Ok(
        query_artifact_rows(conn, &["RegistryLsaPackage"], 0, 10_000)?
            .into_iter()
            .map(|row| LsaPackageDto {
                control_set: string_attr(&row.attrs, "controlSet"),
                authentication_packages: string_vec_attr(&row.attrs, "authenticationPackages"),
                notification_packages: string_vec_attr(&row.attrs, "notificationPackages"),
                security_packages: string_vec_attr(&row.attrs, "securityPackages"),
            })
            .collect(),
    )
}
