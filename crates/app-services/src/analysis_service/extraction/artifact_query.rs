use crate::analysis_service::{candidates::EvidenceCandidate, error::AnalysisServiceError};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::BTreeMap;
use transport::dto::AnalysisParseStatusDto;
mod linux;
mod selection;

pub(super) use linux::{query_linux_kernel_image_versions, query_linux_module_dir_versions};
use selection::{count, host_config, integer, select, Order};

pub(super) struct AnalysisArtifactRow {
    pub(super) id: String,
    pub(super) source_object_id: Option<String>,
    pub(super) extractor_id: Option<String>,
    pub(super) created_at: String,
    pub(super) attrs: BTreeMap<String, Value>,
}

const ANALYSIS_FAMILIES: &[&str] = &[
    "RegistryValue",
    "RegistrySamUser",
    "RegistryUserAssist",
    "RegistryHive",
    "RegistryNetworkAdapter",
    "RegistryNetworkProfile",
    "RegistryInstalledSoftware",
    "RegistryPortableDevice",
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
    "BrowserHistory",
    "BrowserDownload",
    "BrowserCookie",
    "BrowserSessionTab",
    "BrowserPassword",
    "BrowserExtension",
    "EmailMessage",
    "EmailAccount",
    "EvtxBootShutdown",
    "EvtxSecurityEvent",
    "EvtxApplicationEvent",
    "LinuxJournal",
    "LinuxWtmp",
    "LinuxBashCommand",
    "LinuxAptEvent",
    "LinuxCronJob",
    "LinuxSudoEvent",
    "LinuxSystemConfig",
    "LinuxWebSite",
    "LinuxWebAccessLog",
    "LinuxWebErrorLog",
    "LinuxWebFinding",
    "LinuxMysqlConfig",
    "LinuxMysqlLogEntry",
    "LinuxMysqlFinding",
];

pub(super) fn already_has_v1_artifacts(
    conn: &Connection,
    candidate: &EvidenceCandidate,
) -> Result<bool, AnalysisServiceError> {
    let prefix = match candidate.category.as_str() {
        "Registry" => "Registry",
        "BrowserHistory" => "Browser",
        "Email" => "Email",
        "EventLogs" => "Evtx",
        "LinuxArtifacts" => "Linux",
        _ => return Ok(false),
    };
    let families = ANALYSIS_FAMILIES
        .iter()
        .copied()
        .filter(|family| family.starts_with(prefix))
        .collect::<Vec<_>>();
    Ok(count(conn, Some(&families), |artifact| {
        artifact.source_object_id.as_ref() == Some(&candidate.file_id)
            && (candidate.category != "Registry"
                || artifact.extractor_version.as_deref()
                    == Some(crate::analysis_service::capability::REGISTRY_EXTRACTOR_VERSION))
    })? > 0)
}

pub(super) fn count_analysis_artifacts(conn: &Connection) -> Result<u64, AnalysisServiceError> {
    count(conn, Some(ANALYSIS_FAMILIES), |_| true)
}

pub(super) fn count_artifacts_by_type(
    conn: &Connection,
    artifact_type: &str,
) -> Result<u64, AnalysisServiceError> {
    count(conn, Some(&[artifact_type]), |_| true)
}

pub(super) fn count_artifacts_by_type_and_extractor(
    conn: &Connection,
    artifact_type: &str,
    extractor_id: &str,
) -> Result<u64, AnalysisServiceError> {
    count(conn, Some(&[artifact_type]), |artifact| {
        artifact.extractor_id.as_deref() == Some(extractor_id)
    })
}

pub(super) fn count_artifacts_by_type_excluding_extractor(
    conn: &Connection,
    artifact_type: &str,
    extractor_id: &str,
) -> Result<u64, AnalysisServiceError> {
    count(conn, Some(&[artifact_type]), |artifact| {
        artifact.extractor_id.as_deref() != Some(extractor_id)
    })
}

pub(super) fn count_artifacts_by_family_prefix(
    conn: &Connection,
    prefix: &str,
) -> Result<u64, AnalysisServiceError> {
    let prefix = prefix.to_ascii_lowercase();
    count(conn, None, |artifact| {
        artifact.family.to_ascii_lowercase().starts_with(&prefix)
    })
}

