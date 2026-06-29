use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::dto::analysis_base::AnalysisParseStatusDto;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvtxEventSummaryDto {
    pub status: AnalysisParseStatusDto,
    pub boot_shutdown_count: u64,
    pub logon_logoff_count: u64,
    pub privilege_escalation_count: u64,
    pub process_execution_count: u64,
    pub account_management_count: u64,
    pub scheduled_task_count: u64,
    pub application_crash_count: u64,
    pub software_installation_count: u64,
    pub other_count: u64,
    pub total_count: u64,
    pub boot_events: Vec<EvtxBootEventDto>,
    pub security_events: Vec<EvtxSecurityEventDto>,
    pub application_events: Vec<EvtxApplicationEventDto>,
    pub generated_at: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvtxBootEventDto {
    pub timestamp: String,
    pub event_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    pub kind: String,
    pub source_path: String,
    pub note: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub details: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvtxSecurityEventDto {
    pub timestamp: String,
    pub event_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    pub kind: String,
    pub source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject_user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logon_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workstation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_process_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privilege_list: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_name: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub details: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvtxApplicationEventDto {
    pub timestamp: String,
    pub event_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    pub kind: String,
    pub source_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fault_module: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub details: BTreeMap<String, String>,
}
