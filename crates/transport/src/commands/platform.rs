use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ImportTargetPlatformDto {
    Windows,
    Linux,
    #[serde(other)]
    Unsupported,
}
