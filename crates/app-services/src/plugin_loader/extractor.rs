//! `PluginExtractor`: adapts a loaded plugin DLL to `ArtifactExtractor`
//! (design doc §5.5). Calls are serialized per plugin (Mutex) and wrapped in
//! `catch_unwind`; provenance fields are host-enforced, never plugin-reported.

use super::library::PluginLibrary;
use artifacts_core::{ArtifactContext, ArtifactExtractor, ArtifactSink, ExtractorReport};
use domain::ArtifactFamily;
use plugin_api::{MeowExtractRequest, MeowExtractResponse, MeowStatus};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::ffi::CString;
use std::io::Read as _;
use std::sync::Mutex;

const ARTIFACT_FILE_LIMIT_BYTES: u64 = infrastructure::constants::ARTIFACT_FILE_LIMIT_BYTES;

/// Validated plugin metadata, copied out of the DLL at load time.
pub(crate) struct PluginMeta {
    pub plugin_id: String,
    pub version: String,
    pub display_name: String,
    pub families: Vec<String>,
    pub patterns: Vec<PathPattern>,
}

/// Path match pattern from `path_patterns_json` (design doc §3): `*.pf`
/// suffix semantics, otherwise an exact file name match. Both
/// case-insensitive, aligned with the built-in extractors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PathPattern {
    Suffix(String),
    ExactName(String),
}

impl PathPattern {
    fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        if let Some(suffix) = trimmed.strip_prefix('*') {
            if suffix.is_empty() {
                None
            } else {
                Some(Self::Suffix(suffix.to_lowercase()))
            }
        } else if trimmed.is_empty() {
            None
        } else {
            Some(Self::ExactName(trimmed.to_lowercase()))
        }
    }

    fn matches(&self, file_path: &str) -> bool {
        match self {
            Self::Suffix(suffix) => file_path.to_lowercase().ends_with(suffix.as_str()),
            Self::ExactName(name) => file_path
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(file_path)
                .eq_ignore_ascii_case(name),
        }
    }
}

pub(crate) fn parse_path_patterns(json: &str) -> Result<Vec<PathPattern>, String> {
    let raw: Vec<String> = serde_json::from_str(json)
        .map_err(|error| format!("path_patterns_json is not a JSON string array: {error}"))?;
    Ok(raw.iter().filter_map(|p| PathPattern::parse(p)).collect())
}

pub(crate) fn parse_families(json: &str) -> Result<Vec<String>, String> {
    serde_json::from_str(json)
        .map_err(|error| format!("families_json is not a JSON string array: {error}"))
}

/// An `ArtifactExtractor` backed by a plugin DLL.
pub struct PluginExtractor {
    id: &'static str,
    display_name: &'static str,
    version: String,
    primary_family: String,
    families: HashSet<String>,
    patterns: Vec<PathPattern>,
    library: PluginLibrary,
    call_lock: Mutex<()>,
}

impl PluginExtractor {
    pub(crate) fn new(meta: PluginMeta, library: PluginLibrary) -> Self {
        // Plugin metadata lives as long as the loaded DLL; leak it into
        // 'static to satisfy the ArtifactExtractor signature. Plugins load
        // once per process and are never unloaded, so this does not grow.
        let id: &'static str = Box::leak(meta.plugin_id.into_boxed_str());
        let display_name: &'static str = Box::leak(meta.display_name.into_boxed_str());
        let primary_family = meta
            .families
            .first()
            .cloned()
            .unwrap_or_else(|| id.to_string());
        Self {
            id,
            display_name,
            version: meta.version,
            primary_family,
            families: meta.families.into_iter().collect(),
            patterns: meta.patterns,
            library,
            call_lock: Mutex::new(()),
        }
    }

