use base64::Engine;
use sha2::{Digest, Sha256};
use transport::dto::{
    mcp_host::{McpHostFileChunkDto, McpHostFileEncodingDto, McpHostReadFileRequestDto},
    ViewerRangeRequestDto,
};

use super::{McpHostFileQueryContext, McpHostServiceError};
use crate::file_service::{self, PreviewRuntimeRegistry};

// A tool read owns its temporary preview handle. Drop closes it on every exit,
// including range validation, decoding failures and reader errors.
struct ReadSession<'a> {
    registry: &'a PreviewRuntimeRegistry,
    case_id: &'a domain::CaseId,
    handle_id: String,
}

impl Drop for ReadSession<'_> {
    fn drop(&mut self) {
        let _ = file_service::close_preview_session_for_case(
            self.registry,
            self.case_id,
            &self.handle_id,
        );
    }
}

pub(super) fn read_file(
    context: &McpHostFileQueryContext<'_>,
    request: &McpHostReadFileRequestDto,
) -> Result<McpHostFileChunkDto, McpHostServiceError> {
    let query = &context.query;
    let metadata = file_service::get_file_metadata_for_case(
        query.connection,
        query.case_root,
        &query.case_meta.id,
        &request.file_id,
    )
    .map_err(file_read_error)?;
    if metadata.entry_type != "file" {
        return Err(McpHostServiceError::InvalidInput);
    }
    let handle = file_service::open_preview_session_for_case_with_bitlocker(
        context.bitlocker_runtime,
        context.preview_runtime,
        query.connection,
        query.case_root,
        &query.case_meta.id,
        &request.file_id,
    )
    .map_err(file_read_error)?;
    let session = ReadSession {
        registry: context.preview_runtime,
        case_id: &query.case_meta.id,
        handle_id: handle.handle_id,
    };
    if request.offset > handle.size {
        return Err(McpHostServiceError::InvalidInput);
    }
    let expected_length = (handle.size - request.offset).min(u64::from(request.length)) as u32;
    let bytes = if expected_length == 0 {
        Vec::new()
    } else {
        file_service::read_preview_session_range_for_case_with_bitlocker(
            context.bitlocker_runtime,
            context.preview_runtime,
            query.connection,
            query.case_root,
            &query.case_meta.id,
            &ViewerRangeRequestDto {
                handle_id: session.handle_id.clone(),
                offset: request.offset,
                length: expected_length,
            },
        )
        .map_err(file_read_error)?
        .raw_bytes
        .ok_or(McpHostServiceError::MissingBytes)?
    };
    if bytes.len() != expected_length as usize {
        return Err(McpHostServiceError::IncompleteRead);
    }
    let (encoding, content) = encode_bytes(&bytes, request.encoding)?;
    let next_offset = request.offset + bytes.len() as u64;
    Ok(McpHostFileChunkDto {
        file_id: request.file_id.clone(),
        size: handle.size,
        offset: request.offset,
        bytes_read: bytes.len() as u32,
        next_offset,
        eof: next_offset >= handle.size,
        encoding,
        content,
        chunk_sha256: hex::encode(Sha256::digest(&bytes)),
    })
}

fn file_read_error(error: file_service::FileServiceError) -> McpHostServiceError {
    match &error {
        file_service::FileServiceError::Unsupported(_) => McpHostServiceError::UnsupportedFile,
        file_service::FileServiceError::Io(error)
            if error.kind() == std::io::ErrorKind::Unsupported =>
        {
            McpHostServiceError::UnsupportedFile
        }
        _ => McpHostServiceError::File(error),
    }
}

fn encode_bytes(
    bytes: &[u8],
    encoding: McpHostFileEncodingDto,
) -> Result<(McpHostFileEncodingDto, String), McpHostServiceError> {
    use McpHostFileEncodingDto::{Auto, Base64, Utf8};
    match encoding {
        Utf8 => std::str::from_utf8(bytes)
            .map(|text| (Utf8, text.into()))
            .map_err(|_| McpHostServiceError::InvalidUtf8),
        Auto if !bytes.contains(&0) => match std::str::from_utf8(bytes) {
            Ok(text) => Ok((Utf8, text.into())),
            Err(_) => Ok((
                Base64,
                base64::engine::general_purpose::STANDARD.encode(bytes),
            )),
        },
        Auto | Base64 => Ok((
            Base64,
            base64::engine::general_purpose::STANDARD.encode(bytes),
        )),
    }
}
