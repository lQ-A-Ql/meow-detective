//! Logical-path routing: which WeChat artifact (if any) a file belongs to.
//!
//! The host's path filter is intentionally wide (`*.db` plus a few exact
//! file names), so this module is the plugin's own narrow gate: anything
//! that does not carry the WeChat markers routes to `NotOurs` and yields an
//! empty-Ok payload, never an error. Imported paths may carry a `[P{n}]`
//! partition prefix and mixed separators; callers pass a normalized path
//! (backslashes already folded to `/`).

/// Extraction routes recognized by this plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// `Program Files/Tencent/Weixin/<version>/plugin_info.ini`.
    InstallInfo,
    /// `AppData/Roaming/Tencent/xwechat/ilink/wechat/cloud_account.txt`.
    CloudAccount,
    /// `AppData/Roaming/Tencent/xwechat/login/<wxid>/key_info.dat`.
    KeyInfo,
    /// `AppData/Roaming/Tencent/xwechat/ilink/kvcomm/config.ini`.
    KvConfig,
    /// `xwechat_files/<wxid>/db_storage/<category>/*.db`.
    Database,
    /// Local message attachments and Moments cache images.
    LocalMedia,
    /// Anything else: empty-Ok.
    NotOurs,
}

/// Classify a normalized (`/`-separated) logical path, case-insensitively.
pub fn classify(path: &str) -> Route {
    let lower = path.to_ascii_lowercase();
    let basename = lower.rsplit('/').next().unwrap_or(lower.as_str());
    if basename.ends_with(".db")
        && lower.contains("xwechat_files/")
        && lower.contains("db_storage/")
    {
        return Route::Database;
    }
    if lower.contains("xwechat_files/")
        && ((lower.contains("/msg/attach/")
            && lower.contains("/img/")
            && basename.ends_with(".dat"))
            || lower.contains("/sns/img/"))
    {
        return Route::LocalMedia;
    }
    match basename {
        "plugin_info.ini" if lower.contains("tencent") => Route::InstallInfo,
        "cloud_account.txt" if lower.contains("xwechat") => Route::CloudAccount,
        "key_info.dat" if lower.contains("xwechat") => Route::KeyInfo,
        "config.ini" if lower.contains("xwechat") => Route::KvConfig,
        _ => Route::NotOurs,
    }
}

/// Final path segment (file name) of a normalized path.
pub fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Segment immediately before the file name (the parent directory name).
pub fn parent_segment(path: &str) -> Option<&str> {
    let mut parts = path.rsplitn(3, '/');
    let _file = parts.next()?;
    parts.next()
}

/// The path segment right after the given marker directory, matched
/// case-insensitively but returned in its original case (e.g. the wxid
/// segment after `xwechat_files`, the category after `db_storage`).
pub fn segment_after<'a>(path: &'a str, marker: &str) -> Option<&'a str> {
    let segments: Vec<&str> = path.split('/').collect();
    let marker_lower = marker.to_ascii_lowercase();
    for (index, segment) in segments.iter().enumerate() {
        if segment.to_ascii_lowercase() == marker_lower {
            return segments.get(index + 1).copied();
        }
    }
    None
}

/// WeChat install version: the path segment shaped like `4.1.8.67`
/// (three or four dotted numeric components) under the Weixin directory.
pub fn install_version(path: &str) -> Option<&str> {
    path.split('/').find(|segment| {
        let parts: Vec<&str> = segment.split('.').collect();
        (3..=4).contains(&parts.len())
            && parts
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// Install directory prefix up to and including the version segment.
pub fn install_path(path: &str) -> Option<String> {
    let version = install_version(path)?;
    let mut prefix = Vec::new();
    for segment in path.split('/') {
        prefix.push(segment);
        if segment == version {
            return Some(prefix.join("/"));
        }
    }
    None
}
