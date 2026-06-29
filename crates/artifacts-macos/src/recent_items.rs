//! macOS Recent Items parser.
//!
//! Parses the `com.apple.recentitems.plist` file found at:
//! `~/Library/Application Support/com.apple.sharedfilelist/com.apple.LSSharedFileList.RecentApplications.sfl2`
//! `~/Library/Preferences/com.apple.recentitems.plist`
//!
//! Recent Items tracks recently opened files, connected servers, and launched
//! applications. The data is stored as a plist with entries for:
//! - **Files**: recently opened documents
//! - **Servers**: recently connected network servers
//! - **Applications**: recently launched applications
//! - **Volumes**: recently mounted volumes
//!
//! Each entry contains a name, path/URL, and optional last-used timestamp.
//!
//! Key structure:
//! ```xml
//! <dict>
//!     <key>Name</key><string>Document.pdf</string>
//!     <key>URL</key><string>file:///Users/test/Documents/Document.pdf</string>
//!     <key>Date</key><date>2024-01-15T10:30:00Z</date>
//! </dict>
//! ```

use crate::error::{MacArtifactError, Result};
use serde::{Deserialize, Serialize};

/// The kind of recent item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RecentItemKind {
    /// A recently opened file/document
    File,
    /// A recently connected network server
    Server,
    /// A recently launched application
    Application,
    /// A recently mounted volume
    Volume,
    /// Unknown/unrecognized type
    Unknown,
}

/// A single recent item entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecentItem {
    /// Display name of the item
    pub name: String,
    /// File path or URL of the item
    pub path: String,
    /// What kind of item this is
    pub kind: RecentItemKind,
    /// Optional last-used timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used: Option<String>,
}

/// Parse a `com.apple.recentitems.plist` file.
///
/// Supports both XML and binary plist formats. Extracts files, servers,
/// applications, and volumes from the recent items list.
pub fn parse_recent_items_plist(data: &[u8]) -> Result<Vec<RecentItem>> {
    if data.is_empty() {
        return Err(MacArtifactError::InvalidInput(
            "Recent items plist data is empty".to_string(),
        ));
    }

    let text = if crate::plist::is_binary_plist(data) {
        // Use binary plist parser and convert
        let entries = crate::plist::parse_binary_plist(data, "com.apple.recentitems.plist")?;
        return parse_from_bplist_entries(&entries);
    } else {
        std::str::from_utf8(data)
            .map_err(|_| {
                MacArtifactError::Decode("Recent items plist is not valid UTF-8".to_string())
            })?
            .to_string()
    };

    parse_xml_recent_items(&text)
}

/// Convert binary plist entries to RecentItem structs.
fn parse_from_bplist_entries(entries: &[crate::plist::MacPlistEntry]) -> Result<Vec<RecentItem>> {
    let mut items: Vec<RecentItem> = Vec::new();
    let mut current_name = String::new();
    let mut current_path = String::new();
    let mut current_kind = RecentItemKind::Unknown;
    let mut current_date: Option<String> = None;

    for entry in entries {
        match entry.key.as_str() {
            "Name" => {
                current_name = entry.value.clone();
            }
            "URL" | "Path" => {
                current_path = entry.value.clone();
                // Determine kind from URL scheme or path
                if entry.value.starts_with("file://") {
                    if entry.value.contains(".app/") || entry.value.ends_with(".app") {
                        current_kind = RecentItemKind::Application;
                    } else {
                        current_kind = RecentItemKind::File;
                    }
                } else if entry.value.starts_with("smb://")
                    || entry.value.starts_with("afp://")
                    || entry.value.starts_with("nfs://")
                {
                    current_kind = RecentItemKind::Server;
                } else if entry.value.starts_with("/Volumes/") {
                    current_kind = RecentItemKind::Volume;
                } else if entry.value.starts_with("/") {
                    current_kind = RecentItemKind::File;
                }
            }
            "Date" | "LastUsedDate" => {
                current_date = Some(entry.value.clone());
            }
            _ => {}
        }

        // If we have a complete entry, add it
        if !current_name.is_empty() && !current_path.is_empty() {
            items.push(RecentItem {
                name: std::mem::take(&mut current_name),
                path: std::mem::take(&mut current_path),
                kind: std::mem::replace(&mut current_kind, RecentItemKind::Unknown),
                last_used: current_date.take(),
            });
        }
    }

    Ok(items)
}

