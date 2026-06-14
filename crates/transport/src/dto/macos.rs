use serde::{Deserialize, Serialize};

/// Represents a parsed plist entry (binary or XML).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MacPlistEntryDto {
    pub key: String,
    pub value: String,
    #[serde(rename = "type")]
    pub value_type: String,
    pub source_file: String,
}

/// The type of plist file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum PlistTypeDto {
    Binary,
    Xml,
}

/// A parsed Unified Log (tracev3) entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UnifiedLogEntryDto {
    pub timestamp: String,
    pub process: String,
    pub message: String,
    pub activity_id: String,
    pub thread_id: String,
}

/// A parsed Spotlight index entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpotlightEntryDto {
    pub file_path: String,
    pub display_name: String,
    pub kind: String,
    pub content_type: String,
    pub dates: Vec<String>,
    pub authors: Vec<String>,
}

/// A parsed QuarantineEventsV2 entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineEntryDto {
    pub url: String,
    pub origin_bundle: String,
    pub agent: String,
    pub timestamp: String,
}

/// A parsed Launch Services entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LaunchServiceDto {
    pub label: String,
    pub bundle_id: String,
    pub path: String,
    pub kind: String,
}

/// The kind of recent item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum RecentItemKindDto {
    File,
    Server,
    Application,
    Volume,
    Unknown,
}

/// A parsed recent items entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecentItemDto {
    pub name: String,
    pub path: String,
    pub kind: RecentItemKindDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used: Option<String>,
}

/// The type of FSEvents log event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum FSEventTypeDto {
    Created,
    Removed,
    Modified,
    Renamed,
    Unknown,
}

/// A parsed FSEvents log entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FSEventDto {
    pub path: String,
    pub event_type: FSEventTypeDto,
    pub timestamp: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_entry_dto_serializes_camel_case() {
        let dto = MacPlistEntryDto {
            key: "CFBundleIdentifier".to_string(),
            value: "com.apple.Safari".to_string(),
            value_type: "string".to_string(),
            source_file: "/Applications/Safari.app/Contents/Info.plist".to_string(),
        };
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["key"], "CFBundleIdentifier");
        assert_eq!(json["type"], "string"); // renamed via #[serde(rename = "type")]
        assert_eq!(
            json["sourceFile"],
            "/Applications/Safari.app/Contents/Info.plist"
        );
    }

    #[test]
    fn unified_log_entry_dto_serializes() {
        let dto = UnifiedLogEntryDto {
            timestamp: "2024-01-15T10:30:00Z".to_string(),
            process: "kernel".to_string(),
            message: "System boot".to_string(),
            activity_id: "0x1234".to_string(),
            thread_id: "0x5678".to_string(),
        };
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["process"], "kernel");
        assert_eq!(json["activityId"], "0x1234");
    }

    #[test]
    fn spotlight_entry_dto_serializes() {
        let dto = SpotlightEntryDto {
            file_path: "/Users/test/Documents/report.pdf".to_string(),
            display_name: "report.pdf".to_string(),
            kind: "PDF document".to_string(),
            content_type: "com.adobe.pdf".to_string(),
            dates: vec!["2024-01-15T10:30:00Z".to_string()],
            authors: vec!["John Doe".to_string()],
        };
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["displayName"], "report.pdf");
        assert_eq!(json["authors"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn quarantine_entry_dto_serializes() {
        let dto = QuarantineEntryDto {
            url: "https://example.com/file.dmg".to_string(),
            origin_bundle: "com.google.Chrome".to_string(),
            agent: "com.google.Chrome".to_string(),
            timestamp: "2024-01-15T10:30:00Z".to_string(),
        };
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["originBundle"], "com.google.Chrome");
    }

    #[test]
    fn recent_item_dto_serializes() {
        let dto = RecentItemDto {
            name: "report.pdf".to_string(),
            path: "/Users/test/Documents/report.pdf".to_string(),
            kind: RecentItemKindDto::File,
            last_used: Some("2024-01-15T10:30:00Z".to_string()),
        };
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["name"], "report.pdf");
        assert_eq!(json["kind"], "file");
        assert!(json.get("lastUsed").is_some());
    }

    #[test]
    fn fsevent_dto_serializes() {
        let dto = FSEventDto {
            path: "/Users/test/Documents".to_string(),
            event_type: FSEventTypeDto::Modified,
            timestamp: "2024-01-15T10:30:00Z".to_string(),
        };
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(json["path"], "/Users/test/Documents");
        assert_eq!(json["eventType"], "modified");
        assert_eq!(json["timestamp"], "2024-01-15T10:30:00Z");
    }
}
