use std::fmt;

use crate::MountError;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MountPath(String);

impl MountPath {
    pub fn root() -> Self {
        Self("/".to_string())
    }

    pub fn parse(value: &str) -> Result<Self, MountError> {
        if value.contains('\0') {
            return Err(MountError::InvalidPath("NUL is not allowed".to_string()));
        }
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed == "/" || trimmed == "\\" {
            return Ok(Self::root());
        }

        let mut segments = Vec::new();
        for segment in trimmed.split(['/', '\\']) {
            if segment.is_empty() {
                continue;
            }
            if segment == ".." {
                return Err(MountError::PathTraversal);
            }
            if segment == "." {
                return Err(MountError::PathTraversal);
            }
            if !is_windows_representable_component(segment) {
                return Err(MountError::InvalidPath(format!(
                    "path component cannot be represented by the Windows mount: {segment}"
                )));
            }
            segments.push(segment);
        }
        if segments.is_empty() {
            return Ok(Self::root());
        }
        Ok(Self(format!("/{}", segments.join("/"))))
    }

    pub fn is_root(&self) -> bool {
        self.0 == "/"
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn file_name(&self) -> &str {
        self.0.rsplit('/').next().unwrap_or("")
    }
}

fn is_windows_representable_component(component: &str) -> bool {
    if component.is_empty()
        || component.ends_with('.')
        || component.ends_with(' ')
        || component.chars().any(|character| {
            matches!(
                character,
                '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
            )
        })
    {
        return false;
    }

    let upper = component.to_ascii_uppercase();
    let stem = upper.split('.').next().unwrap_or(&upper);
    !matches!(
        stem,
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

impl fmt::Display for MountPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
