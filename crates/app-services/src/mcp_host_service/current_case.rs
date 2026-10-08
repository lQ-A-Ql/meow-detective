use serde_json::{json, Value};

use super::McpHostQueryContext;

pub(super) fn get_current_case(context: &McpHostQueryContext<'_>) -> Value {
    json!({
        "id": context.case_meta.id.0,
        "name": context.case_meta.name,
        "createdAt": context.case_meta.created_at,
    })
}
