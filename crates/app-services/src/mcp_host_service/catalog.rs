use super::McpHostServiceError;
mod schema;

use schema::{
    file_chunk_schema, identifier, job_snapshot_schema, metrics_schema, non_negative_integer,
    output_schema, page_limit, source_id, timestamp,
};
use serde_json::{json, Value};
use transport::dto::mcp_host::{
    McpHostSettingsDto, McpHostToolDto, MCP_FILE_CHUNK_LIMIT, MCP_FILE_PAGE_LIMIT,
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
        (
            "forensics.list_jobs",
            "读取当前案件最近任务的状态、进度和结果摘要。",
            json!({}),
            vec![],
        ),
        (
            "forensics.get_job",
            "读取当前案件指定任务的状态和进度。",
            json!({"jobId":identifier()}),
            vec!["jobId"],
        ),
        (
            "forensics.get_task",
            "读取当前案件证据任务的状态、进度和已发布结果。",
            json!({"taskId":identifier()}),
            vec!["taskId"],
        ),
        (
            "forensics.cancel_job",
            "请求取消当前案件中的后台任务，并返回取消请求是否已写入任务状态。",
            json!({"jobId":identifier()}),
            vec!["jobId"],
        ),
        (
            "forensics.start_content_index",
            "为指定数据源创建内容索引任务并返回任务标识。",
            json!({"dataSourceId":identifier(), "idempotencyKey":{"type":"string","minLength":1,"maxLength":128}}),
            vec!["dataSourceId"],
        ),
        (
            "forensics.start_analysis",
            "为指定数据源创建证据分类分析任务并返回任务标识。",
            json!({"dataSourceId":identifier(), "categories":{"type":"array","maxItems":32,"items":identifier()}, "idempotencyKey":{"type":"string","minLength":1,"maxLength":128}}),
            vec!["dataSourceId"],
        ),
        (
            "forensics.list_report_templates",
            "读取当前版本支持的报告模板。",
            json!({}),
            vec![],
        ),
        (
            "forensics.get_report_history",
            "读取当前案件已生成报告的历史记录。",
            json!({}),
            vec![],
        ),
        (
            "forensics.start_report_export",
            "创建报告导出任务并返回任务标识。",
            json!({"templateId":identifier(), "format":{"type":"string","enum":["html","csv","json"]}, "idempotencyKey":{"type":"string","minLength":1,"maxLength":128}}),
            vec!["templateId", "format"],
        ),
        (
            "forensics.get_mcp_metrics",
            "读取 MCP 服务运行、队列、调用计数和平均耗时指标。",
            json!({}),
            vec![],
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
            "forensics.browse_registry_key",
            "读取 source-scoped Registry hive 的指定键，返回键元数据、值和子键。",
            json!({"fileId":identifier(), "keyPath":{"type":"string","maxLength":4096,"default":""}}),
            vec!["fileId"],
        ),
        (
            "forensics.open_registry_hive",
            "打开 source-scoped Registry hive 只读会话。",
            json!({"fileId":identifier(), "view":{"type":"string","enum":["base","recovered"],"default":"base"}}),
            vec!["fileId", "view"],
        ),
        (
            "forensics.resolve_registry_key",
            "在 Registry 会话中解析键路径。",
            json!({"sessionId":identifier(), "keyPath":{"type":"string","maxLength":4096}}),
            vec!["sessionId", "keyPath"],
        ),
        (
            "forensics.list_registry_subkeys",
            "分页列出 Registry 键的子键。",
            json!({"sessionId":identifier(), "keyId":identifier(), "limit":{"type":"integer","minimum":1,"maximum":200,"default":100}, "cursor":identifier()}),
            vec!["sessionId", "keyId"],
        ),
        (
            "forensics.list_registry_values",
            "分页列出 Registry 键的值元数据。",
            json!({"sessionId":identifier(), "keyId":identifier(), "limit":{"type":"integer","minimum":1,"maximum":200,"default":100}, "cursor":identifier()}),
            vec!["sessionId", "keyId"],
        ),
        (
            "forensics.read_registry_value_range",
            "读取 Registry 值的受限字节范围。",
            json!({"sessionId":identifier(), "keyId":identifier(), "valueId":identifier(), "offset":non_negative_integer(), "length":{"type":"integer","minimum":1,"maximum":65536,"default":65536}}),
            vec!["sessionId", "keyId", "valueId"],
        ),
        (
            "forensics.close_registry_hive",
            "关闭 Registry 只读会话并释放内存。",
            json!({"sessionId":identifier()}),
            vec!["sessionId"],
        ),
        (
            "forensics.read_file",
            "只读获取文件字节，单次最多 64 KiB；auto 自动返回 UTF-8 或 Base64，nextOffset 用于继续读取。",
            json!({"fileId":identifier(), "offset":offset, "length":{"type":"integer", "minimum":1, "maximum":MCP_FILE_CHUNK_LIMIT, "default":MCP_FILE_CHUNK_LIMIT}, "encoding":{"type":"string", "enum":["auto","utf8","base64"], "default":"auto"}}),
            vec!["fileId"],
        ),
        (
            "forensics.extract_file",
            "将当前案件中的普通文件提取到数据源对应的受控 exports 目录；relativePath 为空时使用原文件名。桌面调试可使用已验证的绝对路径。默认拒绝覆盖。",
            json!({"fileId":identifier(), "destinationPath":{"type":"string","minLength":1,"maxLength":32767}, "relativePath":{"type":"string","minLength":1,"maxLength":32767}, "overwrite":{"type":"boolean","default":false}}),
            vec!["fileId"],
        ),
    ]
    .into_iter()
    .map(|(name, description, properties, required)| {
        if matches!(
            name,
            "forensics.extract_file"
                | "forensics.cancel_job"
                | "forensics.start_content_index"
                | "forensics.start_analysis"
                | "forensics.start_report_export"
        ) {
            tool_with_access(name, description, properties, required, false, true)
        } else {
            tool(name, description, properties, required)
        }
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
            "forensics.list_linux_services",
            "分页读取 Linux systemd 服务配置、启用状态和 source-scoped 服务文件 ID。",
            source_page.clone(),
            vec!["dataSourceId"],
        ),
        (
            "forensics.list_docker_containers",
            "分页读取 Docker 容器元数据和 source-scoped 配置文件 ID。",
            source_page.clone(),
            vec!["dataSourceId"],
        ),
        (
            "forensics.list_docker_images",
            "分页读取 Docker 镜像元数据和来源文件 ID。",
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
            "分页读取当前案件的关联分析快照；节点、边、簇和线索使用同一 offset/limit。",
            json!({"offset":non_negative_integer(), "limit":page_limit(100, 50)}),
            vec![],
        ),
        (
            "forensics.query_graph",
            "按类型化起始节点和边条件查询当前案件图关系。",
            json!({"startIds":{"type":"array","maxItems":64,"items":identifier()}, "edgeTypes":{"type":"array","maxItems":16,"items":identifier()}, "maxDepth":{"type":"integer","minimum":0,"maximum":8,"default":3}, "confidenceFloor":{"type":"number","minimum":0,"maximum":1}, "limit":{"type":"integer","minimum":1,"maximum":200,"default":100}, "edgeLimit":{"type":"integer","minimum":1,"maximum":600,"default":400}}),
            vec!["startIds"],
        ),
        (
            "forensics.get_provenance_chain",
            "读取指定图边的来源规则、解析器和提取版本链。",
            json!({"edgeId":identifier()}),
            vec!["edgeId"],
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
            "搜索当前案件全局文件索引；支持路径匹配、条目类型、扩展名、数据源和排序。空 query 可浏览索引，使用 nextCursor 遍历完整结果集。",
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
    tool_with_access(name, description, properties, required, true, false)
}

fn tool_with_access(
    name: &str,
    description: &str,
    properties: Value,
    required: Vec<&str>,
    read_only: bool,
    destructive: bool,
) -> McpHostToolDto {
    McpHostToolDto {
        name: name.into(),
        description: description.into(),
        input_schema: json!({
            "type":"object",
            "properties":properties,
            "required":required,
            "additionalProperties":false
        }),
        output_schema: match name {
            "forensics.list_jobs" => json!({
                "type":"array",
                "items":job_snapshot_schema()
            }),
            "forensics.get_job" => job_snapshot_schema(),
            "forensics.get_mcp_metrics" => metrics_schema(),
            "forensics.read_file" => file_chunk_schema(),
            _ => output_schema(),
        },
        read_only,
        destructive,
    }
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
