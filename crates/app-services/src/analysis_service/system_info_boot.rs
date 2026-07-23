//! EVTX boot/shutdown inspection for the system-info summary, split from
//! `system_info.rs` to keep both modules within the size guard limits.

use super::provenance::{entry_provenance, EVTX_BOOT_SHUTDOWN_PARSER};
use super::system_info::{push_unavailable_provenance, AnalysisProvenanceContext};
use domain::{FileEntry, FileEntryId};
use transport::dto::analysis::{AnalysisBootRecordDto, AnalysisProvenanceDto};
use transport::dto::AnalysisParseStatusDto;

pub(super) fn inspect_evtx_boot_source<E: std::fmt::Display>(
    entry: Option<&FileEntry>,
    context: &AnalysisProvenanceContext<'_>,
    read_header_fn: &mut impl FnMut(&FileEntryId, usize) -> Result<Vec<u8>, E>,
    warnings: &mut Vec<String>,
    provenance: &mut Vec<AnalysisProvenanceDto>,
    boot_history: &mut Vec<AnalysisBootRecordDto>,
) {
    match entry {
        Some(entry) => {
            let mut parser_warnings = Vec::new();
            let mut parsed_any = false;
            match read_header_fn(&entry.id, artifacts_windows::MAX_EVTX_ANALYSIS_BYTES) {
                Ok(bytes) => {
                    match artifacts_windows::extract_boot_shutdown_events(&bytes, &entry.path) {
                        Ok(extraction) => {
                            parser_warnings.extend(extraction.warnings);
                            if !extraction.events.is_empty() {
                                parsed_any = true;
                            }
                            let event_provenance = entry_provenance(
                                entry,
                                EVTX_BOOT_SHUTDOWN_PARSER,
                                context.parsed_at,
                                AnalysisParseStatusDto::Parsed,
                                Vec::new(),
                            );
                            boot_history.extend(extraction.events.into_iter().map(|event| {
                                AnalysisBootRecordDto {
                                    timestamp: event.timestamp,
                                    boot_type: event.kind.as_str().to_string(),
                                    source: event.source_path,
                                    event_id: Some(event.event_id),
                                    record_id: event.record_id,
                                    note: Some(event.note),
                                    details: event.details,
                                    provenance: event_provenance.clone(),
                                }
                            }));
                        }
                        Err(err) => parser_warnings.push(err.to_string()),
                    }
                }
                Err(err) => parser_warnings.push(format!("{} 读取失败: {}", entry.path, err)),
            }
            provenance.push(entry_provenance(
                entry,
                EVTX_BOOT_SHUTDOWN_PARSER,
                context.parsed_at,
                if parsed_any {
                    AnalysisParseStatusDto::Parsed
                } else {
                    AnalysisParseStatusDto::NotParsed
                },
                parser_warnings,
            ));
        }
        None => {
            let artifact_path = "Windows/System32/winevt/Logs/System.evtx";
            let warning = format!(
                "未在证据文件目录中发现 {}；EVTX boot/shutdown 解析不可用。",
                artifact_path
            );
            warnings.push(warning.clone());
            push_unavailable_provenance(
                provenance,
                context.data_source_id,
                artifact_path,
                EVTX_BOOT_SHUTDOWN_PARSER,
                context.parsed_at,
                warning,
            );
        }
    }
}
