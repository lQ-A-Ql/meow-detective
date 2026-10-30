use serde_json::{json, Value};
use transport::dto::mcp_host::MCP_MAX_SAFE_OFFSET;

pub(super) fn output_schema() -> Value {
    json!({
        "type":["object","array","string","number","boolean","null"],
        "description":"结构化工具结果；具体字段随工具版本保持稳定。"
    })
}

pub(super) fn job_snapshot_schema() -> Value {
    json!({
        "type":"object",
        "properties": {
            "id":{"type":"string"},
            "name":{"type":"string"},
            "scope":{"type":"string"},
            "progress":{"type":"integer","minimum":0,"maximum":100},
            "status":{"type":"string"},
            "detail":{"type":"string"},
            "warningCount":{"type":"integer","minimum":0},
            "skippedCount":{"type":"integer","minimum":0},
            "failedCount":{"type":"integer","minimum":0},
            "partial":{"type":"boolean"},
            "currentPartition":{"type":["string","null"]},
            "completedPartitions":{"type":["integer","null"],"minimum":0},
            "totalPartitions":{"type":["integer","null"],"minimum":0},
            "partitionProgress":{"type":["integer","null"],"minimum":0}
        },
        "required":["id","name","scope","progress","status","detail","warningCount","skippedCount","failedCount","partial"]
    })
}

pub(super) fn identifier() -> Value {
    json!({"type":"string", "minLength":1, "maxLength":256})
}

pub(super) fn timestamp() -> Value {
    json!({"type":"string", "minLength":1, "maxLength":64})
}

pub(super) fn non_negative_integer() -> Value {
    json!({"type":"integer", "minimum":0, "maximum":MCP_MAX_SAFE_OFFSET, "default":0})
}

pub(super) fn page_limit(maximum: u32, default: u32) -> Value {
    json!({"type":"integer", "minimum":1, "maximum":maximum, "default":default})
}

pub(super) fn source_id() -> Value {
    json!({"dataSourceId":identifier()})
}
