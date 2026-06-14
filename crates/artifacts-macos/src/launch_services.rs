//! macOS Launch Services parser.
//!
//! Parses the Launch Services database/plist found at:
//! `~/Library/Preferences/com.apple.LaunchServices.plist`
//!
//! Launch Services manages application registration, file type associations,
//! and URL scheme handlers on macOS. The plist contains entries describing
//! registered applications, their bundle IDs, paths, and handler roles.
//!
//! Common keys in the plist:
//! - `LSHandlers` — array of handler dict entries
//!   - `LSHandlerRoleAll` / `LSHandlerRoleViewer` — handler role
//!   - `LSHandlerURLScheme` — URL scheme (e.g., "http", "mailto")
//!   - `LSHandlerContentType` — UTI content type
//!   - `LSHandlerPreferredVersions` — preferred app versions

use serde::{Deserialize, Serialize};

/// A single Launch Services entry (registered application or handler).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LaunchService {
    /// Display label (bundle name or handler role description)
    pub label: String,
    /// Bundle identifier (e.g., "com.apple.Safari")
    pub bundle_id: String,
    /// Path to the application or handler bundle
    pub path: String,
    /// Kind of service: "Application", "URLHandler", "ContentHandler", "ProtocolHandler"
    pub kind: String,
}

/// Parse a `com.apple.LaunchServices.plist` file.
///
/// This parser handles both XML and binary plist formats by detecting the
/// magic bytes and dispatching to the appropriate parser.
///
/// For XML plists, it extracts:
/// - Application identifiers and paths
/// - URL scheme handlers
/// - Content type handlers
/// - Protocol handlers
pub fn parse_launch_services_plist(data: &[u8]) -> Result<Vec<LaunchService>, String> {
    if data.is_empty() {
        return Err("Launch Services plist data is empty".to_string());
    }

    // Detect format and parse
    let text = if crate::plist::is_binary_plist(data) {
        // For binary plist, use the existing parser
        let entries = crate::plist::parse_binary_plist(data, "com.apple.LaunchServices.plist")?;
        return parse_from_plist_entries(&entries);
    } else {
        // Treat as XML
        std::str::from_utf8(data)
            .map_err(|_| "Launch Services plist is not valid UTF-8".to_string())?
            .to_string()
    };

    parse_xml_launch_services(&text)
}

/// Convert plist entries to LaunchService structs.
fn parse_from_plist_entries(
    entries: &[crate::plist::MacPlistEntry],
) -> Result<Vec<LaunchService>, String> {
    let mut services: Vec<LaunchService> = Vec::new();

    for entry in entries {
        match entry.key.as_str() {
            "CFBundleIdentifier" => {
                services.push(LaunchService {
                    label: entry.value.clone(),
                    bundle_id: entry.value.clone(),
                    path: String::new(),
                    kind: "Application".to_string(),
                });
            }
            "LSHandlerURLScheme" => {
                services.push(LaunchService {
                    label: format!("URL Handler: {}", entry.value),
                    bundle_id: String::new(),
                    path: String::new(),
                    kind: "URLHandler".to_string(),
                });
            }
            "LSHandlerContentType" => {
                services.push(LaunchService {
                    label: format!("Content Handler: {}", entry.value),
                    bundle_id: String::new(),
                    path: String::new(),
                    kind: "ContentHandler".to_string(),
                });
            }
            _ => {
                // Include other interesting keys as services
                if entry.value_type == "string" && !entry.value.is_empty() {
                    services.push(LaunchService {
                        label: entry.key.clone(),
                        bundle_id: entry.value.clone(),
                        path: String::new(),
                        kind: "Unknown".to_string(),
                    });
                }
            }
        }
    }

    Ok(services)
}