/// Parse XML recent items plist.
fn parse_xml_recent_items(xml: &str) -> Result<Vec<RecentItem>> {
    let mut items: Vec<RecentItem> = Vec::new();

    // State machine for parsing RecentItems/Files/Servers/Applications/Volumes entries
    let mut in_item_dict = false;
    let mut current_name = String::new();
    let mut current_path = String::new();
    let mut current_kind = RecentItemKind::Unknown;
    let mut current_date: Option<String> = None;
    let mut current_key: Option<String> = None;
    let mut current_section = String::new(); // "Files", "Servers", "Applications", "Volumes"

    for line in xml.lines() {
        let trimmed = line.trim();

        // Track section
        if let Some(section) = detect_section_start(trimmed) {
            current_section = section;
            continue;
        }

        if trimmed.contains("<dict>") {
            in_item_dict = true;
            continue;
        }
        if trimmed.contains("</dict>") {
            if in_item_dict && !current_name.is_empty() {
                items.push(RecentItem {
                    name: std::mem::take(&mut current_name),
                    path: std::mem::take(&mut current_path),
                    kind: std::mem::replace(&mut current_kind, RecentItemKind::Unknown),
                    last_used: current_date.take(),
                });
            }
            current_name.clear();
            current_path.clear();
            current_kind = RecentItemKind::Unknown;
            current_date = None;
            current_key = None;
            in_item_dict = false;
            continue;
        }
        if trimmed.contains("</array>") {
            current_section.clear();
            continue;
        }

        if !in_item_dict {
            continue;
        }

        // Extract <key>...</key>
        if let Some(key) = crate::xml::extract_xml_tag_content(trimmed, "key") {
            current_key = Some(key);
            continue;
        }

        // Extract value based on current key — take ownership to avoid borrowing conflicts
        let key_str = match current_key.take() {
            Some(k) => k,
            None => continue,
        };

        // Try string value
        if let Some(value) = crate::xml::extract_xml_tag_content(trimmed, "string") {
            match key_str.as_str() {
                "Name" => {
                    current_name = value;
                }
                "URL" | "Path" | "Alias" => {
                    let path_lower = value.to_lowercase();
                    // Infer kind from path/URL
                    if path_lower.starts_with("file://") {
                        if value.contains(".app/") || value.ends_with(".app") {
                            current_kind = RecentItemKind::Application;
                        } else {
                            current_kind = RecentItemKind::File;
                        }
                    } else if path_lower.starts_with("smb://")
                        || path_lower.starts_with("afp://")
                        || path_lower.starts_with("nfs://")
                    {
                        current_kind = RecentItemKind::Server;
                    } else if path_lower.starts_with("/volumes/") {
                        current_kind = RecentItemKind::Volume;
                    } else if path_lower.starts_with("/") && value.contains(".app") {
                        current_kind = RecentItemKind::Application;
                    } else if path_lower.starts_with("/") {
                        current_kind = RecentItemKind::File;
                    }
                    current_path = value;
                }
                "BundleIdentifier" => {
                    // Application entry
                    current_kind = RecentItemKind::Application;
                    if current_name.is_empty() {
                        current_name = value.clone();
                    }
                    if current_path.is_empty() {
                        current_path = value;
                    }
                }
                _ => {
                    // Put the key back for date check
                    current_key = Some(key_str);
                }
            }
            // key_str consumed or put back above; nothing to do here
        } else if let Some(value) = crate::xml::extract_xml_tag_content(trimmed, "date") {
            if key_str.as_str() == "Date" || key_str.as_str() == "LastUsed" {
                current_date = Some(value);
            }
        } else {
            // Neither string nor date found — put the key back
            current_key = Some(key_str);
        }
    }

    // If XML parsing yielded no items, try generic plist parsing
    if items.is_empty() {
        let entries = crate::plist::parse_xml_plist(xml.as_bytes(), "com.apple.recentitems.plist")?;
        return parse_from_bplist_entries(&entries);
    }

    Ok(items)
}

/// Detect which recent items section we're in from a <key> tag.
fn detect_section_start(line: &str) -> Option<String> {
    if let Some(key) = crate::xml::extract_xml_tag_content(line, "key") {
        match key.as_str() {
            "RecentFiles" | "Files" => return Some("Files".to_string()),
            "RecentServers" | "Servers" | "RecentNetworkServers" => {
                return Some("Servers".to_string())
            }
            "RecentApplications" | "Applications" | "Apps" => {
                return Some("Applications".to_string())
            }
            "RecentVolumes" | "Volumes" => return Some("Volumes".to_string()),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_data() {
        let result = parse_recent_items_plist(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_xml_recent_files() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>RecentFiles</key>
    <array>
        <dict>
            <key>Name</key>
            <string>report.pdf</string>
            <key>URL</key>
            <string>file:///Users/test/Documents/report.pdf</string>
            <key>Date</key>
            <date>2024-01-15T10:30:00Z</date>
        </dict>
        <dict>
            <key>Name</key>
            <string>budget.xlsx</string>
            <key>URL</key>
            <string>file:///Users/test/Documents/budget.xlsx</string>
            <key>Date</key>
            <date>2024-01-14T14:00:00Z</date>
        </dict>
    </array>
</dict>
</plist>"#;

        let items = parse_recent_items_plist(xml.as_bytes()).expect("should parse");
        assert!(!items.is_empty(), "Expected at least one recent item");

        let first = &items[0];
        assert_eq!(first.name, "report.pdf");
        assert!(first.path.contains("report.pdf"));
        assert_eq!(first.kind, RecentItemKind::File);
        assert!(first.last_used.is_some());
    }

    #[test]
    fn parse_xml_recent_servers() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>RecentServers</key>
    <array>
        <dict>
            <key>Name</key>
            <string>fileserver</string>
            <key>URL</key>
            <string>smb://fileserver.local/shared</string>
        </dict>
    </array>
</dict>
</plist>"#;

        let items = parse_recent_items_plist(xml.as_bytes()).expect("should parse");
        assert!(!items.is_empty());

        let server = &items[0];
        assert_eq!(server.name, "fileserver");
        assert_eq!(server.kind, RecentItemKind::Server);
    }

    #[test]
    fn parse_xml_recent_applications() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>RecentApplications</key>
    <array>
        <dict>
            <key>Name</key>
            <string>Safari</string>
            <key>URL</key>
            <string>file:///Applications/Safari.app</string>
        </dict>
    </array>
</dict>
</plist>"#;

        let items = parse_recent_items_plist(xml.as_bytes()).expect("should parse");
        assert!(!items.is_empty());

        let app = &items[0];
        assert_eq!(app.name, "Safari");
        assert_eq!(app.kind, RecentItemKind::Application);
    }
}
