use super::McpHostServiceError;
use serde_json::json;
use transport::dto::mcp_host::{McpHostSettingsDto, McpHostToolDto};

pub fn tool_catalog() -> Vec<McpHostToolDto> {
    let entries = [
        (
            "forensics.get_current_case",
            "读取当前打开案件的基本信息。",
            vec![],
        ),
        (
            "forensics.list_data_sources",
            "读取当前案件的数据源摘要、分区及处理状态。",
            vec![],
        ),
        (
            "forensics.get_data_source",
            "读取指定数据源的摘要、分区及哈希状态。",
            vec!["dataSourceId"],
        ),
        (
            "forensics.list_plugin_modules",
            "读取指定数据源的插件模块、版本和家族数量。",
            vec!["dataSourceId"],
        ),
        (
            "forensics.get_plugin_module",
            "读取指定数据源上一个插件的版本和家族数量。",
            vec!["dataSourceId", "pluginId"],
        ),
    ];
    entries.into_iter().map(|(name, description, required)| {
        let properties = required.iter().map(|key| (key.to_string(), json!({"type":"string", "minLength":1, "maxLength":256}))).collect::<serde_json::Map<_, _>>();
        McpHostToolDto {
            name: name.into(), description: description.into(),
            input_schema: json!({"type":"object", "properties":properties, "required":required, "additionalProperties":false}),
        }
    }).collect()
}

pub fn validate_settings(settings: &mut McpHostSettingsDto) -> Result<(), McpHostServiceError> {
    let catalog = tool_catalog();
    if settings
        .disabled_tools
        .iter()
        .any(|name| !catalog.iter().any(|tool| tool.name == *name))
    {
        return Err(McpHostServiceError::InvalidInput);
    }
    settings.disabled_tools.sort();
    settings.disabled_tools.dedup();
    Ok(())
}
