use std::io::Read;

#[derive(Debug, Clone)]
pub struct ExtractedText {
    pub file_id: String,
    pub content: String,
    pub encoding: String,
    pub extractable: bool,
    pub byte_count: u64,
}

const MAX_TEXT_BYTES: u64 = 10 * 1024 * 1024;

pub fn extract_text(reader: impl Read, file_id: &str, mime_hint: Option<&str>) -> ExtractedText {
    let is_binary = mime_hint.is_some_and(|m| {
        !m.starts_with("text/")
            && m != "application/json"
            && m != "application/xml"
            && m != "application/javascript"
    });

    if is_binary {
        return ExtractedText {
            file_id: file_id.to_string(),
            content: String::new(),
            encoding: "binary".to_string(),
            extractable: false,
            byte_count: 0,
        };
    }

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

    if buf.len() >= 2 {
        if buf[0] == 0xFF && buf[1] == 0xFE {
            return extract_utf16_le(file_id, &buf, byte_count);
        }
        if buf[0] == 0xFE && buf[1] == 0xFF {
            return extract_utf16_be(file_id, &buf, byte_count);
        }
    }

    let content = String::from_utf8_lossy(&buf).into_owned();

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
