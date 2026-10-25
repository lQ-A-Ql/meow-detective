use crate::resource_budget::MIB;
use transport::dto::bolt_reader::*;

#[derive(Debug, Clone, Copy)]
pub enum BoltReadMode {
    EtcdMetadata,
    ContainerdImages,
}

pub(super) fn request(mode: BoltReadMode, input_size: u64) -> BoltReadRequestDto {
    let images = matches!(mode, BoltReadMode::ContainerdImages);
    BoltReadRequestDto {
        protocol_version: BOLT_READER_PROTOCOL_VERSION,
        request_id: uuid::Uuid::new_v4().to_string(),
        input_path: String::new(),
        input_size,
        input_sha256: String::new(),
        prefix: if images {
            vec![b"v1".to_vec(), b"moby".to_vec(), b"images".to_vec()]
        } else {
            Vec::new()
        },
        record_keys: if images {
            vec![b"digest".to_vec()]
        } else {
            Vec::new()
        },
        value_keys: if images {
            vec![b"digest".to_vec()]
        } else {
            Vec::new()
        },
        parent_depth: if images { Some(2) } else { None },
        bucket_parent_depth: if images { Some(0) } else { None },
        include_buckets: true,
        parent_suffix: if images {
            vec![b"target".to_vec()]
        } else {
            Vec::new()
        },
        limits: BoltReadLimitsDto {
            max_file_bytes: if images { 32 * MIB } else { 512 * MIB },
            max_entries: if images { 1_048_576 } else { 131_072 },
            max_depth: 32,
            max_output_bytes: 64 * MIB,
            memory_limit_bytes: if images { 256 * MIB } else { 768 * MIB },
            max_runtime_millis: 30_000,
        },
    }
}
