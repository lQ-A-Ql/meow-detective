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

pub(super) fn metrics_schema() -> Value {
    json!({
        "type":"object",
        "properties": {
            "running":{"type":"boolean"},
            "activeRequests":{"type":"integer","minimum":0},
            "queuedRequests":{"type":"integer","minimum":0},
            "taskOwnerCount":{"type":"integer","minimum":0},
            "totalCalls":{"type":"integer","minimum":0},
            "successfulCalls":{"type":"integer","minimum":0},
            "failedCalls":{"type":"integer","minimum":0},
            "cancelledCalls":{"type":"integer","minimum":0},
            "totalDurationMs":{"type":"integer","minimum":0},
            "averageDurationMs":{"type":"integer","minimum":0},
            "currentRssMb":{"type":"integer","minimum":0},
            "peakRssMb":{"type":"integer","minimum":0}
        },
        "required":["running","activeRequests","queuedRequests","taskOwnerCount","totalCalls","successfulCalls","failedCalls","cancelledCalls","totalDurationMs","averageDurationMs","currentRssMb","peakRssMb"]
    })
}

pub(super) fn file_chunk_schema() -> Value {
    json!({
        "type":"object",
        "properties": {
            "fileId":{"type":"string"},
            "size":{"type":"integer","minimum":0},
            "offset":{"type":"integer","minimum":0},
            "bytesRead":{"type":"integer","minimum":0},
            "nextOffset":{"type":"integer","minimum":0},
            "eof":{"type":"boolean"},
            "encoding":{"type":"string","enum":["auto","utf8","base64"]},
            "content":{"type":"string"},
            "chunkSha256":{"type":"string","minLength":64,"maxLength":64}
        },
        "required":["fileId","size","offset","bytesRead","nextOffset","eof","encoding","content","chunkSha256"]
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
