use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AnalysisParseStatusDto {
    Parsed,
    NotParsed,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisProvenanceDto {
    pub data_source_id: String,
    pub artifact_path: String,
    pub parser: String,
    pub parsed_at: String,
    pub status: AnalysisParseStatusDto,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisSystemInfoDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub computer_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registered_owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_id: Option<String>,
    pub network_adapters: Vec<AnalysisNetworkAdapterDto>,
    pub boot_history: Vec<AnalysisBootRecordDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub status: AnalysisParseStatusDto,
    pub warnings: Vec<String>,
    pub provenance: Vec<AnalysisProvenanceDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisNetworkAdapterDto {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mac_address: Option<String>,
    pub ip_addresses: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dhcp_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dhcp_server: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisBootRecordDto {
    pub timestamp: String,
    pub boot_type: String,
    pub source: String,
    pub provenance: AnalysisProvenanceDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisFileClassificationDto {
    pub category: String,
    pub files: Vec<AnalysisClassifiedFileDto>,
    pub total_size: u64,
    pub status: AnalysisParseStatusDto,
    pub warnings: Vec<String>,
    pub provenance: Vec<AnalysisProvenanceDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisClassifiedFileDto {
    pub file_id: String,
    pub path: String,
    pub name: String,
    pub size: u64,
    pub file_type: String,
    pub magic_description: String,
    pub provenance: AnalysisProvenanceDto,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_info_serializes_camel_case_and_status() {
        let dto = AnalysisSystemInfoDto {
            computer_name: Some("host".to_string()),
            os_version: None,
            build_number: None,
            install_date: None,
            registered_owner: None,
            organization: None,
            product_id: None,
            network_adapters: vec![AnalysisNetworkAdapterDto {
                name: "Ethernet".to_string(),
                mac_address: Some("00:11:22:33:44:55".to_string()),
                ip_addresses: vec!["192.0.2.10".to_string()],
                dhcp_enabled: Some(true),
                dhcp_server: None,
            }],
            boot_history: vec![AnalysisBootRecordDto {
                timestamp: "2026-01-01T00:00:00Z".to_string(),
                boot_type: "normal".to_string(),
                source: "fixture".to_string(),
                provenance: AnalysisProvenanceDto {
                    data_source_id: "ds-1".to_string(),
                    artifact_path: "Windows/System32/winevt/Logs/System.evtx".to_string(),
                    parser: "evtx.boot_shutdown".to_string(),
                    parsed_at: "2026-01-01T00:00:00+00:00".to_string(),
                    status: AnalysisParseStatusDto::Parsed,
                    warnings: Vec::new(),
                },
            }],
            timezone: None,
            language: None,
            status: AnalysisParseStatusDto::NotParsed,
            warnings: vec!["parser unavailable".to_string()],
            provenance: vec![AnalysisProvenanceDto {
                data_source_id: "ds-1".to_string(),
                artifact_path: "Windows/System32/config/SYSTEM".to_string(),
                parser: "registry.system".to_string(),
                parsed_at: "2026-01-01T00:00:00+00:00".to_string(),
                status: AnalysisParseStatusDto::NotParsed,
                warnings: vec!["value traversal unavailable".to_string()],
            }],
        };

        let json = serde_json::to_value(dto).unwrap();
        assert_eq!(json["computerName"], "host");
        assert_eq!(
            json["networkAdapters"][0]["macAddress"],
            "00:11:22:33:44:55"
        );
        assert_eq!(json["bootHistory"][0]["bootType"], "normal");
        assert_eq!(json["status"], "notParsed");
        assert_eq!(json["provenance"][0]["dataSourceId"], "ds-1");
        assert_eq!(
            json["provenance"][0]["artifactPath"],
            "Windows/System32/config/SYSTEM"
        );
        assert_eq!(
            json["provenance"][0]["parsedAt"],
            "2026-01-01T00:00:00+00:00"
        );
        assert!(json.get("computer_name").is_none());
    }

    #[test]
    fn provenance_serializes_required_camel_case_fields() {
        let dto = AnalysisProvenanceDto {
            data_source_id: "ds".to_string(),
            artifact_path: "Windows/System32/winevt/Logs/System.evtx".to_string(),
            parser: "evtx.boot_shutdown".to_string(),
            parsed_at: "2026-01-01T00:00:00+00:00".to_string(),
            status: AnalysisParseStatusDto::Unavailable,
            warnings: vec!["EVTX parser is unavailable".to_string()],
        };

        let json = serde_json::to_value(dto).unwrap();
        assert_eq!(json["dataSourceId"], "ds");
        assert_eq!(
            json["artifactPath"],
            "Windows/System32/winevt/Logs/System.evtx"
        );
        assert_eq!(json["parser"], "evtx.boot_shutdown");
        assert_eq!(json["parsedAt"], "2026-01-01T00:00:00+00:00");
        assert_eq!(json["status"], "unavailable");
        assert_eq!(json["warnings"][0], "EVTX parser is unavailable");
        assert!(json.get("data_source_id").is_none());
    }

    #[test]
    fn classification_serializes_camel_case() {
        let dto = AnalysisFileClassificationDto {
            category: "Documents".to_string(),
            files: vec![AnalysisClassifiedFileDto {
                file_id: "file-1".to_string(),
                path: "doc.pdf".to_string(),
                name: "doc.pdf".to_string(),
                size: 4,
                file_type: "PDF".to_string(),
                magic_description: "PDF Document".to_string(),
                provenance: AnalysisProvenanceDto {
                    data_source_id: "ds-1".to_string(),
                    artifact_path: "doc.pdf".to_string(),
                    parser: "analysis.magic".to_string(),
                    parsed_at: "2026-01-01T00:00:00+00:00".to_string(),
                    status: AnalysisParseStatusDto::Parsed,
                    warnings: Vec::new(),
                },
            }],
            total_size: 4,
            status: AnalysisParseStatusDto::Parsed,
            warnings: Vec::new(),
            provenance: vec![AnalysisProvenanceDto {
                data_source_id: "ds-1".to_string(),
                artifact_path: "doc.pdf".to_string(),
                parser: "analysis.magic".to_string(),
                parsed_at: "2026-01-01T00:00:00+00:00".to_string(),
                status: AnalysisParseStatusDto::Parsed,
                warnings: Vec::new(),
            }],
        };

        let json = serde_json::to_value(dto).unwrap();
        assert_eq!(json["files"][0]["fileId"], "file-1");
        assert_eq!(json["totalSize"], 4);
        assert_eq!(json["files"][0]["fileType"], "PDF");
        assert_eq!(json["files"][0]["magicDescription"], "PDF Document");
        assert_eq!(json["files"][0]["provenance"]["dataSourceId"], "ds-1");
        assert_eq!(json["provenance"][0]["artifactPath"], "doc.pdf");
    }
}
