use thiserror::Error;

const MAX_PROPERTY_LINE_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidBuildProperty {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BuildPropertyError {
    #[error("Android build property line exceeds the supported limit")]
    LineTooLong,
    #[error("Android build property file contains a NUL byte")]
    NulByte,
}

/// Parses the conservative key=value subset used by Android property files.
///
/// A property file is evidence text, not a shell input: continuation syntax,
/// interpolation, and comments after a value deliberately have no special
/// meaning. Repeated keys are retained in input order so callers can apply a
/// documented source-precedence policy without losing provenance.
pub fn parse_build_properties(
    input: &[u8],
) -> Result<Vec<AndroidBuildProperty>, BuildPropertyError> {
    if input.contains(&0) {
        return Err(BuildPropertyError::NulByte);
    }

    let text = String::from_utf8_lossy(input);
    let mut properties = Vec::new();
    for line in text.lines() {
        if line.len() > MAX_PROPERTY_LINE_BYTES {
            return Err(BuildPropertyError::LineTooLong);
        }
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        properties.push(AndroidBuildProperty {
            key: key.to_string(),
            value: value.trim().to_string(),
        });
    }
    Ok(properties)
}
