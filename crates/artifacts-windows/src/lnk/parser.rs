//! Windows Shell Link (.lnk) artifact extractor.

use super::decode::{decode_lnk, DecodedLnk};
use artifacts_core::{
    new_artifact, new_timeline_event, ArtifactContext, ArtifactExtractor, ArtifactSink,
    ExtractorReport,
};
use domain::ArtifactFamily;
use std::collections::BTreeMap;

pub struct LnkExtractor;

impl ArtifactExtractor for LnkExtractor {
    fn id(&self) -> &'static str {
        "lnk"
    }

    fn display_name(&self) -> &'static str {
        "Windows LNK Shortcut Parser"
    }

    fn family(&self) -> ArtifactFamily {
        ArtifactFamily {
            name: "LNK".into(),
            description: Some("Windows Shell Link (.lnk)".into()),
        }
    }

    fn supports_path(&self, file_path: &str) -> bool {
        file_path.to_lowercase().ends_with(".lnk")
    }

    fn run(
        &self,
        mut ctx: ArtifactContext,
        sink: &mut dyn ArtifactSink,
    ) -> Result<ExtractorReport, String> {
        let decoded = decode_lnk(&mut ctx.reader)?;
        Ok(write_decoded_link(&ctx, decoded, None, sink))
    }
}

pub(crate) fn write_decoded_link(
    ctx: &ArtifactContext,
    decoded: DecodedLnk,
    container_entry: Option<&str>,
    sink: &mut dyn ArtifactSink,
) -> ExtractorReport {
    let mut attrs = BTreeMap::from([("file_size".into(), decoded.file_size.into())]);
    if let Some(entry) = container_entry {
        attrs.insert("containerEntry".into(), entry.into());
    }
    if !decoded.target_path.is_empty() {
        attrs.insert("target_path".into(), decoded.target_path.clone().into());
    }
    let mut timeline_events = 0;
    for timestamp in decoded.timestamps {
        attrs.insert(timestamp.field.into(), timestamp.value.to_rfc3339().into());
        sink.write_timeline_event(new_timeline_event(
            &ctx.file_id,
            timestamp.event_type,
            timestamp.value,
            format!("LNK time: {}", ctx.file_path),
            format!(
                "{} at {}",
                timestamp.event_type,
                timestamp.value.to_rfc3339()
            ),
            attrs.clone(),
        ));
        timeline_events += 1;
    }
    let summary = if decoded.target_path.is_empty() {
        format!("Shell link, {} bytes", decoded.file_size)
    } else {
        format!("Shortcut → {}", decoded.target_path)
    };
    sink.write_artifact(new_artifact(
        "LNK",
        format!("LNK: {}", ctx.file_path),
        summary,
        Some(&ctx.file_id),
        attrs,
    ));
    ExtractorReport {
        artifacts_found: 1,
        timeline_events,
        errors: Vec::new(),
    }
}
