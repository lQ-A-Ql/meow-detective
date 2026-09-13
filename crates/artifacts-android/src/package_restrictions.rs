use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use thiserror::Error;

const MAX_ATTRIBUTE_VALUE_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidPackageRestriction {
    pub package_name: String,
    pub installed: Option<bool>,
    pub enabled_state: Option<i32>,
    pub hidden: Option<bool>,
    pub stopped: Option<bool>,
    pub suspended: Option<bool>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum PackageRestrictionsError {
    #[error("Android package restrictions XML is malformed: {0}")]
    InvalidXml(String),
    #[error("Android package restrictions attribute exceeds the supported limit")]
    AttributeTooLong,
}

/// Parses per-user PackageManager restriction facts from plain XML or ABX.
pub fn parse_package_restrictions(
    input: &[u8],
) -> Result<Vec<AndroidPackageRestriction>, PackageRestrictionsError> {
    if input.starts_with(&android_abx::MAGIC) {
        return parse_abx_restrictions(input);
    }
    parse_plain_restrictions(input)
}

fn parse_plain_restrictions(
    input: &[u8],
) -> Result<Vec<AndroidPackageRestriction>, PackageRestrictionsError> {
    let mut reader = Reader::from_reader(input);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut restrictions = Vec::new();
    let mut stack = Vec::<Vec<u8>>::new();
    loop {
        match reader
            .read_event_into(&mut buffer)
            .map_err(|error| PackageRestrictionsError::InvalidXml(error.to_string()))?
        {
            Event::Start(element) => {
                if element.name().as_ref() == b"pkg" {
                    append_plain_restriction(&reader, &element, &mut restrictions)?;
                }
                stack.push(element.name().as_ref().to_vec());
            }
            Event::Empty(element) => {
                if element.name().as_ref() == b"pkg" {
                    append_plain_restriction(&reader, &element, &mut restrictions)?;
                }
            }
            Event::End(element) => {
                let expected = stack.pop().ok_or_else(|| {
                    PackageRestrictionsError::InvalidXml("unexpected end element".to_string())
                })?;
                if expected.as_slice() != element.name().as_ref() {
                    return Err(PackageRestrictionsError::InvalidXml(
                        "mismatched end element".to_string(),
                    ));
                }
            }
            Event::Eof if stack.is_empty() => return Ok(restrictions),
            Event::Eof => {
                return Err(PackageRestrictionsError::InvalidXml(
                    "document ended before closing all elements".to_string(),
                ));
            }
            _ => {}
        }
        buffer.clear();
    }
}

fn parse_abx_restrictions(
    input: &[u8],
) -> Result<Vec<AndroidPackageRestriction>, PackageRestrictionsError> {
    let mut parser = android_abx::AbxParser::new(input)
        .map_err(|error| PackageRestrictionsError::InvalidXml(error.to_string()))?;
    let mut restrictions = Vec::new();
    while let Some(event) = parser
        .next_event()
        .map_err(|error| PackageRestrictionsError::InvalidXml(error.to_string()))?
    {
        let android_abx::Event::StartTag { name, attributes } = event else {
            continue;
        };
        if name.as_str() != "pkg" {
            continue;
        }
        append_restriction(
            attributes
                .iter()
                .map(|attribute| (attribute.name.to_string(), attribute.as_str().into_owned())),
            &mut restrictions,
        )?;
    }
    Ok(restrictions)
}

fn append_plain_restriction(
    reader: &Reader<&[u8]>,
    element: &BytesStart<'_>,
    restrictions: &mut Vec<AndroidPackageRestriction>,
) -> Result<(), PackageRestrictionsError> {
    let attributes = element
        .attributes()
        .with_checks(false)
        .map(|attribute| {
            let attribute = attribute
                .map_err(|error| PackageRestrictionsError::InvalidXml(error.to_string()))?;
            let value = attribute
                .decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, reader.decoder())
                .map_err(|error| PackageRestrictionsError::InvalidXml(error.to_string()))?
                .into_owned();
            Ok((
                String::from_utf8_lossy(attribute.key.as_ref()).into_owned(),
                value,
            ))
        })
        .collect::<Result<Vec<_>, PackageRestrictionsError>>()?;
    append_restriction(attributes, restrictions)
}

fn append_restriction(
    attributes: impl IntoIterator<Item = (String, String)>,
    restrictions: &mut Vec<AndroidPackageRestriction>,
) -> Result<(), PackageRestrictionsError> {
    let mut package_name = None;
    let mut installed = None;
    let mut enabled_state = None;
    let mut hidden = None;
    let mut stopped = None;
    let mut suspended = None;
    for (key, value) in attributes {
        if value.len() > MAX_ATTRIBUTE_VALUE_BYTES {
            return Err(PackageRestrictionsError::AttributeTooLong);
        }
        match key.as_str() {
            "name" => package_name = non_blank(value),
            "installed" => installed = parse_bool(&value),
            "enabled" => enabled_state = value.parse().ok(),
            "hidden" => hidden = parse_bool(&value),
            "stopped" => stopped = parse_bool(&value),
            "suspended" => suspended = parse_bool(&value),
            _ => {}
        }
    }
    if let Some(package_name) = package_name {
        restrictions.push(AndroidPackageRestriction {
            package_name,
            installed,
            enabled_state,
            hidden,
            stopped,
            suspended,
        });
    }
    Ok(())
}

fn non_blank(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}
