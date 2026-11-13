use super::common::{
    evidence_path_matches, normalize_evidence_path, EvidenceCandidate, EvidenceCategoryDef,
    EvidencePathPattern,
};
use crate::analysis_service::error::AnalysisServiceError;
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::analysis_scan_repo::AnalysisScanRepo;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct LinuxArtifactCatalogCandidate {
    pub file_id: String,
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Default)]
pub(crate) struct LinuxArtifactCatalogSummary {
    pub candidates: Vec<LinuxArtifactCatalogCandidate>,
    pub kernel_image_versions: Vec<String>,
    pub kernel_module_versions: Vec<String>,
}

#[derive(Clone, Default, Deserialize, Serialize)]
pub(crate) struct LinuxKernelCatalog {
    pub(crate) image_versions: Vec<String>,
    pub(crate) module_versions: Vec<String>,
}

#[derive(Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub(crate) struct LinuxCatalogMetadata {
    pub(crate) candidates: Vec<LinuxArtifactCatalogCandidate>,
    pub(crate) kernel_image_versions: Vec<String>,
    pub(crate) kernel_module_versions: Vec<String>,
}

impl LinuxCatalogMetadata {
    pub(crate) fn from_discovery(
        candidates: &[EvidenceCandidate],
        kernel: Option<LinuxKernelCatalog>,
    ) -> Self {
        let kernel = kernel.unwrap_or_default();
        Self {
            candidates: candidates
                .iter()
                .filter(|candidate| candidate.category == "LinuxArtifacts")
                .map(|candidate| LinuxArtifactCatalogCandidate {
                    file_id: candidate.file_id.0.clone(),
                    path: candidate.path.clone(),
                    size: candidate.size,
                })
                .collect(),
            kernel_image_versions: kernel.image_versions,
            kernel_module_versions: kernel.module_versions,
        }
    }
}

pub(crate) fn collect_linux_artifact_catalog(
    conn: &Connection,
) -> Result<LinuxArtifactCatalogSummary, AnalysisServiceError> {
    if let Some(summary) = collect_linux_artifact_catalog_from_checkpoints(conn)? {
        return Ok(summary);
    }
    collect_linux_artifact_catalog_uncached(conn)
}

fn collect_linux_artifact_catalog_from_checkpoints(
    conn: &Connection,
) -> Result<Option<LinuxArtifactCatalogSummary>, AnalysisServiceError> {
    let metadata = persistence_sqlite::repositories::source_meta_repo::SourceMetaRepo::new(conn)
        .read(LINUX_CATALOG_METADATA_KEY)?
        .map(|value| serde_json::from_str::<LinuxCatalogMetadata>(&value))
        .transpose()
        .map_err(|error| {
            AnalysisServiceError::Other(format!("decode Linux catalog metadata: {error}"))
        })?;
    if let Some(metadata) = metadata {
        return Ok(Some(LinuxArtifactCatalogSummary {
            candidates: metadata.candidates,
            kernel_image_versions: metadata.kernel_image_versions,
            kernel_module_versions: metadata.kernel_module_versions,
        }));
    }
    let repository = AnalysisScanRepo::new(conn);
    let mut candidates = HashMap::<String, LinuxArtifactCatalogCandidate>::new();
    let mut seen = HashSet::new();
    for version in crate::analysis_service::capability::CHECKPOINT_VERSIONS {
        for scan in repository.list_clean_for_version(version)? {
            if scan.capability_key.starts_with("Linux")
                && seen.insert(scan.source_object_id.clone())
            {
                candidates.insert(
                    scan.source_object_id.clone(),
                    LinuxArtifactCatalogCandidate {
                        file_id: scan.source_object_id,
                        path: String::new(),
                        size: scan.source_size,
                    },
                );
            }
        }
        for scan in repository.list_diagnostics_for_version(version)? {
            if scan.capability_key.starts_with("Linux")
                && seen.insert(scan.source_object_id.clone())
            {
                candidates.insert(
                    scan.source_object_id.clone(),
                    LinuxArtifactCatalogCandidate {
                        file_id: scan.source_object_id,
                        path: String::new(),
                        size: scan.source_size,
                    },
                );
            }
        }
        for scan in repository.list_complete_for_version(version)? {
            if scan.capability_key.starts_with("Linux")
                && seen.insert(scan.source_object_id.clone())
            {
                candidates.insert(
                    scan.source_object_id.clone(),
                    LinuxArtifactCatalogCandidate {
                        file_id: scan.source_object_id,
                        path: String::new(),
                        size: scan.source_size,
                    },
                );
            }
        }
    }
    if candidates.is_empty() {
        return Ok(None);
    }
    let mut candidates = candidates.into_values().collect::<Vec<_>>();
    candidates.sort_by(|left, right| left.file_id.cmp(&right.file_id));
    Ok(Some(LinuxArtifactCatalogSummary {
        candidates,
        kernel_image_versions: Vec::new(),
        kernel_module_versions: Vec::new(),
    }))
}

