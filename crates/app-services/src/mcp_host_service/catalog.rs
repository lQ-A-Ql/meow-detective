use super::McpHostServiceError;
use serde_json::json;
use transport::dto::mcp_host::{
    McpHostSettingsDto, McpHostToolDto, MCP_FILE_CHUNK_LIMIT, MCP_FILE_PAGE_LIMIT,
    MCP_MAX_SAFE_OFFSET,
};

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
    let mut catalog: Vec<_> = entries.into_iter().map(|(name, description, required)| {
        let properties = required.iter().map(|key| (key.to_string(), json!({"type":"string", "minLength":1, "maxLength":256}))).collect::<serde_json::Map<_, _>>();
        McpHostToolDto {
            name: name.into(), description: description.into(),
            input_schema: json!({"type":"object", "properties":properties, "required":required, "additionalProperties":false}),
        }
    }).collect();
    catalog.extend(file_tools());
    catalog
}

fn file_tools() -> Vec<McpHostToolDto> {
    let identifier = json!({"type":"string", "minLength":1, "maxLength":256});
    let offset = json!({"type":"integer", "minimum":0, "maximum":MCP_MAX_SAFE_OFFSET, "default":0});
    [
        ("forensics.list_files", "分页列出数据源根目录或指定目录的文件，包含隐藏、系统及已删除条目。parentId 使用返回的目录 id。",
         json!({"dataSourceId":identifier, "parentId":identifier, "offset":offset, "limit":{"type":"integer", "minimum":1, "maximum":MCP_FILE_PAGE_LIMIT, "default":100}}), vec!["dataSourceId"]),
        ("forensics.get_file_metadata", "读取文件或目录的元数据、时间戳和已记录哈希。fileId 使用 list_files 返回的 id。",
         json!({"fileId":identifier}), vec!["fileId"]),
        ("forensics.read_file", "只读获取文件字节，单次最多 64 KiB；auto 自动返回 UTF-8 或 Base64，nextOffset 用于继续读取。fileId 使用 list_files 返回的 id。",
         json!({"fileId":identifier, "offset":offset, "length":{"type":"integer", "minimum":1, "maximum":MCP_FILE_CHUNK_LIMIT, "default":MCP_FILE_CHUNK_LIMIT}, "encoding":{"type":"string", "enum":["auto","utf8","base64"], "default":"auto"}}), vec!["fileId"]),
    ].into_iter().map(|(name, description, properties, required)| McpHostToolDto {
        name:name.into(), description:description.into(),
        input_schema:json!({"type":"object", "properties":properties, "required":required, "additionalProperties":false}),
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
