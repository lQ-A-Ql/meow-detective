use crate::{file_service::FileServiceError, text_service::TextService};
use transport::dto::TextPreviewDto;

use super::{preview_bytes::read_preview_bytes_for_file, PreviewReadContext};

pub fn text_preview_for_file<C>(
    mut context: C,
    file_id: &str,
    max_bytes: Option<usize>,
) -> Result<TextPreviewDto, FileServiceError>
where
    C: PreviewReadContext,
{
    let max = max_bytes
        .unwrap_or(infrastructure::constants::DEFAULT_TEXT_PREVIEW_MAX_BYTES)
        .min(transport::dto::MAX_VIEWER_RANGE_LENGTH as usize) as u32;
    let content_bytes = read_preview_bytes_for_file(&mut context, file_id, 0, max)?;
    let preview =
        TextService::extract_text_preview(&mut std::io::Cursor::new(&content_bytes), max as usize)?;
    let is_binary = preview.is_binary;
    let content = preview.content;
    let hex_dump = is_binary.then(|| format_hex_dump(&content_bytes));

    Ok(TextPreviewDto {
        hex_dump,
        content,
        encoding: preview.encoding,
        is_truncated: preview.is_truncated,
        line_count: preview.line_count,
        is_binary,
        language: preview.language,
    })
}

fn format_hex_dump(bytes: &[u8]) -> String {
    let max_display = 16384usize.min(bytes.len());
    let mut out = String::with_capacity(max_display * 5);
    for (line_idx, chunk) in bytes[..max_display].chunks(16).enumerate() {
        let offset = line_idx * 16;
        use std::fmt::Write;
        let _ = write!(out, "{offset:08X}  ");
        for (index, byte) in chunk.iter().enumerate() {
            if index == 8 {
                out.push(' ');
            }
            let _ = write!(out, "{byte:02X} ");
        }
        out.push_str(" |");
        for byte in chunk {
            out.push(if byte.is_ascii_graphic() || *byte == b' ' {
                *byte as char
            } else {
                '.'
            });
        }
        out.push_str("|\n");
    }
    if bytes.len() > max_display {
        out.push_str("... (truncated)\n");
    }
    out
}
