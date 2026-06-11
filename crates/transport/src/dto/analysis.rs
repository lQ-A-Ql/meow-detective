use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AnalysisParseStatusDto {
    Parsed,
    Partial,
    NotParsed,
    Unavailable,
    CandidateFound,
    NotFound,
    Failed,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisFieldProvenanceDto {
    pub field: String,
    pub value_name: String,
    pub key_path: String,
    pub hive_path: String,
    pub parser: String,
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
    pub field_provenance: Vec<AnalysisFieldProvenanceDto>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub provenance: AnalysisProvenanceDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisFileClassificationDto {
    pub category: String,
    pub files: Vec<AnalysisClassifiedFileDto>,
    pub file_count: u64,
    pub total_size: u64,
    pub status: AnalysisParseStatusDto,
    pub warnings: Vec<String>,
    pub provenance: Vec<AnalysisProvenanceDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceClassificationSummaryDto {
    pub status: AnalysisParseStatusDto,
    pub categories: Vec<EvidenceCategoryDto>,
    pub totals: EvidenceClassificationTotalsDto,
    pub generated_at: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceClassificationTotalsDto {
    pub category_count: u64,
    pub candidate_file_count: u64,
    pub total_size: u64,
    pub artifact_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceCategoryDto {
    pub category: String,
    pub display_name: String,
    pub status: AnalysisParseStatusDto,
    pub file_count: u64,
    pub total_size: u64,
    pub artifact_count: u64,
    pub confidence: f32,
    pub sources: Vec<EvidenceSourceDto>,
    pub warnings: Vec<String>,
    pub provenance: Vec<AnalysisProvenanceDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceSourceDto {
    pub file_id: String,
    pub path: String,
    pub size: u64,
    pub evidence_kind: String,
    pub parser: String,
    pub status: AnalysisParseStatusDto,
    pub artifact_count: u64,
    pub warnings: Vec<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisExtractionRunDto {
    pub status: AnalysisParseStatusDto,
    pub scanned_count: u64,
    pub artifact_count: u64,
    pub timeline_event_count: u64,
    pub generated_at: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryExtractionSummaryDto {
    pub status: AnalysisParseStatusDto,
    pub total: u64,
    pub values: Vec<RegistryValueDto>,
    pub generated_at: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryValueDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub hive_path: String,
    pub key_path: String,
    pub value_name: String,
    pub value_type: String,
    pub data: String,
    pub parser: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHistorySummaryDto {
    pub status: AnalysisParseStatusDto,
    pub visit_total: u64,
    pub download_total: u64,
    pub visits: Vec<BrowserVisitDto>,
    pub downloads: Vec<BrowserDownloadDto>,
    pub generated_at: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserVisitDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub browser: String,
    pub profile: String,
    pub url: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visit_time: Option<String>,
    pub visit_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserDownloadDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    pub browser: String,
    pub profile: String,
    pub url: String,
    pub target_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_time: Option<String>,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailExtractionSummaryDto {
    pub status: AnalysisParseStatusDto,
    pub total: u64,
    pub messages: Vec<EmailMessageDto>,
    pub generated_at: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailMessageDto {
    pub artifact_id: String,
    pub file_id: String,
    pub source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sent_at: Option<String>,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub message_id: String,
    pub attachments: Vec<String>,
    pub body_preview: String,
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
                boot_type: "eventLogStarted".to_string(),
                source: "Windows/System32/winevt/Logs/System.evtx".to_string(),
                event_id: Some(6005),
                record_id: Some(42),
                note: Some("EventLog 6005 candidate, not a direct boot assertion".to_string()),
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
            field_provenance: vec![AnalysisFieldProvenanceDto {
                field: "computerName".to_string(),
                value_name: "ComputerName".to_string(),
                key_path: "ControlSet001\\Control\\ComputerName\\ComputerName".to_string(),
                hive_path: "Windows/System32/config/SYSTEM".to_string(),
                parser: "registry.system".to_string(),
            }],
        };

        let json = serde_json::to_value(dto).unwrap();
        assert_eq!(json["computerName"], "host");
        assert_eq!(
            json["networkAdapters"][0]["macAddress"],
            "00:11:22:33:44:55"
        );
        assert_eq!(json["bootHistory"][0]["bootType"], "eventLogStarted");
        assert_eq!(json["bootHistory"][0]["eventId"], 6005);
        assert_eq!(json["bootHistory"][0]["recordId"], 42);
        assert_eq!(
            json["bootHistory"][0]["note"],
            "EventLog 6005 candidate, not a direct boot assertion"
        );
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
        assert_eq!(json["fieldProvenance"][0]["field"], "computerName");
        assert_eq!(json["fieldProvenance"][0]["valueName"], "ComputerName");
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
    fn current_provenance_contract_is_bounded_to_source_attribution() {
        let dto = EvidenceCategoryDto {
            category: "ProgramExecution".to_string(),
            display_name: "Program execution".to_string(),
            status: AnalysisParseStatusDto::Parsed,
            file_count: 1,
            total_size: 98_304,
            artifact_count: 2,
            confidence: 0.95,
            sources: vec![EvidenceSourceDto {
                file_id: "file-prefetch".to_string(),
                path: "Windows/Prefetch/CMD.EXE-12345678.pf".to_string(),
                size: 98_304,
                evidence_kind: "execution_artifact".to_string(),
                parser: "prefetch".to_string(),
                status: AnalysisParseStatusDto::Parsed,
                artifact_count: 2,
                warnings: Vec::new(),
            }],
            warnings: Vec::new(),
            provenance: vec![AnalysisProvenanceDto {
                data_source_id: "ds-001".to_string(),
                artifact_path: "Windows/Prefetch/CMD.EXE-12345678.pf".to_string(),
                parser: "prefetch".to_string(),
                parsed_at: "2026-01-01T00:00:00+00:00".to_string(),
                status: AnalysisParseStatusDto::Parsed,
                warnings: Vec::new(),
            }],
        };

        let json = serde_json::to_value(dto).unwrap();
        assert!((json["confidence"].as_f64().unwrap() - 0.95).abs() < 0.000_001);
        assert_eq!(json["sources"][0]["fileId"], "file-prefetch");
        assert_eq!(json["sources"][0]["evidenceKind"], "execution_artifact");
        assert_eq!(json["sources"][0]["parser"], "prefetch");
        assert_eq!(json["provenance"][0]["dataSourceId"], "ds-001");
        assert_eq!(
            json["provenance"][0]["artifactPath"],
            "Windows/Prefetch/CMD.EXE-12345678.pf"
        );
        assert_eq!(json["provenance"][0]["parser"], "prefetch");
        assert!(json["sources"][0].get("file_id").is_none());
        assert!(json["provenance"][0].get("data_source_id").is_none());
        assert!(json["provenance"][0].get("sourceHash").is_none());
        assert!(json["provenance"][0].get("parserVersion").is_none());
    }

    #[test]
    #[ignore = "future provenance contract: add after DataSource/Artifact/Timeline schema migrations"]
    fn future_provenance_contract_includes_hash_version_and_confidence() {
        let dto = AnalysisProvenanceDto {
            data_source_id: "ds-001".to_string(),
            artifact_path: "Windows/System32/config/SYSTEM".to_string(),
            parser: "registry.system".to_string(),
            parsed_at: "2026-01-01T00:00:00+00:00".to_string(),
            status: AnalysisParseStatusDto::Parsed,
            warnings: Vec::new(),
        };

        let json = serde_json::to_value(dto).unwrap();
        assert_eq!(json["dataSourceId"], "ds-001");
        assert_eq!(json["artifactPath"], "Windows/System32/config/SYSTEM");
        assert_eq!(json["parser"], "registry.system");
        assert!(json.get("sourceHash").is_some());
        assert!(json.get("parserVersion").is_some());
        assert!(json.get("confidence").is_some());
        assert!(json.get("sourceAttribution").is_some());
        assert!(json.get("source_hash").is_none());
        assert!(json.get("parser_version").is_none());
        assert!(json.get("source_attribution").is_none());
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
            file_count: 1,
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
        assert_eq!(json["fileCount"], 1);
        assert_eq!(json["totalSize"], 4);
        assert_eq!(json["files"][0]["fileType"], "PDF");
        assert_eq!(json["files"][0]["magicDescription"], "PDF Document");
        assert_eq!(json["files"][0]["provenance"]["dataSourceId"], "ds-1");
        assert_eq!(json["provenance"][0]["artifactPath"], "doc.pdf");
    }
}
