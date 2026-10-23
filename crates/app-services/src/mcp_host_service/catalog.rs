use super::McpHostServiceError;
use serde_json::{json, Value};
use transport::dto::mcp_host::{
    McpHostSettingsDto, McpHostToolDto, MCP_FILE_CHUNK_LIMIT, MCP_FILE_PAGE_LIMIT,
    MCP_MAX_SAFE_OFFSET,
};

pub fn tool_catalog() -> Vec<McpHostToolDto> {
    core_tools()
        .into_iter()
        .chain(file_tools())
        .chain(analysis_tools())
        .chain(timeline_tools())
        .chain(index_tools())
        .collect()
}

fn core_tools() -> Vec<McpHostToolDto> {
    [
        (
            "forensics.get_current_case",
            "读取当前打开案件的基本信息。",
            json!({}),
            vec![],
        ),
        (
            "forensics.list_data_sources",
            "读取当前案件的数据源摘要、分区及处理状态。",
            json!({}),
            vec![],
        ),
        (
            "forensics.get_data_source",
            "读取指定数据源的摘要、分区及哈希状态。",
            json!({"dataSourceId":identifier()}),
            vec!["dataSourceId"],
        ),
        (
            "forensics.list_plugin_modules",
            "读取指定数据源的插件模块、版本和家族数量。",
            json!({"dataSourceId":identifier()}),
            vec!["dataSourceId"],
        ),
        (
            "forensics.get_plugin_module",
            "读取指定数据源上一个插件的版本和家族数量。",
            json!({"dataSourceId":identifier(), "pluginId":identifier()}),
            vec!["dataSourceId", "pluginId"],
        ),
    ]
    .into_iter()
    .map(|(name, description, properties, required)| tool(name, description, properties, required))
    .collect()
}

fn file_tools() -> Vec<McpHostToolDto> {
    let offset = non_negative_integer();
    [
        (
            "forensics.list_files",
            "分页列出数据源根目录或指定目录的文件，包含隐藏、系统及已删除条目。parentId 使用返回的目录 id。",
            json!({"dataSourceId":identifier(), "parentId":identifier(), "offset":offset, "limit":{"type":"integer", "minimum":1, "maximum":MCP_FILE_PAGE_LIMIT, "default":100}}),
            vec!["dataSourceId"],
        ),
        (
            "forensics.get_file_metadata",
            "读取文件或目录的元数据、时间戳和已记录哈希。fileId 使用 list_files 或索引搜索返回的 id。",
            json!({"fileId":identifier()}),
            vec!["fileId"],
        ),
        (
            "forensics.read_file",
            "只读获取文件字节，单次最多 64 KiB；auto 自动返回 UTF-8 或 Base64，nextOffset 用于继续读取。",
            json!({"fileId":identifier(), "offset":offset, "length":{"type":"integer", "minimum":1, "maximum":MCP_FILE_CHUNK_LIMIT, "default":MCP_FILE_CHUNK_LIMIT}, "encoding":{"type":"string", "enum":["auto","utf8","base64"], "default":"auto"}}),
            vec!["fileId"],
        ),
    ]
    .into_iter()
    .map(|(name, description, properties, required)| {
        tool(name, description, properties, required)
    })
    .collect()
}

fn analysis_tools() -> Vec<McpHostToolDto> {
    source_analysis_tools()
        .into_iter()
        .chain(extraction_analysis_tools())
        .chain(case_analysis_tools())
        .collect()
}

fn source_analysis_tools() -> Vec<McpHostToolDto> {
    [
        (
            "forensics.get_analysis_system_info",
            "读取数据源分析中的系统信息、启动历史和网络适配器。",
            source_id(),
            vec!["dataSourceId"],
        ),
        (
            "forensics.get_evidence_classification_summary",
            "读取证据构成、类型和分类统计。",
            source_id(),
            vec!["dataSourceId"],
        ),
        (
            "forensics.get_file_classification_board",
            "读取文件类型分类板；sampleSize 控制读取文件头的最大样本数。",
            json!({"dataSourceId":identifier(), "sampleSize":{"type":"integer","minimum":1,"maximum":5000,"default":300}}),
            vec!["dataSourceId"],
        ),
        (
            "forensics.get_registry_structured_summary",
            "读取结构化注册表摘要。",
            source_id(),
            vec!["dataSourceId"],
        ),
    ]
    .into_iter()
    .map(|(name, description, properties, required)| {
        tool(name, description, properties, required)
    })
    .collect()
}

