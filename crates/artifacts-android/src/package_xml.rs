use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use thiserror::Error;

pub const DEFAULT_MAX_PACKAGES: usize = 100_000;
const MAX_ATTRIBUTE_VALUE_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidPackageMetadata {
    pub package_name: String,
    pub code_path: Option<String>,
    pub installer: Option<String>,
    pub version_code: Option<String>,
    pub uid: Option<u32>,
    pub install_time_millis: Option<i64>,
    pub last_update_time_millis: Option<i64>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PackageXmlError {
    #[error("Android packages.xml is malformed: {0}")]
    InvalidXml(String),
    #[error("Android packages.xml contains more than {limit} packages")]
    TooManyPackages { limit: usize },
    #[error("Android packages.xml attribute exceeds the supported limit")]
    AttributeTooLong,
}

/// Parses PackageManager metadata from either plain XML or AOSP ABX.
///
/// Human-readable labels and launcher icons are APK resource data, not
/// authoritative packages.xml fields. They are therefore intentionally not
/// invented here; callers can attach an APK-derived presentation layer later.
pub fn parse_packages_xml(input: &[u8]) -> Result<Vec<AndroidPackageMetadata>, PackageXmlError> {
    if input.starts_with(&android_abx::MAGIC) {
        return parse_packages_abx(input, DEFAULT_MAX_PACKAGES);
    }
    parse_packages_plain_xml(input, DEFAULT_MAX_PACKAGES)
}

fn parse_packages_plain_xml(
    input: &[u8],
    limit: usize,
) -> Result<Vec<AndroidPackageMetadata>, PackageXmlError> {
    let mut reader = Reader::from_reader(input);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut packages = Vec::new();
    let mut depth = 0usize;

    loop {
        match reader
            .read_event_into(&mut buffer)
            .map_err(|error| PackageXmlError::InvalidXml(error.to_string()))?
        {
            Event::Start(element) => {
                depth = depth.saturating_add(1);
                append_plain_package(&reader, &element, limit, &mut packages)?;
            }
            Event::Empty(element) => append_plain_package(&reader, &element, limit, &mut packages)?,
            Event::End(_) => {
                depth = depth.checked_sub(1).ok_or_else(|| {
                    PackageXmlError::InvalidXml("unexpected end element".to_string())
                })?;
            }
            Event::Eof if depth == 0 => break,
            Event::Eof => {
                return Err(PackageXmlError::InvalidXml(
                    "document ended before closing all elements".to_string(),
                ));
            }
            _ => {}
        }
        buffer.clear();
    }
    Ok(packages)
}

fn parse_packages_abx(
    input: &[u8],
    limit: usize,
) -> Result<Vec<AndroidPackageMetadata>, PackageXmlError> {
    let mut parser = android_abx::AbxParser::new(input)
        .map_err(|error| PackageXmlError::InvalidXml(error.to_string()))?;
    let mut packages = Vec::new();
    while let Some(event) = parser
        .next_event()
        .map_err(|error| PackageXmlError::InvalidXml(error.to_string()))?
    {
        let android_abx::Event::StartTag { name, attributes } = event else {
            continue;
        };
        if name.as_str() != "package" {
            continue;
        }
        append_package(
            attributes
                .iter()
                .map(|attribute| (attribute.name.to_string(), attribute.as_str().into_owned())),
            limit,
            &mut packages,
        )?;
    }
    Ok(packages)
}

fn append_plain_package(
    reader: &Reader<&[u8]>,
    element: &BytesStart<'_>,
    limit: usize,
    packages: &mut Vec<AndroidPackageMetadata>,
) -> Result<(), PackageXmlError> {
    if element.name().as_ref() != b"package" {
        return Ok(());
    }
    let attributes = element
        .attributes()
        .with_checks(false)
        .map(|attribute| {
            let attribute =
                attribute.map_err(|error| PackageXmlError::InvalidXml(error.to_string()))?;
            let value = attribute
                .decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, reader.decoder())
                .map_err(|error| PackageXmlError::InvalidXml(error.to_string()))?
                .into_owned();
            Ok((
                String::from_utf8_lossy(attribute.key.as_ref()).into_owned(),
                value,
            ))
        })
        .collect::<Result<Vec<_>, PackageXmlError>>()?;
    append_package(attributes, limit, packages)
}

fn append_package(
    attributes: impl IntoIterator<Item = (String, String)>,
    limit: usize,
    packages: &mut Vec<AndroidPackageMetadata>,
) -> Result<(), PackageXmlError> {
    if packages.len() >= limit {
        return Err(PackageXmlError::TooManyPackages { limit });
    }
    if let Some(package) = package_from_attributes(attributes)? {
        packages.push(package);
    }
    Ok(())
}

fn package_from_attributes(
    attributes: impl IntoIterator<Item = (String, String)>,
) -> Result<Option<AndroidPackageMetadata>, PackageXmlError> {
    let mut package_name = None;
    let mut code_path = None;
    let mut installer = None;
    let mut version_code = None;
    let mut uid = None;
    let mut first_install_time_millis = None;
    let mut install_time_millis = None;
    let mut last_update_time_millis = None;

    for (key, value) in attributes {
        if value.len() > MAX_ATTRIBUTE_VALUE_BYTES {
            return Err(PackageXmlError::AttributeTooLong);
        }
        match key.as_str() {
            "name" => package_name = non_blank(value),
            "codePath" => code_path = non_blank(value),
            "installer" => installer = non_blank(value),
            "version" | "versionCode" => version_code = non_blank(value),
            "userId" | "sharedUserId" => uid = value.parse::<u32>().ok(),
            "ft" => first_install_time_millis = parse_package_time(&value),
            "it" => install_time_millis = parse_package_time(&value),
            "ut" => last_update_time_millis = parse_package_time(&value),
            _ => {}
        }
    }

    Ok(package_name.map(|package_name| AndroidPackageMetadata {
        package_name,
        code_path,
        installer,
        version_code,
        uid,
        install_time_millis: install_time_millis.or(first_install_time_millis),
        last_update_time_millis,
    }))
}

fn non_blank(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

/// PackageManager serializes timestamps with Long.toHexString; accept decimal
/// too for vendor tools that emit the public millisecond value.
fn parse_package_time(value: &str) -> Option<i64> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let decimal = value.len() >= 13 && value.bytes().all(|byte| byte.is_ascii_digit());
    let radix = if decimal { 10 } else { 16 };
    let value = value.strip_prefix("0x").unwrap_or(value);
    i64::from_str_radix(value, radix)
        .ok()
        .filter(|time| *time >= 0)
}
