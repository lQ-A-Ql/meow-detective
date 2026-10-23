//! Shared bounded magic-byte recognition for evidence files.
//!
//! This module only identifies bytes. Callers decide how a recognized type is
//! presented or parsed; extensions are accepted only for the classification
//! board's ZIP/Office family distinction.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MagicMatch {
    pub file_type: &'static str,
    pub family: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewMagicMatch {
    pub file_type: &'static str,
    pub mime: &'static str,
}

struct MagicSignature {
    offset: usize,
    bytes: &'static [u8],
    file_type: &'static str,
    family: &'static str,
}

const fn sig(
    offset: usize,
    bytes: &'static [u8],
    file_type: &'static str,
    family: &'static str,
) -> MagicSignature {
    MagicSignature {
        offset,
        bytes,
        file_type,
        family,
    }
}

const MAGIC_SIGNATURES: &[MagicSignature] = &[
    sig(0, b"MZ", "PE", "executables"),
    sig(0, b"\x7fELF", "ELF", "executables"),
    sig(0, b"%PDF", "PDF", "documents"),
    sig(0, b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1", "OLE2", "documents"),
    sig(0, b"PK\x03\x04", "ZIP", "documents"),
    sig(0, b"\xff\xd8\xff", "JPEG", "images"),
    sig(0, b"\x89PNG\r\n\x1a\n", "PNG", "images"),
    sig(0, b"GIF87a", "GIF", "images"),
    sig(0, b"GIF89a", "GIF", "images"),
    sig(0, b"BM", "BMP", "images"),
    sig(0, b"\x00\x00\x01\x00", "ICO", "images"),
    sig(0, b"CMMM", "THUMBCACHE", "images"),
    sig(4, b"ftyp", "MP4", "media"),
    sig(0, b"\x1a\x45\xdf\xa3", "MKV", "media"),
    sig(0, b"ID3", "MP3", "media"),
    sig(0, b"\xff\xfb", "MP3", "media"),
    sig(0, b"fLaC", "FLAC", "media"),
    sig(0, b"OggS", "OGG", "media"),
    sig(0, b"Rar!\x1a\x07", "RAR", "archives"),
    sig(0, b"\x37\x7a\xbc\xaf\x27\x1c", "7Z", "archives"),
    sig(0, b"\x1f\x8b", "GZ", "archives"),
    sig(0, b"SQLite format 3", "SQLite", "databases"),
    sig(4, b"\xef\xcd\xab\x89", "ESE", "databases"),
    sig(0, b"regf", "REG", "system"),
    sig(0, b"EVF\x09\x0d\x0a\xff\x00", "E01", "forensics"),
    sig(0, b"LVF\x09\x0d\x0a\xff\x00", "Ex01", "forensics"),
    sig(0, b"LEF\x09\x0d\x0a\xff\x00", "L01", "forensics"),
    sig(0, b"AFF\r\n\xff\x00", "AFF", "forensics"),
    sig(0, b"KDMV", "VMDK", "forensics"),
    sig(0, b"ElfFile\0", "EVTX", "documents"),
    sig(0, b"!BDN", "PST", "documents"),
    sig(0, b"L\x00\x00\x00", "LNK", "documents"),
    sig(0, b"SCCA", "PF", "system"),
    sig(0, b"MAM\x04", "PF", "system"),
];

/// Detect a file family from a bounded header.
pub fn detect(data: &[u8], extension: &str) -> Option<MagicMatch> {
    if data.len() >= 12 && &data[..4] == b"RIFF" {
        let (file_type, family) = match &data[8..12] {
            b"WAVE" => ("WAV", "media"),
            b"AVI " => ("AVI", "media"),
            b"WEBP" => ("WEBP", "images"),
            _ => return None,
        };
        return Some(MagicMatch { file_type, family });
    }

    for signature in MAGIC_SIGNATURES {
        if data.len() < signature.offset + signature.bytes.len()
            || &data[signature.offset..signature.offset + signature.bytes.len()] != signature.bytes
        {
            continue;
        }
        if signature.file_type == "ZIP" && !is_office_extension(extension) {
            return Some(MagicMatch {
                file_type: "ZIP",
                family: "archives",
            });
        }
        return Some(MagicMatch {
            file_type: signature.file_type,
            family: signature.family,
        });
    }
    None
}

/// Detect a type that has a supported media, document, database, or image
/// preview. Office Open XML is refined from ZIP member names in the header.
pub fn detect_preview(data: &[u8]) -> Option<PreviewMagicMatch> {
    if data.starts_with(b"PK\x03\x04") {
        if contains_ascii(data, b"word/") {
            return Some(preview(
                "DOCX",
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            ));
        }
        if contains_ascii(data, b"xl/") {
            return Some(preview(
                "XLSX",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            ));
        }
        if contains_ascii(data, b"ppt/") {
            return Some(preview(
                "PPTX",
                "application/vnd.openxmlformats-officedocument.presentationml.presentation",
            ));
        }
        return None;
    }

    if data.len() >= 12 && &data[4..8] == b"ftyp" && matches!(&data[8..12], b"M4A " | b"M4B ") {
        return Some(preview("M4A", "audio/mp4"));
    }

    let detected = detect(data, "")?;
    let mime = match detected.file_type {
        "PDF" => "application/pdf",
        "SQLite" => "application/x-sqlite3",
        "JPEG" => "image/jpeg",
        "PNG" => "image/png",
        "GIF" => "image/gif",
        "BMP" => "image/bmp",
        "WEBP" => "image/webp",
        "ICO" => "image/x-icon",
        "MP4" => "video/mp4",
        "MKV" => "video/x-matroska",
        "AVI" => "video/x-msvideo",
        "MP3" => "audio/mpeg",
        "WAV" => "audio/wav",
        "FLAC" => "audio/flac",
        "OGG" => "audio/ogg",
        _ => return None,
    };
    Some(preview(detected.file_type, mime))
}

pub fn is_office_extension(extension: &str) -> bool {
    matches!(
        extension.trim().to_ascii_lowercase().as_str(),
        "docx" | "xlsx" | "pptx"
    )
}

fn preview(file_type: &'static str, mime: &'static str) -> PreviewMagicMatch {
    PreviewMagicMatch { file_type, mime }
}

fn contains_ascii(data: &[u8], needle: &[u8]) -> bool {
    data.windows(needle.len()).any(|window| window == needle)
}
