mod activity;
mod core;
mod security;
mod system;

use self::activity::ActivityRegistryData;
use self::core::CoreRegistryData;
use self::security::SecurityRegistryData;
use self::system::SystemRegistryData;
use crate::analysis_service::error::AnalysisServiceError;
use chrono::Utc;
use rusqlite::Connection;
use transport::dto::{AnalysisParseStatusDto, RegistryStructuredSummaryDto};

pub(super) fn load_registry_structured_summary(
    conn: &Connection,
) -> Result<RegistryStructuredSummaryDto, AnalysisServiceError> {
    let core = CoreRegistryData::load(conn)?;
    let system = SystemRegistryData::load(conn)?;
    let activity = ActivityRegistryData::load(conn)?;
    let security = SecurityRegistryData::load(conn)?;
    let status = structured_status(&core, &system, &activity, &security);

    Ok(RegistryStructuredSummaryDto {
        hive_overviews: core.hive_overviews,
        sam_users: core.sam_users,
        user_assist_entries: core.user_assist_entries,
        network_profiles: core.network_profiles,
        installed_software: core.installed_software,
        usb_devices: system.usb_devices,
        mounted_devices: system.mounted_devices,
        system_services: system.system_services,
        shutdown_times: system.shutdown_times,
        shimcache_entries: system.shimcache_entries,
        run_keys: system.run_keys,
        open_save_mru: activity.open_save_mru,
        last_visited_mru: activity.last_visited_mru,
        run_mru: activity.run_mru,
        shellbag_entries: activity.shellbag_entries,
        muicache_entries: activity.muicache_entries,
        amcache_applications: activity.amcache_applications,
        amcache_application_files: activity.amcache_application_files,
        winlogon_config: system.winlogon_config,
        lsa_packages: system.lsa_packages,
        appcompat_layers: activity.appcompat_layers,
        security_policies: security.security_policies,
        lsa_secrets: security.lsa_secrets,
        cached_credentials: security.cached_credentials,
        status,
        generated_at: Utc::now().to_rfc3339(),
        warnings: Vec::new(),
    })
}

fn structured_status(
    core: &CoreRegistryData,
    system: &SystemRegistryData,
    activity: &ActivityRegistryData,
    security: &SecurityRegistryData,
) -> AnalysisParseStatusDto {
    if core.sam_users.is_empty()
        && core.user_assist_entries.is_empty()
        && core.hive_overviews.is_empty()
        && core.network_profiles.is_empty()
        && system.usb_devices.is_empty()
        && system.mounted_devices.is_empty()
        && system.shutdown_times.is_empty()
        && system.shimcache_entries.is_empty()
        && system.run_keys.is_empty()
        && activity.open_save_mru.is_empty()
        && activity.last_visited_mru.is_empty()
        && activity.run_mru.is_empty()
        && activity.shellbag_entries.is_empty()
        && activity.muicache_entries.is_empty()
        && activity.amcache_applications.is_empty()
        && activity.amcache_application_files.is_empty()
        && system.winlogon_config.is_none()
        && system.lsa_packages.is_empty()
        && activity.appcompat_layers.is_empty()
        && security.security_policies.is_empty()
        && security.lsa_secrets.is_empty()
        && security.cached_credentials.is_empty()
    {
        AnalysisParseStatusDto::NotFound
    } else {
        AnalysisParseStatusDto::Parsed
    }
}
