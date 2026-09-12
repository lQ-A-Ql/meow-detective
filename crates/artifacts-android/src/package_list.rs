use thiserror::Error;

const MAX_PACKAGE_LIST_LINE_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidPackageListRecord {
    pub package_name: String,
    pub uid: Option<u32>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PackageListError {
    #[error("Android packages.list contains a NUL byte")]
    NulByte,
    #[error("Android packages.list line exceeds the supported limit")]
    LineTooLong,
}

/// Parses the stable leading columns of PackageManager's packages.list.
///
/// The package name and UID are retained; later columns are version-dependent
/// policy values and must not be assigned semantics without an AOSP/version
/// specific contract.
pub fn parse_packages_list(
    input: &[u8],
) -> Result<Vec<AndroidPackageListRecord>, PackageListError> {
    if input.contains(&0) {
        return Err(PackageListError::NulByte);
    }
    let text = String::from_utf8_lossy(input);
    let mut records = Vec::new();
    for line in text.lines() {
        if line.len() > MAX_PACKAGE_LIST_LINE_BYTES {
            return Err(PackageListError::LineTooLong);
        }
        let mut columns = line.split_whitespace();
        let Some(package_name) = columns.next() else {
            continue;
        };
        if package_name.starts_with('#') {
            continue;
        }
        records.push(AndroidPackageListRecord {
            package_name: package_name.to_string(),
            uid: columns.next().and_then(|value| value.parse().ok()),
        });
    }
    Ok(records)
}
