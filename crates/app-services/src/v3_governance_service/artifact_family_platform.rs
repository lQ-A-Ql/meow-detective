#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ArtifactFamilyPlatform {
    Windows,
    Linux,
    Unknown,
}

const WINDOWS_FAMILIES: &[&str] = &[
    "LNK",
    "Prefetch",
    "JumpList",
    "RecycleBin",
    "SRU",
    "Thumbcache",
    "ShellBag",
    "AmCache",
    "USBDevice",
    "EventLog",
    "AppCompatCache",
    "MUICache",
    "UserAssist",
    "BAM",
    "ShimCache",
    "NetworkList",
    "ScheduledTask",
    "Service",
    "Startup",
    "MRU",
    "RunMRU",
    "LastVisitedMRU",
    "OfficeMRU",
    "TypedPaths",
    "RecentDocs",
    "ComDlg32",
    "WordWheelQuery",
    "BagMRU",
    "MFT",
    "UsnJrnl",
    "LogonSession",
    "RDPClient",
    "PowerShellHistory",
    "CmdHistory",
    "BrowserDownload",
    "BrowserHistory",
    "BrowserVisit",
    "BrowserCookie",
    "BrowserSessionTab",
    "BrowserPassword",
    "EmailMessage",
];

const LINUX_FAMILIES: &[&str] = &[
    "BashHistory",
    "AuthLog",
    "Syslog",
    "Journald",
    "CronLog",
    "PacmanLog",
    "AptHistory",
    "DpkgLog",
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

pub(super) fn classify_artifact_family(family: &str) -> ArtifactFamilyPlatform {
    if family_starts_with(family, "Registry")
        || family_starts_with(family, "Evtx")
        || family_in(family, WINDOWS_FAMILIES)
    {
        ArtifactFamilyPlatform::Windows
    } else if family_in(family, LINUX_FAMILIES) {
        ArtifactFamilyPlatform::Linux
    } else {
        ArtifactFamilyPlatform::Unknown
    }
}

impl ArtifactFamilyPlatform {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Unknown => "unknown",
        }
    }

    pub(super) const fn matches(self, platform: domain::DataSourcePlatform) -> bool {
        matches!(
            (self, platform),
            (Self::Windows, domain::DataSourcePlatform::Windows)
                | (Self::Linux, domain::DataSourcePlatform::Linux)
        )
    }
}

fn family_in(family: &str, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(family))
}

fn family_starts_with(family: &str, prefix: &str) -> bool {
    family
        .get(..prefix.len())
        .is_some_and(|value| value.eq_ignore_ascii_case(prefix))
}

#[cfg(test)]
#[path = "../../tests/unit/v3_governance/artifact_family_platform_test.rs"]
mod tests;
