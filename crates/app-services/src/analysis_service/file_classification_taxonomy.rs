//! Metadata fallback mapping for the two-level file classification board.
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
