use super::encoding::{append_field, append_optional_field, append_optional_u64};
use super::{ForensicObjectType, FMD_SCHEMA_VERSION};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForensicMetadata {
    pub object_id: String,
    pub case_id: Option<String>,
    pub source_id: Option<String>,
    pub object_type: ForensicObjectType,
    pub parent_object_id: Option<String>,
    pub source_locator: Option<String>,
    pub byte_length: Option<u64>,
    pub parser_id: Option<String>,
    pub parser_version: Option<String>,
}

impl ForensicMetadata {
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(256);
        append_field(&mut bytes, FMD_SCHEMA_VERSION.as_bytes());
        append_field(&mut bytes, self.object_type.as_str().as_bytes());
        append_field(&mut bytes, self.object_id.as_bytes());
        append_optional_field(&mut bytes, self.case_id.as_deref());
        append_optional_field(&mut bytes, self.source_id.as_deref());
        append_optional_field(&mut bytes, self.parent_object_id.as_deref());
        append_optional_field(&mut bytes, self.source_locator.as_deref());
        append_optional_u64(&mut bytes, self.byte_length);
        append_optional_field(&mut bytes, self.parser_id.as_deref());
        append_optional_field(&mut bytes, self.parser_version.as_deref());
        bytes
    }

    pub fn metadata_sha256(&self) -> String {
        hex::encode(Sha256::digest(self.canonical_bytes()))
    }
}
