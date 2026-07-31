#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimelineEventKind {
    RegistryHiveLastWrite,
    RegistrySamLastLogin,
    RegistrySamPasswordLastSet,
    RegistrySystemShutdown,
    FileCreated,
    FileModified,
    FileAccessed,
    FileExecuted,
    FileDeleted,
}

impl TimelineEventKind {
    pub const ALL: [Self; 9] = [
        Self::RegistryHiveLastWrite,
        Self::RegistrySamLastLogin,
        Self::RegistrySamPasswordLastSet,
        Self::RegistrySystemShutdown,
        Self::FileCreated,
        Self::FileModified,
        Self::FileAccessed,
        Self::FileExecuted,
        Self::FileDeleted,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RegistryHiveLastWrite => "REGISTRY_HIVE_LAST_WRITE",
            Self::RegistrySamLastLogin => "REGISTRY_SAM_LAST_LOGIN",
            Self::RegistrySamPasswordLastSet => "REGISTRY_SAM_PASSWORD_LAST_SET",
            Self::RegistrySystemShutdown => "REGISTRY_SYSTEM_SHUTDOWN",
            Self::FileCreated => "FILE_CREATED",
            Self::FileModified => "FILE_MODIFIED",
            Self::FileAccessed => "FILE_ACCESSED",
            Self::FileExecuted => "FILE_EXECUTED",
            Self::FileDeleted => "FILE_DELETED",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|event_kind| event_kind.as_str() == value)
    }

    pub const fn is_registry(self) -> bool {
        matches!(
            self,
            Self::RegistryHiveLastWrite
                | Self::RegistrySamLastLogin
                | Self::RegistrySamPasswordLastSet
                | Self::RegistrySystemShutdown
        )
    }

    pub const fn is_analysis_event(self) -> bool {
        self.is_registry() || matches!(self, Self::FileExecuted)
    }
}

impl std::fmt::Display for TimelineEventKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}
