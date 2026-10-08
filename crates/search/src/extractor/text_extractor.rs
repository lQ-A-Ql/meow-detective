use std::io::Read;

#[derive(Debug, Clone)]
pub struct ExtractedText {
    pub file_id: String,
    pub content: String,
    pub encoding: String,
    pub extractable: bool,
    pub byte_count: u64,
}

/// Bounded content classification shared by indexing and preview callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextContentStatus {
    TextUtf8,
    TextUtf16Le,
    TextUtf16Be,
    Binary,
    UnsupportedEncoding,
}

pub const CONTENT_SNIFF_BYTES: usize = 64 * 1024;

pub fn classify_text_bytes(data: &[u8], mime_hint: Option<&str>) -> TextContentStatus {
    let binary_hint = mime_hint.is_some_and(|mime| {
        !mime.starts_with("text/")
            && mime != "application/json"
            && mime != "application/xml"
            && mime != "application/javascript"
    });
    if binary_hint {
        return TextContentStatus::Binary;
    }
    if data.starts_with(&[0xFF, 0xFE]) {
        return if valid_utf16(&data[..data.len().min(CONTENT_SNIFF_BYTES)], true) {
            TextContentStatus::TextUtf16Le
        } else {
            TextContentStatus::UnsupportedEncoding
        };
    }
    if data.starts_with(&[0xFE, 0xFF]) {
        return if valid_utf16(&data[..data.len().min(CONTENT_SNIFF_BYTES)], false) {
            TextContentStatus::TextUtf16Be
        } else {
            TextContentStatus::UnsupportedEncoding
        };
    }
    let sample = &data[..data.len().min(CONTENT_SNIFF_BYTES)];
    if sample.iter().filter(|byte| **byte == 0).count() * 10 > sample.len() {
        return TextContentStatus::Binary;
    }
    if std::str::from_utf8(sample).is_ok() {
        TextContentStatus::TextUtf8
    } else {
        TextContentStatus::UnsupportedEncoding
    }
}

fn valid_utf16(data: &[u8], little_endian: bool) -> bool {
    let sample_len = data.len().min(CONTENT_SNIFF_BYTES);
    let bytes = &data[2..sample_len - (sample_len.saturating_sub(2) % 2)];
    if !bytes.len().is_multiple_of(2) {
        return false;
    }
    let mut units = Vec::with_capacity(bytes.len() / 2);
    for chunk in bytes.chunks_exact(2) {
        units.push(if little_endian {
            u16::from_le_bytes([chunk[0], chunk[1]])
        } else {
            u16::from_be_bytes([chunk[0], chunk[1]])
        });
    }
    String::from_utf16(&units).is_ok()
}

const MAX_TEXT_BYTES: u64 = 10 * 1024 * 1024;

pub fn extract_text(reader: impl Read, file_id: &str, mime_hint: Option<&str>) -> ExtractedText {
    let mut buf = Vec::new();
    match reader.take(MAX_TEXT_BYTES).read_to_end(&mut buf) {
        Ok(_) => {}
        Err(_) => {
            return ExtractedText {
                file_id: file_id.to_string(),
                content: String::new(),
                encoding: "error".to_string(),
                extractable: false,
                byte_count: 0,
            };
        }
    }

    let byte_count = buf.len() as u64;
    let status = classify_text_bytes(&buf, mime_hint);
    if matches!(
        status,
        TextContentStatus::Binary | TextContentStatus::UnsupportedEncoding
    ) {
        return ExtractedText {
            file_id: file_id.to_string(),
            content: String::new(),
            encoding: if status == TextContentStatus::Binary {
                "binary".to_string()
            } else {
                "unsupported_encoding".to_string()
            },
            extractable: false,
            byte_count,
        };
    }
    if status == TextContentStatus::TextUtf8 && std::str::from_utf8(&buf).is_err() {
        return ExtractedText {
            file_id: file_id.to_string(),
            content: String::new(),
            encoding: "unsupported_encoding".to_string(),
            extractable: false,
            byte_count,
        };
    }

    if buf.len() >= 2 {
        if status == TextContentStatus::TextUtf16Le {
            return extract_utf16_le(file_id, &buf, byte_count);
        }
        if status == TextContentStatus::TextUtf16Be {
            return extract_utf16_be(file_id, &buf, byte_count);
        }
    }

    let content = std::str::from_utf8(&buf).unwrap_or_default().to_string();

    ExtractedText {
        file_id: file_id.to_string(),
        content,
        encoding: "utf-8".to_string(),
        extractable: true,
        byte_count,
    }
}

fn extract_utf16_le(file_id: &str, buf: &[u8], byte_count: u64) -> ExtractedText {
    let chars: Vec<u16> = buf[2..]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let content = String::from_utf16(&chars).unwrap_or_default();
    let extractable = !content.is_empty();
    ExtractedText {
        file_id: file_id.to_string(),
        content,
        encoding: "utf-16le".to_string(),
        extractable,
        byte_count,
    }
}

fn extract_utf16_be(file_id: &str, buf: &[u8], byte_count: u64) -> ExtractedText {
    let chars: Vec<u16> = buf[2..]
        .chunks_exact(2)
        .map(|c| u16::from_be_bytes([c[0], c[1]]))
        .collect();
    let content = String::from_utf16(&chars).unwrap_or_default();
    let extractable = !content.is_empty();
    ExtractedText {
        file_id: file_id.to_string(),
        content,
        encoding: "utf-16be".to_string(),
        extractable,
        byte_count,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/extractor/text_extractor.rs"]
mod tests;