/// Parse XML Launch Services plist.
fn parse_xml_launch_services(xml: &str) -> Result<Vec<LaunchService>, String> {
    let mut services: Vec<LaunchService> = Vec::new();

    // State machine for parsing LSHandlers array
    let mut in_dict = false;
    let mut current_bundle_id = String::new();
    let mut current_role = String::new();
    let mut current_scheme = String::new();
    let mut current_content_type = String::new();
    let mut current_key: Option<String> = None;

    for line in xml.lines() {
        let trimmed = line.trim();

        if trimmed.contains("<dict>") {
            in_dict = true;
            continue;
        }
        if trimmed.contains("</dict>") {
            // Emit the current entry if we have enough info
            if in_dict && !current_bundle_id.is_empty() {
                let label = if !current_scheme.is_empty() {
                    format!("{} ({})", current_bundle_id, current_scheme)
                } else if !current_content_type.is_empty() {
                    format!("{} ({})", current_bundle_id, current_content_type)
                } else {
                    current_bundle_id.clone()
                };

                let kind = if !current_scheme.is_empty() {
                    "URLHandler"
                } else if !current_content_type.is_empty() {
                    "ContentHandler"
                } else {
                    "Application"
                };

                services.push(LaunchService {
                    label,
                    bundle_id: std::mem::take(&mut current_bundle_id),
                    path: String::new(),
                    kind: kind.to_string(),
                });
            }
            current_bundle_id.clear();
            current_role.clear();
            current_scheme.clear();
            current_content_type.clear();
            current_key = None;
            in_dict = false;
            continue;
        }

        if !in_dict {
            continue;
        }

        // Extract <key>...</key>
        if let Some(key) = extract_xml_tag_content(trimmed, "key") {
            current_key = Some(key);
            continue;
        }

        // Extract value based on current key
        if let Some(ref key) = current_key {
            if let Some(value) = extract_xml_tag_content(trimmed, "string") {
                match key.as_str() {
                    "LSHandlerRoleAll" | "LSHandlerRoleViewer" | "LSHandlerRoleEditor" => {
                        current_bundle_id = value;
                        current_role = key.clone();
                    }
                    "LSHandlerURLScheme" => {
                        current_scheme = value;
                    }
                    "LSHandlerContentType" => {
                        current_content_type = value;
                    }
                    "CFBundleIdentifier" => {
                        current_bundle_id = value;
                    }
                    _ => {}
                }
                current_key = None;
            }
        }
    }

    // If we only got the plist-level entries, also try standard plist parsing
    if services.is_empty() {
        let entries = crate::plist::parse_xml_plist(
            data_from_str(xml).as_slice(),
            "com.apple.LaunchServices.plist",
        )?;
        return parse_from_plist_entries(&entries);
    }

    Ok(services)
}

/// Extract content from an XML tag like `<tag>content</tag>`.
fn extract_xml_tag_content(line: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    if let (Some(start), Some(end)) = (line.find(&open), line.find(&close)) {
        let content_start = start + open.len();
        if content_start < end {
            return Some(line[content_start..end].to_string());
        }
        return Some(String::new());
    }
    None
}

/// Convert a string to bytes (for fallback plist parsing).
fn data_from_str(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_data() {
        let result = parse_launch_services_plist(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_xml_launch_services_with_handlers() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>LSHandlers</key>
    <array>
        <dict>
            <key>LSHandlerURLScheme</key>
            <string>http</string>
            <key>LSHandlerRoleAll</key>
            <string>com.apple.Safari</string>
        </dict>
        <dict>
            <key>LSHandlerURLScheme</key>
            <string>mailto</string>
            <key>LSHandlerRoleViewer</key>
            <string>com.apple.mail</string>
        </dict>
        <dict>
            <key>LSHandlerContentType</key>
            <string>com.adobe.pdf</string>
            <key>LSHandlerRoleAll</key>
            <string>com.apple.Preview</string>
        </dict>
    </array>
</dict>
</plist>"#;

        let services = parse_launch_services_plist(xml.as_bytes()).expect("should parse");
        assert!(!services.is_empty(), "Expected at least one service");

        // Check for URL handler
        let safari = services.iter().find(|s| s.bundle_id.contains("Safari"));
        assert!(safari.is_some(), "Should find Safari handler");
        let safari = safari.unwrap();
        assert_eq!(safari.kind, "URLHandler");

        // Check for content handler
        let preview = services.iter().find(|s| s.bundle_id.contains("Preview"));
        assert!(preview.is_some(), "Should find Preview handler");
        assert_eq!(preview.unwrap().kind, "ContentHandler");
    }

    #[test]
    fn parse_xml_launch_services_empty_handlers() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>LSHandlers</key>
    <array>
    </array>
</dict>
</plist>"#;

        let services = parse_launch_services_plist(xml.as_bytes()).expect("should parse");
        // Should return empty or minimal
        // With our fallback to generic plist parsing, we may get few entries
        // but they should all parse
        for s in &services {
            assert!(!s.label.is_empty());
        }
    }

    #[test]
    fn parse_launch_services_with_bundle_id() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.apple.finder</string>
    <key>CFBundleName</key>
    <string>Finder</string>
    <key>CFBundleVersion</key>
    <string>14.0</string>
</dict>
</plist>"#;

        let services = parse_launch_services_plist(xml.as_bytes()).expect("should parse");
        assert!(!services.is_empty(), "Expected entries from plist");
    }
}
