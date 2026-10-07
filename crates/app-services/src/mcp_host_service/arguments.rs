use super::{tool_catalog, McpHostServiceError};
use serde_json::Value;

pub(super) fn validate_arguments(name: &str, arguments: &Value) -> Result<(), McpHostServiceError> {
    let tool = tool_catalog()
        .into_iter()
        .find(|tool| tool.name == name)
        .ok_or(McpHostServiceError::InvalidInput)?;
    let fields = arguments
        .as_object()
        .ok_or(McpHostServiceError::InvalidInput)?;
    let properties = &tool.input_schema["properties"];
    for key in tool.input_schema["required"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if key.as_str().and_then(|key| fields.get(key)).is_none() {
            return Err(McpHostServiceError::InvalidInput);
        }
    }
    for (key, value) in fields {
        let schema = properties
            .get(key)
            .ok_or(McpHostServiceError::InvalidInput)?;
        if !valid_field(value, schema) {
            return Err(McpHostServiceError::InvalidInput);
        }
    }
    Ok(())
}

fn valid_field(value: &Value, schema: &Value) -> bool {
    let valid = match schema["type"].as_str() {
        Some("string") => value.as_str().is_some_and(|text| {
            !text.trim().is_empty()
                && text.chars().count() as u64 >= schema["minLength"].as_u64().unwrap_or(0)
                && text.len() as u64 <= schema["maxLength"].as_u64().unwrap_or(u64::MAX)
        }),
        Some("integer") => value.as_u64().is_some_and(|number| {
            number >= schema["minimum"].as_u64().unwrap_or(0)
                && number <= schema["maximum"].as_u64().unwrap_or(u64::MAX)
        }),
        _ => false,
    };
    valid
        && schema["enum"]
            .as_array()
            .is_none_or(|choices| choices.contains(value))
}
