use std::{cell::RefCell, collections::HashMap};

use domain::FileEntryId;
use rusqlite::Connection;
use serde_json::Value;

use crate::file_service::FileServiceError;

use super::api::read_file_bytes_for_case;

pub fn read_file_header_by_id(
    conn: &Connection,
    file_id: &FileEntryId,
    max_bytes: usize,
) -> Result<Vec<u8>, FileServiceError> {
    read_header_chunks(max_bytes, |offset, length| {
        read_file_bytes_for_case(conn, file_id, offset, length)
    })
}

pub struct FileHeaderReadCache {
    case_id: String,
    descriptors: RefCell<HashMap<String, Value>>,
}

impl FileHeaderReadCache {
    pub fn new(case_id: impl Into<String>) -> Self {
        Self {
            case_id: case_id.into(),
            descriptors: RefCell::new(HashMap::new()),
        }
    }

    pub fn read_file_header_by_id(
        &self,
        conn: &Connection,
        file_id: &FileEntryId,
        max_bytes: usize,
    ) -> Result<Vec<u8>, FileServiceError> {
        if self.case_id.is_empty() {
            return read_file_header_by_id(conn, file_id, max_bytes);
        }
        read_header_chunks(max_bytes, |offset, length| {
            let get_cache = |key: &str| self.descriptors.borrow().get(key).cloned();
            let set_cache = |key: &str, value: &Value| {
                self.descriptors
                    .borrow_mut()
                    .insert(key.to_string(), value.clone());
            };
            read_file_bytes_for_case(
                (conn, self.case_id.as_str(), get_cache, set_cache),
                file_id,
                offset,
                length,
            )
        })
    }
}

fn read_header_chunks(
    max_bytes: usize,
    mut read_chunk: impl FnMut(u64, u32) -> Result<Vec<u8>, FileServiceError>,
) -> Result<Vec<u8>, FileServiceError> {
    let mut bytes = Vec::with_capacity(max_bytes.min(infrastructure::constants::MAX_RANGE_LENGTH));
    let mut offset = 0u64;
    let mut remaining = max_bytes;
    while remaining > 0 {
        let chunk_len = remaining
            .min(infrastructure::constants::MAX_RANGE_LENGTH)
            .min(u32::MAX as usize) as u32;
        if chunk_len == 0 {
            break;
        }
        let chunk = match read_chunk(offset, chunk_len) {
            Ok(chunk) => chunk,
            Err(error) if error.is_read_offset_beyond_size() => break,
            Err(error) => return Err(error),
        };
        if chunk.is_empty() {
            break;
        }
        let is_short_read = chunk.len() < chunk_len as usize;
        offset = offset.saturating_add(chunk.len() as u64);
        remaining = remaining.saturating_sub(chunk.len());
        bytes.extend_from_slice(&chunk);
        if is_short_read {
            break;
        }
    }
    Ok(bytes)
}
