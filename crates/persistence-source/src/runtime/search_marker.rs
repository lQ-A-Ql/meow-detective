//! Search projection generation marker stored in the source Fjall metadata keyspace.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchProjectionMarker {
    pub source_id: String,
    pub input_revision: String,
    pub generation: String,
    pub schema_version: u32,
    pub index_path: String,
}
