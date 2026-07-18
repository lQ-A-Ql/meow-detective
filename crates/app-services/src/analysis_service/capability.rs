use domain::DataSourcePlatform;

use super::error::AnalysisServiceError;
use super::MAX_ANALYSIS_SOURCE_BYTES;

pub(crate) const LINUX_UMBRELLA_KEY: &str = "LinuxArtifacts";
const RETIRED_MACOS_KEY: &str = "MacArtifacts";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CandidateReadPolicy {
    RegistryPreload,
    Bounded(usize),
    LinuxPathAware,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AnalysisCapability {
    pub(crate) key: &'static str,
    pub(crate) platform: DataSourcePlatform,
    pub(crate) section_label: &'static str,
    pub(crate) candidate_category: &'static str,
    pub(crate) read_policy: CandidateReadPolicy,
}

impl AnalysisCapability {
    pub(crate) fn producer_prefix(self) -> &'static str {
        match self.key {
            "Registry" => "registry.",
            "BrowserHistory" => "browser.",
            "Email" => "email.",
            "EventLogs" => "evtx.",
            _ if self.platform == DataSourcePlatform::Linux => "linux.",
            _ => "analysis.",
        }
    }
}

pub(crate) const WINDOWS_CAPABILITIES: &[AnalysisCapability] = &[
    capability(
        "Registry",
        DataSourcePlatform::Windows,
        "Windows Registry",
        "Registry",
        CandidateReadPolicy::RegistryPreload,
    ),
    capability(
        "BrowserHistory",
        DataSourcePlatform::Windows,
        "Browser History",
        "BrowserHistory",
        CandidateReadPolicy::Bounded(MAX_ANALYSIS_SOURCE_BYTES),
    ),
    capability(
        "Email",
        DataSourcePlatform::Windows,
        "Email",
        "Email",
        CandidateReadPolicy::Bounded(MAX_ANALYSIS_SOURCE_BYTES),
    ),
    capability(
        "EventLogs",
        DataSourcePlatform::Windows,
        "Windows Event Logs",
        "EventLogs",
        CandidateReadPolicy::Bounded(MAX_ANALYSIS_SOURCE_BYTES),
    ),
];

pub(crate) const LINUX_CAPABILITIES: &[AnalysisCapability] = &[
    linux_capability("LinuxJournal", "Linux 日志"),
    linux_capability("LinuxLogin", "Linux 登录记录"),
    linux_capability("LinuxCommands", "Linux 命令历史"),
    linux_capability("LinuxPackages", "Linux 软件包记录"),
    linux_capability("LinuxCron", "Linux 计划任务"),
    linux_capability("LinuxSudo", "Linux sudo/auth"),
    linux_capability("LinuxSystemConfig", "Linux 系统配置"),
    linux_capability("LinuxWebServices", "Linux Web 服务"),
    linux_capability("LinuxMysqlServices", "Linux MySQL Services"),
];

const fn capability(
    key: &'static str,
    platform: DataSourcePlatform,
    section_label: &'static str,
    candidate_category: &'static str,
    read_policy: CandidateReadPolicy,
) -> AnalysisCapability {
    AnalysisCapability {
        key,
        platform,
        section_label,
        candidate_category,
        read_policy,
    }
}

const fn linux_capability(key: &'static str, section_label: &'static str) -> AnalysisCapability {
    capability(
        key,
        DataSourcePlatform::Linux,
        section_label,
        LINUX_UMBRELLA_KEY,
        CandidateReadPolicy::LinuxPathAware,
    )
}

pub(crate) fn select_capabilities(
    platform: DataSourcePlatform,
    available: &'static [AnalysisCapability],
    requested: &[&str],
) -> Result<Vec<AnalysisCapability>, AnalysisServiceError> {
    if requested.is_empty() {
        return Ok(available.to_vec());
    }

    let mut selected = Vec::new();
    for raw_key in requested {
        let key = raw_key.trim();
        reject_retired_or_blank_key(key)?;
        if key == LINUX_UMBRELLA_KEY {
            ensure_platform_match(key, platform, DataSourcePlatform::Linux)?;
            append_unique(&mut selected, LINUX_CAPABILITIES.iter().copied());
            continue;
        }

        let capability = find_capability(key).ok_or_else(|| {
            AnalysisServiceError::InvalidInput(format!("unknown analysis capability `{key}`"))
        })?;
        ensure_platform_match(key, platform, capability.platform)?;
        append_unique(&mut selected, std::iter::once(capability));
    }
    Ok(selected)
}

pub(crate) fn find_capability(key: &str) -> Option<AnalysisCapability> {
    WINDOWS_CAPABILITIES
        .iter()
        .chain(LINUX_CAPABILITIES)
        .find(|capability| capability.key == key)
        .copied()
}

pub(crate) fn reject_retired_or_blank_key(key: &str) -> Result<(), AnalysisServiceError> {
    if key.eq_ignore_ascii_case(RETIRED_MACOS_KEY) {
        return Err(AnalysisServiceError::Unsupported(
            RETIRED_MACOS_KEY.to_string(),
        ));
    }
    if key.is_empty() {
        return Err(AnalysisServiceError::InvalidInput(
            "analysis capability must not be blank".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn ensure_platform_match(
    key: &str,
    source_platform: DataSourcePlatform,
    capability_platform: DataSourcePlatform,
) -> Result<(), AnalysisServiceError> {
    if source_platform == capability_platform {
        return Ok(());
    }
    Err(AnalysisServiceError::platform_mismatch(
        key,
        source_platform,
        capability_platform,
    ))
}

fn append_unique(
    selected: &mut Vec<AnalysisCapability>,
    capabilities: impl IntoIterator<Item = AnalysisCapability>,
) {
    for capability in capabilities {
        if !selected.iter().any(|item| item.key == capability.key) {
            selected.push(capability);
        }
    }
}