fn collect_linux_artifact_catalog_uncached(
    conn: &Connection,
) -> Result<LinuxArtifactCatalogSummary, AnalysisServiceError> {
    let mut summary = LinuxArtifactCatalogSummary::default();
    let mut kernel = LinuxKernelCatalog::default();
    FileRepo::new(conn).visit_all(None, |entry| {
        let normalized = normalize_evidence_path(&entry.path);
        observe_kernel_paths(&normalized, &mut kernel);
        if entry.is_file()
            && evidence_path_matches(&normalized, LINUX_ARTIFACTS_CATEGORY_DEF.patterns)
        {
            summary.candidates.push(LinuxArtifactCatalogCandidate {
                file_id: entry.id.0,
                path: entry.path,
                size: entry.size.unwrap_or_default(),
            });
        }
        Ok(())
    })?;
    summary.kernel_image_versions = kernel.image_versions;
    summary.kernel_module_versions = kernel.module_versions;
    Ok(summary)
}

pub(crate) const LINUX_CATALOG_METADATA_KEY: &str = "linux.summary.catalog:v1";

pub(super) fn observe_kernel_paths(path: &str, catalog: &mut LinuxKernelCatalog) {
    if let Some(version) = path
        .strip_prefix("/boot/vmlinuz-")
        .or_else(|| path.strip_prefix("/vmlinuz-"))
        .filter(|version| is_kernel_version_candidate(version))
    {
        catalog.image_versions.push(version.to_string());
    }
    if let Some(version) = path
        .strip_prefix("/lib/modules/")
        .or_else(|| path.strip_prefix("/usr/lib/modules/"))
        .and_then(|suffix| suffix.split('/').next())
        .filter(|version| is_kernel_version_candidate(version))
    {
        catalog.module_versions.push(version.to_string());
    }
}

fn is_kernel_version_candidate(value: &str) -> bool {
    !value.is_empty()
        && value.starts_with(|character: char| character.is_ascii_digit())
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".+-_~".contains(character))
}

