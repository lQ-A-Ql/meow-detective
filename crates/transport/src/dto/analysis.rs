pub use crate::dto::analysis_base::{
    AnalysisExtractionRunDto, AnalysisExtractionSectionRunDto, AnalysisFieldProvenanceDto,
    AnalysisParseStatusDto, AnalysisProvenanceDto,
};
pub use crate::dto::analysis_browser::{
    BrowserCookieDto, BrowserDownloadDto, BrowserHistorySummaryDto, BrowserPasswordDto,
    BrowserSessionTabDto, BrowserVisitDto,
};
pub use crate::dto::analysis_classification::{
    AnalysisClassifiedFileDto, AnalysisFileClassificationDto, EvidenceCategoryDto,
    EvidenceClassificationSummaryDto, EvidenceClassificationTotalsDto, EvidenceSourceDto,
};
pub use crate::dto::analysis_email::{
    EmailAttachmentDto, EmailExtractionSummaryDto, EmailHeaderDto, EmailMessageDto,
};
pub use crate::dto::analysis_evtx::{
    EvtxApplicationEventDto, EvtxBootEventDto, EvtxEventSummaryDto, EvtxSecurityEventDto,
};
pub use crate::dto::analysis_linux::{
    LinuxAptEventDto, LinuxArtifactSummaryDto, LinuxBashCommandDto, LinuxCronJobDto,
    LinuxJournalEntryDto, LinuxLoginRecordDto, LinuxMysqlConfigDto, LinuxMysqlFindingDto,
    LinuxMysqlLogEntryDto, LinuxSudoEventDto, LinuxSystemConfigDto, LinuxWebAccessLogDto,
    LinuxWebErrorLogDto, LinuxWebFindingDto, LinuxWebSiteDto,
};
pub use crate::dto::analysis_registry::{
    AmcacheApplicationDto, AmcacheApplicationFileDto, AppCompatLayerDto, CachedCredentialDto,
    InstalledSoftwareDto, LastVisitedMruEntryDto, LsaPackageDto, LsaSecretDto, MountedDeviceDto,
    MuiCacheEntryDto, NetworkProfileDto, OpenSaveMruEntryDto, RegistryExtractionSummaryDto,
    RegistryHiveOverviewDto, RegistryStructuredSummaryDto, RegistryValueDto, RunMruEntryDto,
    SamUserAccountDto, SecurityPolicyDto, ShellbagEntryDto, ShimCacheEntryDto, ShutdownTimeDto,
    SystemServiceDto, UsbDeviceHistoryDto, UserAssistEntryDto, WinlogonConfigDto,
};
pub use crate::dto::analysis_system::{
    AnalysisBootRecordDto, AnalysisNetworkAdapterDto, AnalysisSystemInfoDto,
};
pub use crate::dto::governance::{
    BenchmarkRequiredCheckDto, BenchmarkRequirementStatusDto, BenchmarkSnapshotDto,
    BenchmarkSummaryDto, CorrelationCoverageStatusDto, CorrelationFamilyCoverageDto,
    ErrorTaxonomyEntryDto, GovernanceFactSourceDto, GovernanceRuntimeCheckDto,
    GovernanceRuntimeResultsDto, GovernanceRuntimeSignalsDto, GovernanceRuntimeSubcheckDto,
    KnownLimitationDto, KnownLimitationStatusDto, ParserSupportMatrixEntryDto,
    ParserSupportMatrixSummaryDto, ReleaseGateEntryDto, ReleaseGateStatusDto,
    ReleaseScoreBreakdownEntryDto, ReleaseScorecardDto, SecurityAuditEntryDto,
    SecurityAuditSummaryDto, SupportMaturityDto, V2GovernanceSnapshotDto,
    VerificationChainStatusDto, VerificationGuaranteeLevelDto, VerificationResultDto,
};

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
                details: std::collections::BTreeMap::new(),
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

    #[test]
    fn governance_snapshot_serializes_camel_case() {
        let dto = V2GovernanceSnapshotDto {
            generated_at: "2026-06-12T00:00:00Z".to_string(),
            fact_sources: vec![
                GovernanceFactSourceDto {
                    area: "verification".to_string(),
                    fact_file: "testdata/governance/v2-verification-catalog.json".to_string(),
                    fact_kind: "catalog".to_string(),
                    derived_outputs: vec![
                        "verificationChains".to_string(),
                        "supportMatrixEntries".to_string(),
                        "supportMatrix".to_string(),
                    ],
                    last_verified_at: "2026-06-12T00:00:00Z".to_string(),
                },
                GovernanceFactSourceDto {
                    area: "releasePolicy".to_string(),
                    fact_file: "testdata/governance/v2-release-policy.json".to_string(),
                    fact_kind: "policy".to_string(),
                    derived_outputs: vec![
                        "releaseGates".to_string(),
                        "releaseScorecard".to_string(),
                    ],
                    last_verified_at: "2026-06-13T00:00:00Z".to_string(),
                },
                GovernanceFactSourceDto {
                    area: "knownLimitations".to_string(),
                    fact_file: "testdata/governance/v2-known-limitations.json".to_string(),
                    fact_kind: "catalog".to_string(),
                    derived_outputs: vec![
                        "knownLimitations".to_string(),
                        "supportMatrix.documentedLimitCount".to_string(),
                    ],
                    last_verified_at: "2026-06-13T00:00:00Z".to_string(),
                },
            ],
            runtime_results: GovernanceRuntimeResultsDto {
                checked_at: "2026-06-13T00:00:00Z".to_string(),
                checks: vec![GovernanceRuntimeCheckDto {
                    check_id: "docs-drift".to_string(),
                    title: "文档防漂移".to_string(),
                    status: ReleaseGateStatusDto::Passed,
                    evidence: "scripts/check-doc-drift.ps1".to_string(),
                    detail: "README / AGENTS / documentation-index / Mermaid 图块数量一致"
                        .to_string(),
                    checked_at: "2026-06-13T00:00:00Z".to_string(),
                    sub_checks: vec![GovernanceRuntimeSubcheckDto {
                        check_id: "readme-fact-sync".to_string(),
                        title: "README 事实同步".to_string(),
                        status: ReleaseGateStatusDto::Passed,
                        evidence: "crate/page/command counts match".to_string(),
                        detail: "README 关键事实与仓库扫描结果一致".to_string(),
                    }],
                }],
            },
            verification_chains: vec![VerificationChainStatusDto {
                chain: "NTFS".to_string(),
                display_name: "NTFS 文件系统".to_string(),
                maturity: SupportMaturityDto::Ga,
                guarantee_level: VerificationGuaranteeLevelDto::Guaranteed,
                fixture_tier: "public-small".to_string(),
                expected_json_version: "v1".to_string(),
                verified_sample_count: 3,
                result: VerificationResultDto::Passed,
                notes: vec!["validated".to_string()],
            }],
            support_matrix: ParserSupportMatrixSummaryDto {
                ga_count: 6,
                beta_count: 2,
                experimental_count: 1,
                unsupported_count: 4,
                documented_limit_count: 1,
            },
            support_matrix_entries: vec![ParserSupportMatrixEntryDto {
                chain: "NTFS".to_string(),
                platform: "Windows".to_string(),
                maturity: SupportMaturityDto::Ga,
                verified_samples: vec![
                    "tiny.raw".to_string(),
                    "synthetic ntfs fixture".to_string(),
                ],
                baseline: "fixture assertions / expected.json".to_string(),
                guarantee_summary: "deleted/hidden/system/orphan 为 guaranteed".to_string(),
                notes: vec!["复杂损坏样本仍不足".to_string()],
            }],
            known_limitations: vec![KnownLimitationDto {
                category: "E01".to_string(),
                item: "多段复杂样本".to_string(),
                status: KnownLimitationStatusDto::Partial,
                summary: "当前公开样本主要覆盖 tiny 单段".to_string(),
                affected_chains: vec!["E01".to_string()],
                source_doc: "docs/known-unsupported-formats.md".to_string(),
            }],
            benchmark: BenchmarkSummaryDto {
                host_profile: "Windows 11 / 32GB RAM / NVMe".to_string(),
                baseline_version: "2026.06".to_string(),
                last_verified_at: "2026-06-12T00:00:00Z".to_string(),
                scenarios: vec![BenchmarkSnapshotDto {
                    dataset_level: "medium".to_string(),
                    scenario: "search warm query".to_string(),
                    p95_ms: 1500,
                    memory_peak_mb: Some(2048),
                    baseline_version: "2026.06".to_string(),
                }],
                required_checks: vec![BenchmarkRequiredCheckDto {
                    dataset_level: "medium".to_string(),
                    scenario: "search warm query".to_string(),
                    threshold_p95_ms: 1500,
                    measured_p95_ms: Some(1500),
                    status: BenchmarkRequirementStatusDto::Covered,
                }],
                covered_required_count: 1,
                missing_required_count: 0,
                exceeded_required_count: 0,
            },
            security: SecurityAuditSummaryDto {
                export_overwrite_default: false,
                export_path_guard_enabled: true,
                stdio_command_whitelist_enforced: true,
                sse_https_only: true,
                embedded_credentials_blocked: true,
                media_handle_scoped: true,
                error_redaction_enabled: true,
                audit_log_required: true,
                audit_event_count: 6,
                sensitive_audit_event_count: 4,
                recent_audit_entries: vec![SecurityAuditEntryDto {
                    action: "mcp.tool.call".to_string(),
                    resource_type: "mcp".to_string(),
                    resource_id: Some("triage-server".to_string()),
                    created_at: "2026-06-12T00:10:00Z".to_string(),
                    summary: Some("status=ok; toolName=query_fixture_catalog".to_string()),
                    sensitive: true,
                }],
                notes: vec!["audit".to_string()],
            },
            error_taxonomy_entries: vec![ErrorTaxonomyEntryDto {
                category: "security".to_string(),
                severity: "high".to_string(),
                recoverable: false,
                examples: vec!["MCP policy block".to_string()],
                redaction_rule: "never expose credentials or raw absolute paths".to_string(),
                notes: vec!["frontend only receives sanitized messages".to_string()],
            }],
            release_gates: vec![ReleaseGateEntryDto {
                gate_id: "docs-drift".to_string(),
                title: "文档防漂移".to_string(),
                status: ReleaseGateStatusDto::Passed,
                evidence: "scripts/check-doc-drift.ps1".to_string(),
                detail: "README / AGENTS / 文档索引与 Mermaid 图块数量一致".to_string(),
            }],
            release_scorecard: ReleaseScorecardDto {
                total_score: 84,
                grade: "B".to_string(),
                verification_score: 26,
                correlation_score: 18,
                performance_score: 16,
                security_score: 24,
                breakdown: vec![ReleaseScoreBreakdownEntryDto {
                    dimension: "verification".to_string(),
                    max_score: 30,
                    actual_score: 26,
                    deductions: vec!["pending hash data source".to_string()],
                }],
                blockers: vec!["private regression pending".to_string()],
                residual_risks: vec!["browser fixture medium only".to_string()],
            },
            runtime_signals: GovernanceRuntimeSignalsDto {
                data_source_count: 2,
                hashed_data_source_count: 1,
                pending_hash_data_source_count: 1,
                warning_data_source_count: 1,
                running_job_count: 0,
                partial_job_count: 1,
                failed_job_count: 0,
                report_count: 2,
                correlation_snapshot_available: true,
                correlation_lead_count: 4,
                correlation_high_confidence_lead_count: 3,
                correlation_review_lead_count: 2,
                correlation_cluster_count: 3,
                correlation_rule_family_count: 7,
                correlation_covered_family_count: 3,
                correlation_high_confidence_family_count: 2,
                correlation_family_coverage: vec![
                    CorrelationFamilyCoverageDto {
                        family: "LNK".to_string(),
                        display_name: "LNK".to_string(),
                        status: CorrelationCoverageStatusDto::Covered,
                        lead_count: 1,
                        high_confidence_lead_count: 1,
                        review_lead_count: 0,
                        cluster_count: 1,
                        sample_signals: vec!["LNK 目标路径命中文件路径".to_string()],
                    },
                    CorrelationFamilyCoverageDto {
                        family: "Registry".to_string(),
                        display_name: "Registry".to_string(),
                        status: CorrelationCoverageStatusDto::Review,
                        lead_count: 1,
                        high_confidence_lead_count: 0,
                        review_lead_count: 1,
                        cluster_count: 1,
                        sample_signals: vec!["Registry 值数据命中文件路径".to_string()],
                    },
                ],
            },
        };

        let json = serde_json::to_value(dto).unwrap();
        assert_eq!(json["generatedAt"], "2026-06-12T00:00:00Z");
        assert_eq!(json["factSources"][0]["area"], "verification");
        assert_eq!(json["factSources"][1]["factKind"], "policy");
        assert_eq!(
            json["factSources"][2]["factFile"],
            "testdata/governance/v2-known-limitations.json"
        );
        assert_eq!(json["runtimeResults"]["checkedAt"], "2026-06-13T00:00:00Z");
        assert_eq!(json["runtimeResults"]["checks"][0]["checkId"], "docs-drift");
        assert_eq!(
            json["runtimeResults"]["checks"][0]["subChecks"][0]["checkId"],
            "readme-fact-sync"
        );
        assert_eq!(
            json["verificationChains"][0]["displayName"],
            "NTFS 文件系统"
        );
        assert_eq!(
            json["verificationChains"][0]["guaranteeLevel"],
            "guaranteed"
        );
        assert_eq!(json["supportMatrix"]["gaCount"], 6);
        assert_eq!(json["supportMatrixEntries"][0]["chain"], "NTFS");
        assert_eq!(
            json["supportMatrixEntries"][0]["verifiedSamples"][0],
            "tiny.raw"
        );
        assert_eq!(json["knownLimitations"][0]["category"], "E01");
        assert_eq!(json["knownLimitations"][0]["status"], "partial");
        assert_eq!(json["knownLimitations"][0]["affectedChains"][0], "E01");
        assert_eq!(
            json["benchmark"]["hostProfile"],
            "Windows 11 / 32GB RAM / NVMe"
        );
        assert_eq!(
            json["benchmark"]["requiredChecks"][0]["thresholdP95Ms"],
            1500
        );
        assert_eq!(
            json["benchmark"]["requiredChecks"][0]["measuredP95Ms"],
            1500
        );
        assert_eq!(json["benchmark"]["requiredChecks"][0]["status"], "covered");
        assert_eq!(json["benchmark"]["coveredRequiredCount"], 1);
        assert_eq!(json["benchmark"]["missingRequiredCount"], 0);
        assert_eq!(json["benchmark"]["exceededRequiredCount"], 0);
        assert_eq!(json["security"]["exportOverwriteDefault"], false);
        assert_eq!(json["security"]["auditEventCount"], 6);
        assert_eq!(json["security"]["sensitiveAuditEventCount"], 4);
        assert_eq!(
            json["security"]["recentAuditEntries"][0]["action"],
            "mcp.tool.call"
        );
        assert_eq!(
            json["security"]["recentAuditEntries"][0]["resourceType"],
            "mcp"
        );
        assert_eq!(json["errorTaxonomyEntries"][0]["category"], "security");
        assert_eq!(json["releaseGates"][0]["gateId"], "docs-drift");
        assert_eq!(json["releaseScorecard"]["totalScore"], 84);
        assert_eq!(
            json["releaseScorecard"]["breakdown"][0]["dimension"],
            "verification"
        );
        assert_eq!(json["runtimeSignals"]["dataSourceCount"], 2);
        assert_eq!(json["runtimeSignals"]["correlationSnapshotAvailable"], true);
        assert_eq!(json["runtimeSignals"]["correlationLeadCount"], 4);
        assert_eq!(
            json["runtimeSignals"]["correlationHighConfidenceLeadCount"],
            3
        );
        assert_eq!(json["runtimeSignals"]["correlationReviewLeadCount"], 2);
        assert_eq!(json["runtimeSignals"]["correlationClusterCount"], 3);
        assert_eq!(json["runtimeSignals"]["correlationRuleFamilyCount"], 7);
        assert_eq!(json["runtimeSignals"]["correlationCoveredFamilyCount"], 3);
        assert_eq!(
            json["runtimeSignals"]["correlationHighConfidenceFamilyCount"],
            2
        );
        assert_eq!(
            json["runtimeSignals"]["correlationFamilyCoverage"][0]["family"],
            "LNK"
        );
        assert_eq!(
            json["runtimeSignals"]["correlationFamilyCoverage"][0]["status"],
            "covered"
        );
        assert_eq!(
            json["runtimeSignals"]["correlationFamilyCoverage"][0]["sampleSignals"][0],
            "LNK 目标路径命中文件路径"
        );
        assert!(json.get("generated_at").is_none());
        assert!(json["supportMatrixEntries"][0]
            .get("verified_samples")
            .is_none());
        assert!(json["errorTaxonomyEntries"][0]
            .get("redaction_rule")
            .is_none());
        assert!(json["releaseGates"][0].get("gate_id").is_none());
    }
}