pub(super) fn query_artifact_rows(
    conn: &Connection,
    families: &[&str],
    offset: u64,
    limit: u32,
) -> Result<Vec<AnalysisArtifactRow>, AnalysisServiceError> {
    select(conn, families, |_| true, Order::Created, offset, limit)
}

pub(super) fn query_evtx_artifact_rows(
    conn: &Connection,
    families: &[&str],
    offset: u64,
    limit: u32,
) -> Result<Vec<AnalysisArtifactRow>, AnalysisServiceError> {
    select(conn, families, |_| true, Order::Event, offset, limit)
}

pub(super) fn query_evtx_artifact_rows_by_kinds(
    conn: &Connection,
    artifact_type: &str,
    kinds: &[&str],
    offset: u64,
    limit: u32,
) -> Result<Vec<AnalysisArtifactRow>, AnalysisServiceError> {
    select(
        conn,
        &[artifact_type],
        |artifact| {
            artifact
                .attrs
                .get("kind")
                .and_then(Value::as_str)
                .is_some_and(|kind| kinds.contains(&kind))
        },
        Order::Event,
        offset,
        limit,
    )
}

pub(super) fn count_linux_system_config_by_kind(
    conn: &Connection,
    config_kind: &str,
) -> Result<u64, AnalysisServiceError> {
    count(conn, Some(&["LinuxSystemConfig"]), |artifact| {
        host_config(artifact, config_kind)
    })
}

pub(super) fn count_linux_user_accounts(conn: &Connection) -> Result<u64, AnalysisServiceError> {
    count(conn, Some(&["LinuxSystemConfig"]), |artifact| {
        host_config(artifact, "passwdAccount") && integer(artifact.attrs.get("uid")) >= 1000
    })
}

pub(super) fn count_linux_locked_shadow_accounts(
    conn: &Connection,
) -> Result<u64, AnalysisServiceError> {
    count(conn, Some(&["LinuxSystemConfig"]), |artifact| {
        host_config(artifact, "shadowAccount")
            && artifact
                .attrs
                .get("locked")
                .is_some_and(|value| value.as_bool() == Some(true) || value.as_i64() == Some(1))
    })
}

pub(super) fn query_linux_system_config_by_kind(
    conn: &Connection,
    config_kind: &str,
    limit: u32,
) -> Result<Vec<AnalysisArtifactRow>, AnalysisServiceError> {
    query_linux_system_config_by_kind_paged(conn, config_kind, 0, limit)
}

pub(super) fn count_linux_system_config_by_kind_paged(
    conn: &Connection,
    config_kind: &str,
) -> Result<u64, AnalysisServiceError> {
    count_linux_system_config_by_kind(conn, config_kind)
}

pub(super) fn query_linux_system_config_by_kind_paged(
    conn: &Connection,
    config_kind: &str,
    offset: u64,
    limit: u32,
) -> Result<Vec<AnalysisArtifactRow>, AnalysisServiceError> {
    select(
        conn,
        &["LinuxSystemConfig"],
        |artifact| host_config(artifact, config_kind),
        Order::Id,
        offset,
        limit,
    )
}

pub(super) fn query_linux_account_rows(
    conn: &Connection,
) -> Result<Vec<AnalysisArtifactRow>, AnalysisServiceError> {
    select(
        conn,
        &["LinuxSystemConfig"],
        |artifact| host_config(artifact, "passwdAccount") || host_config(artifact, "shadowAccount"),
        Order::Id,
        0,
        u32::MAX,
    )
}

pub(super) fn query_linux_hostname_rows(
    conn: &Connection,
    limit: u32,
) -> Result<Vec<AnalysisArtifactRow>, AnalysisServiceError> {
    select(
        conn,
        &["LinuxSystemConfig"],
        |artifact| {
            artifact
                .attrs
                .get("overlayContext")
                .is_none_or(Value::is_null)
                && artifact
                    .attrs
                    .get("sourcePath")
                    .and_then(Value::as_str)
                    .is_some_and(|path| path == "etc/hostname" || path.ends_with("/etc/hostname"))
        },
        Order::Line,
        0,
        limit,
    )
}

pub(super) fn status_from_total(total: u64) -> AnalysisParseStatusDto {
    if total > 0 {
        AnalysisParseStatusDto::Parsed
    } else {
        AnalysisParseStatusDto::NotFound
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/extraction/artifact_query.rs"]
mod tests;