    fn call_extract(
        &self,
        ctx: &ArtifactContext,
        data: &[u8],
    ) -> Result<MeowExtractResponse, String> {
        let file_path = CString::new(ctx.file_path.as_str())
            .map_err(|_| format!("plugin {} path is not UTF-8-clean", self.id))?;
        let file_id = CString::new(ctx.file_id.0.as_str())
            .map_err(|_| format!("plugin {} file id is not UTF-8-clean", self.id))?;
        let request = MeowExtractRequest {
            struct_size: std::mem::size_of::<MeowExtractRequest>() as u32,
            file_path: file_path.as_ptr().cast(),
            file_id: file_id.as_ptr().cast(),
            data: data.as_ptr(),
            data_len: data.len() as u64,
        };
        let extract = self.library.extract_fn();
        // SAFETY: every request pointer references host-owned buffers that
        // outlive the call; the contract forbids the plugin from retaining
        // them. catch_unwind keeps a plugin panic from killing the batch.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
            extract(&request)
        }));
        outcome.map_err(|_| format!("plugin {} panicked during extract", self.id))
    }

    fn handle_response(
        &self,
        response: MeowExtractResponse,
        ctx: &ArtifactContext,
        sink: &mut dyn ArtifactSink,
    ) -> Result<ExtractorReport, String> {
        let buffers = self.take_buffers(&response);
        if response.status != MeowStatus::Ok {
            let detail = buffers
                .error
                .unwrap_or_else(|| "no error message".to_string());
            return Err(format!(
                "plugin {} returned {:?}: {}",
                self.id, response.status, detail
            ));
        }
        let payload = buffers
            .payload
            .ok_or_else(|| format!("plugin {} returned Ok without a payload", self.id))?;
        let text = String::from_utf8(payload)
            .map_err(|_| format!("plugin {} payload is not UTF-8", self.id))?;
        let parsed: PluginPayload = serde_json::from_str(&text)
            .map_err(|error| format!("plugin {} payload is not valid JSON: {error}", self.id))?;
        Ok(self.write_payload(parsed, ctx, sink))
    }

    fn take_buffers(&self, response: &MeowExtractResponse) -> ResponseBuffers {
        ResponseBuffers {
            payload: self.take_payload(response),
            error: self.take_error_message(response),
        }
    }

    fn take_payload(&self, response: &MeowExtractResponse) -> Option<Vec<u8>> {
        if response.payload.is_null() || response.payload_len == 0 {
            return None;
        }
        // SAFETY: contract §3 — payload is plugin-allocated with payload_len
        // bytes; we copy it, then return the exact pointer/length pair to the
        // plugin's own free function (who allocates, frees).
        let bytes =
            unsafe { std::slice::from_raw_parts(response.payload, response.payload_len as usize) }
                .to_vec();
        // SAFETY: exact pointer/length pair handed to us by the plugin above.
        unsafe { (self.library.free_buffer_fn())(response.payload, response.payload_len) };
        Some(bytes)
    }

    fn take_error_message(&self, response: &MeowExtractResponse) -> Option<String> {
        if response.error_message.is_null() {
            return None;
        }
        // SAFETY: contract — error_message is NUL-terminated and
        // plugin-allocated; the freed length covers the NUL terminator the
        // plugin allocated.
        let (text, len) = unsafe {
            let message = std::ffi::CStr::from_ptr(response.error_message.cast());
            (
                message.to_string_lossy().into_owned(),
                message.to_bytes_with_nul().len() as u64,
            )
        };
        // SAFETY: exact pointer/length pair allocated by the plugin.
        unsafe { (self.library.free_buffer_fn())(response.error_message, len) };
        Some(text)
    }

    fn write_payload(
        &self,
        payload: PluginPayload,
        ctx: &ArtifactContext,
        sink: &mut dyn ArtifactSink,
    ) -> ExtractorReport {
        let mut report = ExtractorReport {
            artifacts_found: 0,
            timeline_events: 0,
            errors: payload.warnings,
        };
        for warning in &report.errors {
            tracing::warn!("plugin {}: {}", self.id, warning);
        }
        for artifact in payload.artifacts {
            self.write_artifact(artifact, ctx, sink, &mut report);
        }
        for event in payload.timeline_events {
            self.write_timeline_event(event, ctx, sink, &mut report);
        }
        report
    }

    fn write_artifact(
        &self,
        parsed: PayloadArtifact,
        ctx: &ArtifactContext,
        sink: &mut dyn ArtifactSink,
        report: &mut ExtractorReport,
    ) {
        if !self.families.contains(&parsed.family) {
            let warning = format!(
                "plugin {} emitted undeclared family '{}'; artifact dropped",
                self.id, parsed.family
            );
            tracing::warn!("{warning}");
            report.errors.push(warning);
            return;
        }
        let mut artifact = artifacts_core::new_artifact(
            &parsed.family,
            parsed.title,
            parsed.summary,
            Some(&ctx.file_id),
            parsed.attrs,
        );
        // Host-enforced provenance (contract §4/§5.5): plugin-reported
        // provenance is never trusted.
        artifact.source_object_id = Some(ctx.file_id.clone());
        artifact.extractor_id = Some(self.id.to_string());
        artifact.extractor_version = Some(self.version.clone());
        artifact.source_attribution = Some(ctx.file_path.clone());
        artifact.confidence = parsed.confidence;
        sink.write_artifact(artifact);
        report.artifacts_found += 1;
    }

    fn write_timeline_event(
        &self,
        parsed: PayloadTimelineEvent,
        ctx: &ArtifactContext,
        sink: &mut dyn ArtifactSink,
        report: &mut ExtractorReport,
    ) {
        let timestamp = chrono::DateTime::parse_from_rfc3339(&parsed.timestamp_utc)
            .map(|ts| ts.with_timezone(&chrono::Utc));
        let Ok(timestamp) = timestamp else {
            let warning = format!(
                "plugin {} emitted invalid timestampUtc '{}'; event dropped",
                self.id, parsed.timestamp_utc
            );
            tracing::warn!("{warning}");
            report.errors.push(warning);
            return;
        };
        let mut event = artifacts_core::new_timeline_event(
            &ctx.file_id,
            &parsed.event_type,
            timestamp,
            parsed.event_type.clone(),
            parsed.description,
            parsed.attrs,
        );
        // Host-enforced provenance, same as artifacts.
        event.parser_id = Some(self.id.to_string());
        event.parser_version = Some(self.version.clone());
        event.source_attribution = Some(ctx.file_path.clone());
        sink.write_timeline_event(event);
        report.timeline_events += 1;
    }
}

