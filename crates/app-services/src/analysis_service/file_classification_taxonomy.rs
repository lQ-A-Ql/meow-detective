//! Magic-byte signature table and metadata fallback mapping for the
//! two-level file classification board.

pub(crate) struct MagicSignature {
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

pub(crate) const MAGIC_SIGNATURES: &[MagicSignature] = &[
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

pub(crate) fn detect_magic(data: &[u8], ext: &str) -> Option<(&'static str, &'static str)> {
    // RIFF containers need the format tag at offset 8.
    if data.len() >= 12 && &data[..4] == b"RIFF" {
        return match &data[8..12] {
            b"WAVE" => Some(("WAV", "media")),
            b"AVI " => Some(("AVI", "media")),
            b"WEBP" => Some(("WEBP", "images")),
            _ => None,
        };
    }
    for signature in MAGIC_SIGNATURES {
        if data.len() >= signature.offset + signature.bytes.len()
            && &data[signature.offset..signature.offset + signature.bytes.len()] == signature.bytes
        {
            // ZIP containers double as Office documents only via extension.
            if signature.file_type == "ZIP" && !is_office_ext(ext) {
                return Some(("ZIP", "archives"));
            }
            return Some((signature.file_type, signature.family));
        }
    }
    None
}
pub(crate) fn is_office_ext(ext: &str) -> bool {
    matches!(ext, "docx" | "xlsx" | "pptx")
}
pub(crate) fn file_type_from_ext(ext: &str) -> Option<&'static str> {
    match ext {
        "exe" | "dll" | "sys" | "com" | "scr" => Some("PE"),
        "elf" => Some("ELF"),
        "pdf" => Some("PDF"),
        "jpg" | "jpeg" => Some("JPEG"),
        "png" => Some("PNG"),
        "gif" => Some("GIF"),
        "bmp" => Some("BMP"),
        "webp" => Some("WEBP"),
        "ico" => Some("ICO"),
        "mp4" | "mov" => Some("MP4"),
        "mkv" | "webm" => Some("MKV"),
        "avi" => Some("AVI"),
        "mp3" => Some("MP3"),
        "wav" => Some("WAV"),
        "flac" => Some("FLAC"),
        "aac" | "ogg" => Some("OGG"),
        "zip" => Some("ZIP"),
        "rar" => Some("RAR"),
        "7z" => Some("7Z"),
        "gz" => Some("GZ"),
        "sqlite" | "sqlite3" | "db" | "db3" => Some("SQLite"),
        "evtx" => Some("EVTX"),
        "pst" | "ost" => Some("PST"),
        "lnk" => Some("LNK"),
        "pf" => Some("PF"),
        "e01" => Some("E01"),
        "vmdk" => Some("VMDK"),
        "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" => Some("Office"),
        "mdb" | "edb" => Some("ESE"),
        _ => None,
    }
}
pub(crate) fn family_of(
    file_type: Option<&str>,
    ext: &str,
    name: &str,
    path: &str,
) -> &'static str {
    if path.contains("winevt/logs/") || ext == "log" || matches!(name, "wtmp" | "btmp" | "utmp") {
        return "documents";
    }
    if name.starts_with("thumbcache_") || name == "thumbs.db" {
        return "images";
    }
    match file_type {
        Some("PE" | "ELF") => "executables",
        Some("PDF" | "EVTX" | "PST" | "LNK" | "Office") => "documents",
        Some("JPEG" | "PNG" | "GIF" | "BMP" | "WEBP" | "ICO") => "images",
        Some("MP4" | "MKV" | "AVI" | "WAV" | "MP3" | "FLAC" | "OGG") => "media",
        Some("SQLite" | "ESE") => "databases",
        Some("ZIP" | "RAR" | "7Z" | "GZ") => "archives",
        Some("PF") => "system",
        Some("E01" | "VMDK") => "forensics",
        _ => {
            if matches!(
                ext,
                "doc"
                    | "docx"
                    | "xls"
                    | "xlsx"
                    | "ppt"
                    | "pptx"
                    | "pst"
                    | "ost"
                    | "mbox"
                    | "eml"
                    | "emlx"
                    | "txt"
                    | "md"
                    | "csv"
                    | "json"
                    | "xml"
                    | "html"
                    | "htm"
                    | "yaml"
                    | "yml"
                    | "toml"
                    | "ini"
                    | "lnk"
                    | "evtx"
            ) {
                "documents"
            } else if matches!(ext, "reg" | "dat" | "hve") {
                "system"
            } else if matches!(ext, "raw" | "img" | "dd" | "aff") {
                "forensics"
            } else {
                "other"
            }
        }
    }
}