fn extraction_analysis_tools() -> Vec<McpHostToolDto> {
    let source_page = json!({
        "dataSourceId":identifier(),
        "offset":non_negative_integer(),
        "limit":page_limit(500, 100)
    });
    [
        (
            "forensics.get_registry_summary",
            "分页读取注册表提取摘要。",
            source_page.clone(),
            vec!["dataSourceId"],
        ),
        (
            "forensics.get_browser_history_summary",
            "分页读取浏览器历史、下载、书签和扩展摘要。",
            source_page.clone(),
            vec!["dataSourceId"],
        ),
        (
            "forensics.get_email_summary",
            "分页读取邮件提取摘要。",
            source_page.clone(),
            vec!["dataSourceId"],
        ),
        (
            "forensics.get_event_log_events",
            "分页读取 Windows 事件日志；view 选择 boot、logon、process、account 或 application。",
            json!({"dataSourceId":identifier(), "view":{"type":"string","enum":["boot","logon","process","account","application"]}, "offset":non_negative_integer(), "limit":page_limit(500, 100)}),
            vec!["dataSourceId"],
        ),
        (
            "forensics.get_linux_artifact_summary",
            "分页读取 Linux 产物摘要。",
            source_page.clone(),
            vec!["dataSourceId"],
        ),
        (
            "forensics.get_android_device_info",
            "读取 Android 设备信息。",
            source_id(),
            vec!["dataSourceId"],
        ),
        (
            "forensics.get_android_package_summary",
            "分页读取 Android 包摘要。",
            source_page,
            vec!["dataSourceId"],
        ),
        (
            "forensics.get_plugin_family_entries",
            "分页读取指定插件家族的条目，pluginId 和 family 由插件模块摘要返回。",
            json!({"dataSourceId":identifier(), "pluginId":identifier(), "family":identifier(), "offset":non_negative_integer(), "limit":page_limit(500, 100)}),
            vec!["dataSourceId", "pluginId", "family"],
        ),
        (
            "forensics.get_analysis_report",
            "生成指定数据源的只读 Markdown 分析摘要。",
            source_id(),
            vec!["dataSourceId"],
        ),
    ]
    .into_iter()
    .map(|(name, description, properties, required)| {
        tool(name, description, properties, required)
    })
    .collect()
}

fn case_analysis_tools() -> Vec<McpHostToolDto> {
    [
        (
            "forensics.get_case_overview",
            "读取当前案件概览、时间线计数、关联统计和平台覆盖。",
            json!({}),
            vec![],
        ),
        (
            "forensics.get_correlation_snapshot",
            "读取当前案件的关联分析快照。",
            json!({}),
            vec![],
        ),
    ]
    .into_iter()
    .map(|(name, description, properties, required)| tool(name, description, properties, required))
    .collect()
}

fn timeline_tools() -> Vec<McpHostToolDto> {
    [
        (
            "forensics.get_timeline_events",
            "分页读取当前案件时间线；可用 RFC3339 时间范围和 eventType 过滤，cursor 用于继续分页。",
            json!({"offset":non_negative_integer(), "limit":page_limit(500, 100), "timeStart":timestamp(), "timeEnd":timestamp(), "eventType":identifier(), "cursor":{"type":"string","minLength":1,"maxLength":16384}}),
            vec![],
        ),
        (
            "forensics.get_timeline_event",
            "按 id 读取一条时间线事件。",
            json!({"eventId":identifier()}),
            vec!["eventId"],
        ),
        (
            "forensics.get_timeline_facets",
            "读取时间线总数、事件类型、数据源和直方图分布。",
            json!({"timeStart":timestamp(), "timeEnd":timestamp(), "eventType":identifier(), "bucketCount":{"type":"integer","minimum":20,"maximum":180,"default":60}}),
            vec![],
        ),
    ]
    .into_iter()
    .map(|(name, description, properties, required)| {
        tool(name, description, properties, required)
    })
    .collect()
}

fn index_tools() -> Vec<McpHostToolDto> {
    [
        (
            "forensics.search_files",
            "搜索当前案件全局文件索引；支持路径匹配、条目类型、扩展名、数据源和排序。空 query 可浏览索引。",
            json!({"query":{"type":"string","minLength":0,"maxLength":1000}, "matchPath":{"type":"boolean"}, "entryType":{"type":"string","enum":["any","file","directory"]}, "extensions":{"type":"array","maxItems":32,"items":{"type":"string","minLength":1,"maxLength":32}}, "dataSourceIds":{"type":"array","maxItems":64,"items":identifier()}, "sortKey":{"type":"string","enum":["name","path","size","modifiedAt"]}, "sortDirection":{"type":"string","enum":["asc","desc"]}, "offset":non_negative_integer(), "limit":page_limit(500, 50), "cursor":{"type":"string","minLength":1,"maxLength":16384}}),
            vec!["query"],
        ),
        (
            "forensics.get_content_index_coverage",
            "读取指定数据源内容索引的构建覆盖度、过期状态和来源修订。",
            source_id(),
            vec!["dataSourceId"],
        ),
    ]
    .into_iter()
    .map(|(name, description, properties, required)| {
        tool(name, description, properties, required)
    })
    .collect()
}

fn tool(name: &str, description: &str, properties: Value, required: Vec<&str>) -> McpHostToolDto {
    McpHostToolDto {
        name: name.into(),
        description: description.into(),
        input_schema: json!({
            "type":"object",
            "properties":properties,
            "required":required,
            "additionalProperties":false
        }),
    }
}

fn identifier() -> Value {
    json!({"type":"string", "minLength":1, "maxLength":256})
}

fn timestamp() -> Value {
    json!({"type":"string", "minLength":1, "maxLength":64})
}

fn non_negative_integer() -> Value {
    json!({"type":"integer", "minimum":0, "maximum":MCP_MAX_SAFE_OFFSET, "default":0})
}

fn page_limit(maximum: u32, default: u32) -> Value {
    json!({"type":"integer", "minimum":1, "maximum":maximum, "default":default})
}

fn source_id() -> Value {
    json!({"dataSourceId":identifier()})
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
