//! Windows Prefetch artifact extractor.

use super::decode::decode_prefetch;
use super::payload::decode_prefetch_payload;
use artifacts_core::{
    new_artifact, new_timeline_event, ArtifactContext, ArtifactExtractor, ArtifactSink,
    ExtractorReport,
};
use domain::ArtifactFamily;
use std::collections::BTreeMap;
use std::io::Read;

pub struct PrefetchExtractor;

impl ArtifactExtractor for PrefetchExtractor {
    fn id(&self) -> &'static str {
        "prefetch"
    }

    fn display_name(&self) -> &'static str {
        "Windows Prefetch Parser (v30)"
    }

    fn family(&self) -> ArtifactFamily {
        ArtifactFamily {
            name: "Prefetch".into(),
            description: Some("Windows Prefetch files (.pf) v30".into()),
        }
    }

    fn supports_path(&self, file_path: &str) -> bool {
        file_path.to_lowercase().ends_with(".pf")
    }

    fn run(
        &self,
        mut ctx: ArtifactContext,
        sink: &mut dyn ArtifactSink,
    ) -> Result<ExtractorReport, String> {
        let mut raw = Vec::new();
        ctx.reader
            .read_to_end(&mut raw)
            .map_err(|error| error.to_string())?;
        let data = match decode_prefetch_payload(raw) {
            Ok(data) => data,
            Err(error) if error == "Not a Prefetch file" => return Ok(report(0, 0, Vec::new())),
            Err(error) => return Ok(report(0, 0, vec![error])),
        };
        let decoded = match decode_prefetch(&data) {
            Ok(decoded) => decoded,
            Err(error) => return Ok(report(0, 0, vec![error])),
        };
        let mut attrs = BTreeMap::from([
            ("format_version".into(), decoded.format_version.into()),
            ("executable".into(), decoded.executable.clone().into()),
            ("run_count".into(), decoded.run_count.into()),
            ("hash".into(), format!("{:08X}", decoded.hash).into()),
            ("file_size".into(), decoded.file_size.into()),
        ]);
        attrs.insert(
            "last_run_times".into(),
            decoded
                .run_times
                .iter()
                .map(|time| time.to_rfc3339().into())
                .collect::<Vec<serde_json::Value>>()
                .into(),
        );
        sink.write_artifact(new_artifact(
            "Prefetch",
            format!("Prefetch: {}", decoded.executable),
            format!(
                "{} executed {} times (fmt v{})",
                decoded.executable, decoded.run_count, decoded.format_version
            ),
            Some(&ctx.file_id),
            attrs,
        ));
        write_timeline_events(&ctx, sink, &decoded.executable, &decoded.run_times);
        Ok(report(1, decoded.run_times.len() as u32, Vec::new()))
    }
}

fn write_timeline_events(
    ctx: &ArtifactContext,
    sink: &mut dyn ArtifactSink,
    executable: &str,
    run_times: &[chrono::DateTime<chrono::Utc>],
) {
    for timestamp in run_times {
        sink.write_timeline_event(new_timeline_event(
            &ctx.file_id,
            "PROGRAM_EXECUTION",
            *timestamp,
            format!("{executable} executed"),
            format!("Prefetch run at {}", timestamp.to_rfc3339()),
            BTreeMap::from([("executable".into(), executable.to_string().into())]),
        ));
    }
}

fn report(artifacts: u32, events: u32, errors: Vec<String>) -> ExtractorReport {
    ExtractorReport {
        artifacts_found: artifacts,
        timeline_events: events,
        errors,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/prefetch.rs"]
mod tests;