impl ArtifactExtractor for PluginExtractor {
    fn id(&self) -> &'static str {
        self.id
    }

    fn display_name(&self) -> &'static str {
        self.display_name
    }

    fn family(&self) -> ArtifactFamily {
        ArtifactFamily {
            name: self.primary_family.clone(),
            description: Some(format!(
                "{} v{} (DLL plugin)",
                self.display_name, self.version
            )),
        }
    }

    fn supports_path(&self, file_path: &str) -> bool {
        self.patterns
            .iter()
            .any(|pattern| pattern.matches(file_path))
    }

    fn run(
        &self,
        mut ctx: ArtifactContext,
        sink: &mut dyn ArtifactSink,
    ) -> Result<ExtractorReport, String> {
        // The contract serializes calls per plugin (§3): plugins need not be
        // internally thread-safe.
        let _serial = self
            .call_lock
            .lock()
            .map_err(|_| format!("plugin {} call lock poisoned", self.id))?;
        let mut data = Vec::new();
        ctx.reader
            .by_ref()
            .take(ARTIFACT_FILE_LIMIT_BYTES)
            .read_to_end(&mut data)
            .map_err(|error| format!("plugin {} failed to read input: {error}", self.id))?;
        let response = self.call_extract(&ctx, &data)?;
        self.handle_response(response, &ctx, sink)
    }
}

struct ResponseBuffers {
    payload: Option<Vec<u8>>,
    error: Option<String>,
}

/// Extraction payload schema (design doc §4). Unknown fields are ignored so
/// the payload can evolve without an ABI bump.
#[derive(Debug, Deserialize)]
struct PluginPayload {
    #[serde(default)]
    artifacts: Vec<PayloadArtifact>,
    #[serde(default, rename = "timelineEvents")]
    timeline_events: Vec<PayloadTimelineEvent>,
    #[serde(default)]
    warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct PayloadArtifact {
    family: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    summary: String,
    confidence: Option<f32>,
    #[serde(default)]
    attrs: BTreeMap<String, Value>,
}

#[derive(Debug, Deserialize)]
struct PayloadTimelineEvent {
    #[serde(rename = "timestampUtc")]
    timestamp_utc: String,
    #[serde(rename = "eventType")]
    event_type: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    attrs: BTreeMap<String, Value>,
}
