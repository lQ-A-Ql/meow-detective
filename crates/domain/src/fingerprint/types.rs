use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ForensicObjectType {
    DataSource,
    FileEntry,
    Artifact,
    Report,
}

impl ForensicObjectType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DataSource => "data_source",
            Self::FileEntry => "file_entry",
            Self::Artifact => "artifact",
            Self::Report => "report",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "data_source" => Some(Self::DataSource),
            "file_entry" => Some(Self::FileEntry),
            "artifact" => Some(Self::Artifact),
            "report" => Some(Self::Report),
            _ => None,
        }
    }
}