pub(super) const LINUX_ARTIFACTS_CATEGORY_DEF: EvidenceCategoryDef = EvidenceCategoryDef {
    category: "LinuxArtifacts",
    display_name: "Linux 痕迹",
    evidence_kind: "linux_artifact",
    parser: "linux.artifacts",
    artifact_families: &[
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
        "GitRepository",
        "GitLabArtifact",
    ],
    patterns: &[
        EvidencePathPattern::Suffix("/etc/os-release"),
        EvidencePathPattern::Suffix("/usr/lib/os-release"),
        EvidencePathPattern::Suffix("/etc/passwd"),
        EvidencePathPattern::Suffix("/etc/shadow"),
        EvidencePathPattern::Suffix("/etc/gshadow"),
        EvidencePathPattern::Suffix("/etc/group"),
        EvidencePathPattern::Suffix("/etc/hostname"),
        EvidencePathPattern::Suffix("/etc/debian_version"),
        EvidencePathPattern::Suffix("/etc/pve/.version"),
        EvidencePathPattern::Suffix("/etc/hosts"),
        EvidencePathPattern::Suffix("/etc/fstab"),
        EvidencePathPattern::Suffix("/etc/resolv.conf"),
        EvidencePathPattern::Suffix("/etc/machine-id"),
        EvidencePathPattern::Suffix("/etc/login.defs"),
        EvidencePathPattern::Suffix("/etc/anacrontab"),
        EvidencePathPattern::Suffix("/etc/pve/storage.cfg"),
        EvidencePathPattern::ContainsAndSuffix("/etc/pve/qemu-server/", ".conf"),
        EvidencePathPattern::ContainsAndSuffix("/etc/pve/lxc/", ".conf"),
        EvidencePathPattern::Suffix("/etc/pve/corosync.conf"),
        EvidencePathPattern::Suffix("/etc/corosync/corosync.conf"),
        EvidencePathPattern::Suffix(".journal"),
        EvidencePathPattern::Suffix(".journal~"),
        EvidencePathPattern::Contains("/var/log/journal/"),
        EvidencePathPattern::Contains("/run/log/journal/"),
        EvidencePathPattern::Suffix("/var/log/wtmp"),
        EvidencePathPattern::Contains("/var/log/wtmp."),
        EvidencePathPattern::Suffix("/var/log/btmp"),
        EvidencePathPattern::Contains("/var/log/btmp."),
        EvidencePathPattern::Suffix("/var/log/utmp"),
        EvidencePathPattern::Suffix("/run/utmp"),
        EvidencePathPattern::Suffix("/var/log/lastlog"),
        EvidencePathPattern::Suffix("/var/log/faillog"),
        EvidencePathPattern::Suffix(".bash_history"),
        EvidencePathPattern::Suffix(".zsh_history"),
        EvidencePathPattern::Suffix(".fish_history"),
        EvidencePathPattern::Suffix(".python_history"),
        EvidencePathPattern::Contains("/.local/share/fish/fish_history"),
        EvidencePathPattern::Contains("/var/log/apt/history.log"),
        EvidencePathPattern::Contains("/var/log/apt/history.log."),
        EvidencePathPattern::Contains("/var/log/dpkg.log"),
        EvidencePathPattern::Contains("/var/log/dpkg.log."),
        EvidencePathPattern::Suffix("/var/log/yum.log"),
        EvidencePathPattern::Contains("/var/log/yum.log."),
        EvidencePathPattern::Suffix("/var/log/dnf.log"),
        EvidencePathPattern::Contains("/var/log/dnf.log."),
        EvidencePathPattern::Suffix("/var/log/dnf.rpm.log"),
        EvidencePathPattern::Contains("/var/log/dnf.rpm.log."),
        EvidencePathPattern::Suffix("/etc/crontab"),
        EvidencePathPattern::Contains("/etc/cron.d/"),
        EvidencePathPattern::Contains("/etc/cron.daily/"),
        EvidencePathPattern::Contains("/etc/cron.hourly/"),
        EvidencePathPattern::Contains("/etc/cron.monthly/"),
        EvidencePathPattern::Contains("/etc/cron.weekly/"),
        EvidencePathPattern::Contains("/var/spool/cron/crontabs/"),
        EvidencePathPattern::Contains("/var/spool/cron/"),
        EvidencePathPattern::Suffix("/var/log/auth.log"),
        EvidencePathPattern::Contains("/var/log/auth.log."),
        EvidencePathPattern::Suffix("/var/log/secure"),
        EvidencePathPattern::Contains("/var/log/secure."),
        EvidencePathPattern::Suffix("/var/log/audit/audit.log"),
        EvidencePathPattern::Contains("/var/log/audit/audit.log."),
        EvidencePathPattern::Suffix("/var/log/syslog"),
        EvidencePathPattern::Contains("/var/log/syslog."),
        EvidencePathPattern::Suffix("/var/log/messages"),
        EvidencePathPattern::Contains("/var/log/messages."),
        EvidencePathPattern::Suffix("/var/log/kern.log"),
        EvidencePathPattern::Contains("/var/log/kern.log."),
        EvidencePathPattern::Suffix("/var/log/cloud-init.log"),
        EvidencePathPattern::Contains("/var/log/cloud-init.log."),
        EvidencePathPattern::Suffix("/var/log/cron"),
        EvidencePathPattern::Contains("/var/log/cron."),
        EvidencePathPattern::Suffix("/var/log/daemon.log"),
        EvidencePathPattern::Contains("/var/log/daemon.log."),
        EvidencePathPattern::Suffix("/var/log/mail.log"),
        EvidencePathPattern::Contains("/var/log/mail.log."),
        EvidencePathPattern::Suffix("/var/log/maillog"),
        EvidencePathPattern::Contains("/var/log/maillog."),
        EvidencePathPattern::Suffix("/var/log/ufw.log"),
        EvidencePathPattern::Contains("/var/log/ufw.log."),
        EvidencePathPattern::Suffix("/var/log/fail2ban.log"),
        EvidencePathPattern::Contains("/var/log/fail2ban.log."),
        EvidencePathPattern::Suffix("/var/log/pveproxy/access.log"),
        EvidencePathPattern::Contains("/var/log/pveproxy/access.log."),
        EvidencePathPattern::Suffix("/var/log/pvedaemon.log"),
        EvidencePathPattern::Contains("/var/log/pvedaemon.log."),
        EvidencePathPattern::Contains("/var/log/pve/tasks/"),
        EvidencePathPattern::Contains("/.ssh/authorized_keys"),
        EvidencePathPattern::Contains("/.ssh/known_hosts"),
        EvidencePathPattern::Contains("/etc/ssh/"),
        EvidencePathPattern::Contains("/etc/systemd/system/"),
        EvidencePathPattern::Contains("/etc/systemd/system.control/"),
        EvidencePathPattern::Contains("/etc/systemd/system.attached/"),
        EvidencePathPattern::Contains("/run/systemd/"),
        EvidencePathPattern::Contains("/usr/local/lib/systemd/system/"),
        EvidencePathPattern::Contains("/lib/systemd/system/"),
        EvidencePathPattern::Contains("/usr/lib/systemd/system/"),
        EvidencePathPattern::ContainsAndSuffix("/var/lib/docker/containers/", "/config.v2.json"),
        EvidencePathPattern::ContainsAndSuffix("/var/lib/docker/image/", "/repositories.json"),
        EvidencePathPattern::Suffix("/io.containerd.metadata.v1.bolt/meta.db"),
        EvidencePathPattern::Contains("/etc/init.d/"),
        EvidencePathPattern::Suffix("/etc/rc.local"),
        EvidencePathPattern::Contains("/etc/profile.d/"),
        EvidencePathPattern::Suffix("/etc/sudoers"),
        EvidencePathPattern::Contains("/etc/sudoers.d/"),
        EvidencePathPattern::Suffix("/etc/nginx/nginx.conf"),
        EvidencePathPattern::ContainsAndSuffix("/etc/nginx/conf.d/", ".conf"),
        EvidencePathPattern::Contains("/etc/nginx/sites-enabled/"),
        EvidencePathPattern::Suffix("/etc/apache2/apache2.conf"),
        EvidencePathPattern::Contains("/etc/apache2/sites-enabled/"),
        EvidencePathPattern::Suffix("/etc/httpd/conf/httpd.conf"),
        EvidencePathPattern::ContainsAndSuffix("/etc/httpd/conf.d/", ".conf"),
        EvidencePathPattern::ContainsAndSuffix("/var/log/nginx/", ".log"),
        EvidencePathPattern::ContainsBoth("/var/log/nginx/", ".log."),
        EvidencePathPattern::ContainsAndSuffix("/var/log/apache2/", ".log"),
        EvidencePathPattern::ContainsBoth("/var/log/apache2/", ".log."),
        EvidencePathPattern::ContainsAndSuffix("/var/log/httpd/", ".log"),
        EvidencePathPattern::ContainsBoth("/var/log/httpd/", ".log."),
        // BaoTa (宝塔) panel layout: vhost site configs, panel-managed server
        // main configs, and the centralized /www/wwwlogs web log directory.
        EvidencePathPattern::ContainsAndSuffix("/www/server/panel/vhost/nginx/", ".conf"),
        EvidencePathPattern::ContainsAndSuffix("/www/server/panel/vhost/apache/", ".conf"),
        EvidencePathPattern::ContainsAndSuffix("/www/server/nginx/conf/", ".conf"),
        EvidencePathPattern::ContainsAndSuffix("/www/server/apache/conf/", ".conf"),
        EvidencePathPattern::ContainsAndSuffix("/www/wwwlogs/", ".log"),
        EvidencePathPattern::ContainsBoth("/www/wwwlogs/", ".log."),
        EvidencePathPattern::ContainsAndSuffix("/var/www/", ".php"),
        EvidencePathPattern::ContainsAndSuffix("/var/www/", ".phtml"),
        EvidencePathPattern::ContainsAndSuffix("/var/www/", ".jsp"),
        EvidencePathPattern::ContainsAndSuffix("/var/www/", ".jspx"),
        EvidencePathPattern::ContainsAndSuffix("/usr/share/nginx/html/", ".php"),
        EvidencePathPattern::ContainsAndSuffix("/usr/share/nginx/html/", ".phtml"),
        EvidencePathPattern::ContainsAndSuffix("/usr/share/nginx/html/", ".jsp"),
        EvidencePathPattern::ContainsAndSuffix("/usr/share/nginx/html/", ".jspx"),
        EvidencePathPattern::Suffix("/etc/my.cnf"),
        EvidencePathPattern::Suffix("/etc/mysql/my.cnf"),
        EvidencePathPattern::ContainsAndSuffix("/etc/mysql/mysql.conf.d/", ".cnf"),
        EvidencePathPattern::ContainsAndSuffix("/etc/mysql/mariadb.conf.d/", ".cnf"),
        EvidencePathPattern::ContainsAndSuffix("/etc/mysql/conf.d/", ".cnf"),
        EvidencePathPattern::ContainsAndSuffix("/etc/my.cnf.d/", ".cnf"),
        EvidencePathPattern::Suffix("/var/log/mysql/error.log"),
        EvidencePathPattern::Contains("/var/log/mysql/error.log."),
        EvidencePathPattern::Suffix("/var/log/mysql/mysql.log"),
        EvidencePathPattern::Contains("/var/log/mysql/mysql.log."),
        EvidencePathPattern::Suffix("/var/log/mysql/mysql-slow.log"),
        EvidencePathPattern::Contains("/var/log/mysql/mysql-slow.log."),
        EvidencePathPattern::Suffix("/var/log/mysql/slow.log"),
        EvidencePathPattern::Contains("/var/log/mysql/slow.log."),
        EvidencePathPattern::Suffix("/var/log/mariadb/mariadb.log"),
        EvidencePathPattern::Contains("/var/log/mariadb/mariadb.log."),
        EvidencePathPattern::Suffix("/var/log/mysqld.log"),
        EvidencePathPattern::Contains("/var/log/mysqld.log."),
        EvidencePathPattern::Contains("/.git/"),
        EvidencePathPattern::Contains("/etc/gitlab/"),
        EvidencePathPattern::Contains("/var/log/gitlab/"),
    ],
    matcher: None,
};

#[cfg(test)]
#[path = "../../../tests/unit/analysis_service/candidates/linux.rs"]
mod tests;
