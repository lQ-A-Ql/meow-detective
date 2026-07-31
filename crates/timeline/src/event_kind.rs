#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimelineEventKind {
    RegistryHiveLastWrite,
    RegistryUserAssistLastRun,
    RegistrySamLastLogin,
    RegistrySamPasswordLastSet,
    RegistrySystemShutdown,
    FileModified,
}

impl TimelineEventKind {
    pub const ALL: [Self; 6] = [
        Self::RegistryHiveLastWrite,
        Self::RegistryUserAssistLastRun,
        Self::RegistrySamLastLogin,
        Self::RegistrySamPasswordLastSet,
        Self::RegistrySystemShutdown,
        Self::FileModified,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RegistryHiveLastWrite => "REGISTRY_HIVE_LAST_WRITE",
            Self::RegistryUserAssistLastRun => "REGISTRY_USER_ASSIST_LAST_RUN",
            Self::RegistrySamLastLogin => "REGISTRY_SAM_LAST_LOGIN",
            Self::RegistrySamPasswordLastSet => "REGISTRY_SAM_PASSWORD_LAST_SET",
            Self::RegistrySystemShutdown => "REGISTRY_SYSTEM_SHUTDOWN",
            Self::FileModified => "FILE_MODIFIED",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|event_kind| event_kind.as_str() == value)
    }

    pub const fn is_registry(self) -> bool {
        !matches!(self, Self::FileModified)
    }
}

impl std::fmt::Display for TimelineEventKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}
