use std::fmt;
use std::str::FromStr;

use thiserror::Error;

/// Operating-system family persisted for a data source.
///
/// The canonical storage representation is deliberately independent from
/// transport DTOs so application services can dispatch on a domain type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataSourcePlatform {
    Windows,
    Linux,
    Unknown,
}

/// Validation failures for persisted or explicitly supplied platform values.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DataSourcePlatformParseError {
    #[error("data source platform must be specified as `windows` or `linux`")]
    MissingExplicitValue,
    #[error("data source platform `unknown` is not valid for explicit input")]
    UnknownExplicitValue,
    #[error("unsupported data source platform `{value}`")]
    UnsupportedValue { value: String },
}

impl DataSourcePlatform {
    /// Returns the canonical value used by persistence adapters.
    pub const fn as_storage_str(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::Unknown => "unknown",
        }
    }

    /// Parses a nullable database value without accepting retired platforms.
    ///
    /// A missing or blank database value represents unavailable platform
    /// metadata. Non-empty values are trimmed and matched case-insensitively,
    /// then normalized through [`Self::as_storage_str`].
    pub fn from_storage_str(value: Option<&str>) -> Result<Self, DataSourcePlatformParseError> {
        let Some(value) = value else {
            return Ok(Self::Unknown);
        };

        if value.trim().is_empty() {
            return Ok(Self::Unknown);
        }

        value.parse()
    }

    /// Parses a platform selected explicitly for an import or analysis request.
    ///
    /// Explicit callers must choose a supported platform. `Unknown` is reserved
    /// for absent persisted metadata and therefore fails closed here.
    pub fn parse_explicit(value: &str) -> Result<Self, DataSourcePlatformParseError> {
        match value.parse()? {
            Self::Unknown => Err(DataSourcePlatformParseError::UnknownExplicitValue),
            platform => Ok(platform),
        }
    }
}

impl FromStr for DataSourcePlatform {
    type Err = DataSourcePlatformParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        if value.is_empty() {
            return Err(DataSourcePlatformParseError::MissingExplicitValue);
        }

        if value.eq_ignore_ascii_case("windows") {
            Ok(Self::Windows)
        } else if value.eq_ignore_ascii_case("linux") {
            Ok(Self::Linux)
        } else if value.eq_ignore_ascii_case("unknown") {
            Ok(Self::Unknown)
        } else {
            Err(DataSourcePlatformParseError::UnsupportedValue {
                value: value.to_owned(),
            })
        }
    }
}

impl fmt::Display for DataSourcePlatform {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_storage_str())
    }
}
