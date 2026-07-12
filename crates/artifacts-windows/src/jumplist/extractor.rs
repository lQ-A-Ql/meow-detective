use super::embedded_lnk::extract_lnk_blocks;
use artifacts_core::{
    new_artifact, ArtifactContext, ArtifactExtractor, ArtifactSink, ExtractorReport,
};
use domain::ArtifactFamily;
use std::collections::BTreeMap;
use std::io::Read;

pub struct JumpListExtractor;

impl ArtifactExtractor for JumpListExtractor {
    fn id(&self) -> &'static str {
        "jumplist"
    }

    fn display_name(&self) -> &'static str {
        "Windows Jump List Parser"
    }

    fn family(&self) -> ArtifactFamily {
        ArtifactFamily {
            name: "JumpList".into(),
            description: Some(
                "Windows Jump List entries (.ms-* and .customDestinations-ms)".into(),
            ),
        }
    }

    fn supports_path(&self, file_path: &str) -> bool {
        let lower = file_path.to_lowercase();
        lower.ends_with(".customdestinations-ms")
            || (lower.contains(".ms-") && !lower.contains(".msg"))
    }

    fn run(
        &self,
        mut ctx: ArtifactContext,
        sink: &mut dyn ArtifactSink,
    ) -> Result<ExtractorReport, String> {
        let mut data = Vec::new();
        ctx.reader
            .read_to_end(&mut data)
            .map_err(|error| error.to_string())?;
        if data.len() < 4 {
            return Ok(empty_report());
        }
        let blocks = extract_lnk_blocks(&data);
        if blocks.is_empty() {
            write_summary(&ctx, sink, data.len(), 0);
            return Ok(report(1, 0));
        }

        let (artifacts, events) = extract_embedded_links(&ctx, sink, &blocks);
        write_summary(&ctx, sink, data.len(), blocks.len());
        Ok(report(artifacts + 1, events))
    }
}

fn extract_embedded_links(
    ctx: &ArtifactContext,
    sink: &mut dyn ArtifactSink,
    blocks: &[Vec<u8>],
) -> (u32, u32) {
    let extractor = crate::lnk::parser::LnkExtractor;
    let mut artifacts = 0;
    let mut events = 0;
    for (index, data) in blocks.iter().enumerate() {
        let lnk_ctx = ArtifactContext {
            file_id: domain::FileEntryId(format!("{}:lnk:{index}", ctx.file_id.0)),
            file_path: format!("{}:lnk[{index}]", ctx.file_path),
            reader: Box::new(std::io::Cursor::new(data.clone())),
        };
        match extractor.run(lnk_ctx, sink) {
            Ok(result) => {
                artifacts += result.artifacts_found;
                events += result.timeline_events;
            }
            Err(error) => tracing::warn!("Failed to parse LNK block {}: {}", index, error),
        }
    }
    (artifacts, events)
}

fn write_summary(
    ctx: &ArtifactContext,
    sink: &mut dyn ArtifactSink,
    file_size: usize,
    lnk_count: usize,
) {
    let mut attrs = BTreeMap::new();
    attrs.insert("file_size".into(), (file_size as u64).into());
    attrs.insert("format".into(), "AutomaticDestinations".into());
    if lnk_count > 0 {
        attrs.insert("lnk_count".into(), (lnk_count as u64).into());
    }
    let summary = if lnk_count == 0 {
        format!("Jump List file, {file_size} bytes")
    } else {
        format!("Jump List with {lnk_count} embedded shortcuts")
    };
    sink.write_artifact(new_artifact(
        "JumpList",
        format!("JumpList: {}", ctx.file_path),
        summary,
        Some(&ctx.file_id),
        attrs,
    ));
}

fn empty_report() -> ExtractorReport {
    report(0, 0)
}

fn report(artifacts_found: u32, timeline_events: u32) -> ExtractorReport {
    ExtractorReport {
        artifacts_found,
        timeline_events,
        errors: Vec::new(),
    }
}
