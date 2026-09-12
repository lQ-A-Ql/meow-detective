use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use thiserror::Error;

const MAX_SETTING_VALUE_BYTES: usize = 16 * 1024;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SettingsXmlError {
    #[error("Android settings XML is malformed: {0}")]
    InvalidXml(String),
    #[error("Android settings XML attribute exceeds the supported limit")]
    AttributeTooLong,
}

/// Returns the first non-blank value of a named SettingsProvider XML entry.
///
/// Android 8+ scopes ANDROID_ID by app signing key, so this only represents
/// the legacy/global value when the source artifact actually contains it.
pub fn find_setting_value(
    input: &[u8],
    expected_name: &str,
) -> Result<Option<String>, SettingsXmlError> {
    if input.starts_with(&android_abx::MAGIC) {
        return find_abx_setting_value(input, expected_name);
    }
    let mut reader = Reader::from_reader(input);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    loop {
        match reader
            .read_event_into(&mut buffer)
            .map_err(|error| SettingsXmlError::InvalidXml(error.to_string()))?
        {
            Event::Start(element) | Event::Empty(element)
                if element.name().as_ref() == b"setting" =>
            {
                if let Some(value) = setting_value(&reader, &element, expected_name)? {
                    return Ok(Some(value));
                }
            }
            Event::Eof => return Ok(None),
            _ => {}
        }
        buffer.clear();
    }
}

fn find_abx_setting_value(
    input: &[u8],
    expected_name: &str,
) -> Result<Option<String>, SettingsXmlError> {
    let mut parser = android_abx::AbxParser::new(input)
        .map_err(|error| SettingsXmlError::InvalidXml(error.to_string()))?;
    while let Some(event) = parser
        .next_event()
        .map_err(|error| SettingsXmlError::InvalidXml(error.to_string()))?
    {
        let android_abx::Event::StartTag { name, attributes } = event else {
            continue;
        };
        if name.as_str() != "setting" {
            continue;
        }
        let mut setting_name = None;
        let mut setting_value = None;
        for attribute in attributes {
            let value = attribute.as_str().into_owned();
            if value.len() > MAX_SETTING_VALUE_BYTES {
                return Err(SettingsXmlError::AttributeTooLong);
            }
            match attribute.name.as_str() {
                "name" => setting_name = Some(value),
                "value" => setting_value = Some(value),
                _ => {}
            }
        }
        if setting_name.as_deref() == Some(expected_name) {
            return Ok(setting_value.filter(|value| !value.trim().is_empty()));
        }
    }
    Ok(None)
}

fn setting_value(
    reader: &Reader<&[u8]>,
    element: &BytesStart<'_>,
    expected_name: &str,
) -> Result<Option<String>, SettingsXmlError> {
    let mut name = None;
    let mut value = None;
    for attribute in element.attributes().with_checks(false) {
        let attribute =
            attribute.map_err(|error| SettingsXmlError::InvalidXml(error.to_string()))?;
        if attribute.value.len() > MAX_SETTING_VALUE_BYTES {
            return Err(SettingsXmlError::AttributeTooLong);
        }
        let decoded = attribute
            .decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, reader.decoder())
            .map_err(|error| SettingsXmlError::InvalidXml(error.to_string()))?
            .into_owned();
        match attribute.key.as_ref() {
            b"name" => name = Some(decoded),
            b"value" => value = Some(decoded),
            _ => {}
        }
    }
    match (name, value) {
        (Some(name), Some(value)) if name == expected_name && !value.trim().is_empty() => {
            Ok(Some(value))
        }
        _ => Ok(None),
    }
}
